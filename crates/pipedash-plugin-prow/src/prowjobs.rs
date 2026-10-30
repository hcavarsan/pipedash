use std::path::Path;
use std::time::Duration;

use chrono::{
    DateTime,
    SecondsFormat,
    Utc,
};
use kube::config::{
    KubeConfigOptions,
    Kubeconfig,
};
use kube::core::params::{
    GetParams,
    Patch,
    PatchParams,
    PostParams,
};
use kube::core::Request;
use pipedash_plugin_api::{
    from_status,
    PluginError,
    PluginResult,
};
use serde::de::DeserializeOwned;
use serde_json::{
    json,
    Value,
};

use crate::config::KubeSettings;

const FIELD_MANAGER: &str = "pipedash";
const BUILD_ID_LABEL: &str = "prow.k8s.io/build-id";
const STARTED: &str = "Started again from Pipedash.";
const ABORTED: &str = "Aborted from Pipedash.";

pub(crate) struct ProwJobClient {
    client: kube::Client,
}

fn read_kubeconfigs(paths: &[String]) -> PluginResult<Kubeconfig> {
    let mut merged: Option<Kubeconfig> = None;

    for path in paths.iter().filter(|path| Path::new(path).exists()) {
        let kubeconfig = Kubeconfig::read_from(path).map_err(|error| {
            PluginError::InvalidConfig(format!("Failed to read kubeconfig {path}: {error}"))
        })?;
        merged = Some(match merged {
            Some(existing) => existing.merge(kubeconfig).map_err(|error| {
                PluginError::InvalidConfig(format!("Failed to merge kubeconfig {path}: {error}"))
            })?,
            None => kubeconfig,
        });
    }

    merged.ok_or_else(|| {
        PluginError::InvalidConfig(format!("No kubeconfig file found in: {}", paths.join(", ")))
    })
}

fn blocking_failed(error: tokio::task::JoinError) -> PluginError {
    PluginError::Internal(format!("Kubeconfig loader task failed: {error}"))
}

fn map_error(error: kube::Error, context: &str) -> PluginError {
    match error {
        kube::Error::Api(status) => {
            let retry_after = status
                .details
                .as_ref()
                .map(|details| details.retry_after_seconds)
                .filter(|seconds| *seconds > 0)
                .map(|seconds| Duration::from_secs(seconds.into()));
            let detail = format!("{context}: {} (HTTP {})", status.message, status.code);

            from_status(status.code, detail, retry_after)
        }
        kube::Error::Auth(error) => {
            PluginError::AuthenticationFailed(format!("{context}: {error}"))
        }
        kube::Error::SerdeError(error) => {
            PluginError::SerializationError(format!("{context}: {error}"))
        }
        other => PluginError::NetworkError(format!("{context}: {other}")),
    }
}

fn prowjobs(namespace: &str) -> Request {
    Request::new(format!(
        "/apis/prow.k8s.io/v1/namespaces/{namespace}/prowjobs"
    ))
}

