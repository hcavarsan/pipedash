use chrono::{
    DateTime,
    SecondsFormat,
    Utc,
};
use kube::core::params::{
    GetParams,
    Patch,
    PatchParams,
    PostParams,
};
use kube::core::Request;
use pipedash_plugin_api::kubernetes::{
    self,
    connect,
    map_error,
    read_kubeconfigs,
};
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use serde_json::{
    json,
    Value,
};

use crate::config::KubeSettings;

const FIELD_MANAGER: &str = "pipedash";
const BUILD_ID_LABEL: &str = "prow.k8s.io/build-id";
const STARTED: &str = "Started again from Pipedash.";
const ABORTED: &str = "Aborted from Pipedash.";
const ACTIVE_STATES: [&str; 3] = ["scheduling", "triggered", "pending"];
const CONFLICT_ATTEMPTS: usize = 3;
const CONFLICT: u16 = 409;

pub(crate) struct ProwJobClient {
    client: kube::Client,
}

fn prowjobs(namespace: &str) -> Request {
    Request::new(format!(
        "/apis/prow.k8s.io/v1/namespaces/{namespace}/prowjobs"
    ))
}

fn is_conflict(error: &kube::Error) -> bool {
    matches!(error, kube::Error::Api(status) if status.code == CONFLICT)
}

fn abort_patch(object: &Value) -> PluginResult<Patch<Value>> {
    let version = object["metadata"]["resourceVersion"]
        .as_str()
        .filter(|version| !version.is_empty())
        .ok_or_else(|| {
            PluginError::SerializationError("The ProwJob has no resourceVersion".to_string())
        })?;
    Ok(Patch::Merge(json!({
        "metadata": { "resourceVersion": version },
        "status": { "state": "aborted", "description": ABORTED },
    })))
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
        let source = paths.join(", ");
        kubernetes::available_contexts(move || read_kubeconfigs(&paths), source).await
    }

    pub(crate) async fn connect(settings: &KubeSettings) -> PluginResult<Self> {
        let paths = settings.paths.clone();
        let client = connect(
            move || read_kubeconfigs(&paths),
            settings.context.clone(),
            false,
        )
        .await?;

        Ok(Self { client })
    }

    pub(crate) async fn start_again(&self, namespace: &str, name: &str) -> PluginResult<String> {
        let request = prowjobs(namespace);
        let original: Value = kubernetes::exec(
            &self.client,
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
        let created: Value = kubernetes::exec(
            &self.client,
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
        let params = PatchParams {
            field_manager: Some(FIELD_MANAGER.to_string()),
            ..Default::default()
        };
        let context = format!("abort ProwJob {name}");

        for _ in 0..CONFLICT_ATTEMPTS {
            let current: Value = kubernetes::exec(
                &self.client,
                request.get(name, &GetParams::default()),
                &format!("read ProwJob {name}"),
            )
            .await?;
            let state = current["status"]["state"].as_str().unwrap_or_default();
            if !ACTIVE_STATES.contains(&state) {
                return Err(PluginError::ApiError(format!(
                    "ProwJob {name} already finished ({state})"
                )));
            }

            let patched: Value = match kubernetes::send(
                &self.client,
                request.patch(name, &params, &abort_patch(&current)?),
                &context,
            )
            .await?
            {
                Ok(patched) => patched,
                Err(error) if is_conflict(&error) => continue,
                Err(error) => return Err(map_error(error, &context)),
            };
            if patched["status"]["state"] == "aborted" {
                return Ok(());
            }
            match kubernetes::send::<Value>(
                &self.client,
                request.patch_subresource("status", name, &params, &abort_patch(&patched)?),
                &context,
            )
            .await?
            {
                Ok(_) => return Ok(()),
                Err(error) if is_conflict(&error) => continue,
                Err(error) => return Err(map_error(error, &context)),
            }
        }
        Err(PluginError::ApiError(format!(
            "ProwJob {name} kept changing while Pipedash tried to abort it. Try again."
        )))
    }
}

#[cfg(test)]
mod tests;