pub(crate) fn rerun_object(original: &Value, now: DateTime<Utc>) -> PluginResult<Value> {
    let spec = original
        .get("spec")
        .filter(|spec| spec.is_object())
        .ok_or_else(|| PluginError::SerializationError("The ProwJob has no spec".to_string()))?;
    let mut labels = original["metadata"]["labels"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    labels.remove(BUILD_ID_LABEL);
    let annotations = original["metadata"]["annotations"]
        .as_object()
        .cloned()
        .unwrap_or_default();

    Ok(json!({
        "apiVersion": "prow.k8s.io/v1",
        "kind": "ProwJob",
        "metadata": {
            "generateName": "pipedash-",
            "labels": labels,
            "annotations": annotations,
        },
        "spec": spec,
        "status": {
            "startTime": now.to_rfc3339_opts(SecondsFormat::Secs, true),
            "state": "triggered",
            "description": STARTED,
        },
    }))
}

impl ProwJobClient {
    pub(crate) async fn contexts(paths: Vec<String>) -> PluginResult<Vec<String>> {
        tokio::task::spawn_blocking(move || {
            let mut contexts: Vec<String> = read_kubeconfigs(&paths)?
                .contexts
                .into_iter()
                .map(|context| context.name)
                .collect();
            if contexts.is_empty() {
                return Err(PluginError::InvalidConfig(format!(
                    "No contexts found in kubeconfig: {}",
                    paths.join(", ")
                )));
            }
            contexts.sort();
            contexts.dedup();
            Ok(contexts)
        })
        .await
        .map_err(blocking_failed)?
    }

    pub(crate) async fn connect(settings: &KubeSettings) -> PluginResult<Self> {
        let paths = settings.paths.clone();
        let context = settings.context.clone();
        let runtime = tokio::runtime::Handle::current();
        let client = tokio::task::spawn_blocking(move || {
            let kubeconfig = read_kubeconfigs(&paths)?;
            let options = KubeConfigOptions {
                context,
                ..Default::default()
            };
            let mut config = runtime
                .block_on(kube::Config::from_custom_kubeconfig(kubeconfig, &options))
                .map_err(|error| {
                    PluginError::InvalidConfig(format!("Failed to load kubeconfig: {error}"))
                })?;
            config.default_retry = false;

            kube::Client::try_from(config).map_err(|error| {
                PluginError::InvalidConfig(format!("Failed to create Kubernetes client: {error}"))
            })
        })
        .await
        .map_err(blocking_failed)??;

        Ok(Self { client })
    }

    async fn exec<T: DeserializeOwned>(
        &self, request: Result<http::Request<Vec<u8>>, kube::core::request::Error>, context: &str,
    ) -> PluginResult<T> {
        let request =
            request.map_err(|error| PluginError::InvalidConfig(format!("{context}: {error}")))?;

        self.client
            .request(request)
            .await
            .map_err(|error| map_error(error, context))
    }

    pub(crate) async fn start_again(&self, namespace: &str, name: &str) -> PluginResult<String> {
        let request = prowjobs(namespace);
        let original: Value = self
            .exec(
                request.get(name, &GetParams::default()),
                &format!("read ProwJob {name}"),
            )
            .await?;
        let object = rerun_object(&original, Utc::now())?;
        let body = serde_json::to_vec(&object)
            .map_err(|error| PluginError::SerializationError(error.to_string()))?;
        let params = PostParams {
            field_manager: Some(FIELD_MANAGER.to_string()),
            ..Default::default()
        };
        let created: Value = self
            .exec(
                request.create(&params, body),
                &format!("start ProwJob {name} again"),
            )
            .await?;

        created["metadata"]["name"]
            .as_str()
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                PluginError::ApiError("Kubernetes created the ProwJob without a name".to_string())
            })
    }

    pub(crate) async fn abort(&self, namespace: &str, name: &str) -> PluginResult<()> {
        let request = prowjobs(namespace);
        let patch = Patch::Merge(json!({
            "status": { "state": "aborted", "description": ABORTED }
        }));
        let params = PatchParams {
            field_manager: Some(FIELD_MANAGER.to_string()),
            ..Default::default()
        };
        let context = format!("abort ProwJob {name}");
        let patched: Value = self
            .exec(request.patch(name, &params, &patch), &context)
            .await?;
        if patched["status"]["state"] == "aborted" {
            return Ok(());
        }
        self.exec::<Value>(
            request.patch_subresource("status", name, &params, &patch),
            &context,
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn rerun_object_copies_spec_and_drops_build_id() {
        let original = json!({
            "apiVersion": "prow.k8s.io/v1",
            "kind": "ProwJob",
            "metadata": {
                "name": "abc",
                "namespace": "default",
                "resourceVersion": "42",
                "labels": { "prow.k8s.io/build-id": "123", "prow.k8s.io/job": "ci-x" },
                "annotations": { "prow.k8s.io/job": "ci-x" }
            },
            "spec": { "job": "ci-x", "type": "periodic" },
            "status": { "state": "success", "build_id": "123" }
        });
        let now = Utc.with_ymd_and_hms(2026, 10, 30, 12, 0, 0).unwrap();

        let object = rerun_object(&original, now).unwrap();

        assert_eq!(object["metadata"]["generateName"], "pipedash-");
        assert!(object["metadata"].get("name").is_none());
        assert!(object["metadata"].get("resourceVersion").is_none());
        assert_eq!(
            object["metadata"]["labels"],
            json!({ "prow.k8s.io/job": "ci-x" })
        );
        assert_eq!(object["metadata"]["annotations"]["prow.k8s.io/job"], "ci-x");
        assert_eq!(object["spec"], original["spec"]);
        assert_eq!(object["status"]["state"], "triggered");
        assert_eq!(object["status"]["startTime"], "2026-10-30T12:00:00Z");
        assert!(object["status"].get("build_id").is_none());

        assert!(rerun_object(&json!({ "metadata": {} }), now).is_err());
    }
}
