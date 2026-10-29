use std::{
    collections::HashMap,
    path::Path,
    time::Duration,
};

use kube::{
    config::{
        KubeConfigOptions,
        Kubeconfig,
    },
    core::{
        params::{
            GetParams,
            ListParams,
            Patch,
            PatchParams,
            PostParams,
        },
        Request,
    },
};
use pipedash_plugin_api::{
    from_status,
    PluginError,
    PluginResult,
};
use serde::{
    de::{
        DeserializeOwned,
        IgnoredAny,
    },
    Deserialize,
};
use serde_json::{
    json,
    Value,
};

use crate::{
    config::{
        KubeconfigSource,
        Target,
    },
    health::Deployment,
    types::{
        Event,
        FluxResource,
        Kind,
    },
    workloads::{
        self,
        WorkloadKind,
    },
};

const PAGE_LIMIT: u32 = 500;
const FIELD_MANAGER: &str = "pipedash";
pub(crate) const REQUESTED_AT_ANNOTATION: &str = "reconcile.fluxcd.io/requestedAt";

#[derive(Deserialize)]
struct ObjectList {
    #[serde(default)]
    items: Vec<Value>,
    #[serde(default)]
    metadata: ListMeta,
}

#[derive(Default, Deserialize)]
struct ListMeta {
    #[serde(rename = "continue")]
    continue_token: Option<String>,
}

#[derive(Deserialize)]
struct AccessReview {
    #[serde(default)]
    status: AccessStatus,
}

#[derive(Default, Deserialize)]
struct AccessStatus {
    #[serde(default)]
    allowed: bool,
}

pub(crate) struct FluxClient {
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

fn load_kubeconfig(source: &KubeconfigSource) -> PluginResult<Kubeconfig> {
    match source {
        KubeconfigSource::Content(content) => Kubeconfig::from_yaml(content).map_err(|_| {
            PluginError::InvalidConfig(
                "The pasted kubeconfig content is not valid kubeconfig YAML".to_string(),
            )
        }),
        KubeconfigSource::Paths(paths) => read_kubeconfigs(paths),
    }
}

const MOBILE_PLATFORM: bool = cfg!(any(target_os = "ios", target_os = "android"));

fn ensure_supported_auth(
    kubeconfig: &Kubeconfig, context: Option<&str>, mobile: bool,
) -> PluginResult<()> {
    if !mobile {
        return Ok(());
    }

    let uses_exec = context
        .or(kubeconfig.current_context.as_deref())
        .and_then(|name| {
            kubeconfig
                .contexts
                .iter()
                .find(|candidate| candidate.name == name)
        })
        .and_then(|named| named.context.as_ref()?.user.as_deref())
        .and_then(|user| {
            kubeconfig
                .auth_infos
                .iter()
                .find(|candidate| candidate.name == user)
        })
        .filter(|named| {
            named
                .auth_info
                .as_ref()
                .is_some_and(|auth_info| auth_info.exec.is_some())
        });

    match uses_exec {
        Some(named) => Err(PluginError::NotSupported(format!(
            "Kubernetes user '{}' authenticates through an exec credential plugin, which is not supported on this platform. Use a token or client certificate in the kubeconfig instead.",
            named.name
        ))),
        None => Ok(()),
    }
}

fn blocking_failed(error: tokio::task::JoinError) -> PluginError {
    PluginError::Internal(format!("Kubeconfig loader task failed: {error}"))
}

pub(crate) async fn available_contexts(source: KubeconfigSource) -> PluginResult<Vec<String>> {
    tokio::task::spawn_blocking(move || {
        let mut contexts: Vec<String> = load_kubeconfig(&source)?
            .contexts
            .into_iter()
            .map(|context| context.name)
            .collect();
        if contexts.is_empty() {
            return Err(PluginError::InvalidConfig(format!(
                "No contexts found in kubeconfig: {}",
                source.describe()
            )));
        }
        contexts.sort();
        contexts.dedup();

        Ok(contexts)
    })
    .await
    .map_err(blocking_failed)?
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

fn list_context(kind: Kind, namespace: Option<&str>) -> String {
    match namespace {
        Some(namespace) => format!("list {kind}s in namespace {namespace}"),
        None => format!("list {kind}s in all namespaces"),
    }
}

impl FluxClient {
    pub async fn connect(source: KubeconfigSource, context: Option<String>) -> PluginResult<Self> {
        let runtime = tokio::runtime::Handle::current();
        let client = tokio::task::spawn_blocking(move || {
            let kubeconfig = load_kubeconfig(&source)?;
            ensure_supported_auth(&kubeconfig, context.as_deref(), MOBILE_PLATFORM)?;
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

    async fn list_page(
        &self, request: &Request, limit: u32, token: Option<&str>, context: &str,
    ) -> PluginResult<ObjectList> {
        let mut params = ListParams::default().limit(limit);
        if let Some(token) = token {
            params = params.continue_token(token);
        }

        self.exec(request.list(&params), context).await
    }

    pub async fn list(
        &self, kind: Kind, namespace: Option<&str>,
    ) -> PluginResult<Option<Vec<FluxResource>>> {
        let context = list_context(kind, namespace);
        let request = Request::new(kind.collection_path(namespace));
        let mut resources = Vec::new();
        let mut token: Option<String> = None;

        loop {
            let page = match self
                .list_page(&request, PAGE_LIMIT, token.as_deref(), &context)
                .await
            {
                Ok(page) => page,
                Err(PluginError::PipelineNotFound(_)) if token.is_none() => return Ok(None),
                Err(error) => return Err(error),
            };
            for item in page.items {
                resources.push(FluxResource::from_value(kind, item)?);
            }
            match page
                .metadata
                .continue_token
                .filter(|token| !token.is_empty())
            {
                Some(next) => token = Some(next),
                None => return Ok(Some(resources)),
            }
        }
    }

    pub async fn is_served(&self, kind: Kind, namespace: Option<&str>) -> PluginResult<bool> {
        let request = Request::new(kind.collection_path(namespace));
        let context = list_context(kind, namespace);

        match self.list_page(&request, 1, None, &context).await {
            Ok(_) => Ok(true),
            Err(PluginError::PipelineNotFound(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    pub async fn get(&self, target: &Target) -> PluginResult<FluxResource> {
        let request = Request::new(target.kind.collection_path(Some(&target.namespace)));
        let value: Value = self
            .exec(
                request.get(&target.name, &GetParams::default()),
                &format!("get {target}"),
            )
            .await?;

        FluxResource::from_value(target.kind, value)
    }

    pub async fn request_reconcile(&self, target: &Target, requested_at: &str) -> PluginResult<()> {
        let request = Request::new(target.kind.collection_path(Some(&target.namespace)));
        let annotations = HashMap::from([(REQUESTED_AT_ANNOTATION, requested_at)]);
        let patch = Patch::Merge(json!({ "metadata": { "annotations": annotations } }));
        let params = PatchParams {
            field_manager: Some(FIELD_MANAGER.to_string()),
            ..Default::default()
        };

        self.exec::<IgnoredAny>(
            request.patch(&target.name, &params, &patch),
            &format!("request reconcile of {target}"),
        )
        .await?;

        Ok(())
    }

    async fn list_all(
        &self, request: &Request, mut params: ListParams, context: &str,
    ) -> PluginResult<Vec<Value>> {
        let mut items = Vec::new();

        loop {
            let page: ObjectList = self.exec(request.list(&params), context).await?;
            items.extend(page.items);
            match page
                .metadata
                .continue_token
                .filter(|token| !token.is_empty())
            {
                Some(next) => params.continue_token = Some(next),
                None => return Ok(items),
            }
        }
    }

    pub async fn events(&self, target: &Target) -> PluginResult<Vec<Event>> {
        let request = Request::new(format!("/api/v1/namespaces/{}/events", target.namespace));
        let selector = format!(
            "involvedObject.kind={},involvedObject.name={},involvedObject.namespace={}",
            target.kind, target.name, target.namespace
        );
        let context = format!("list events of {target}");
        let params = ListParams::default().limit(PAGE_LIMIT).fields(&selector);

        self.list_all(&request, params, &context)
            .await?
            .into_iter()
            .map(|item| {
                serde_json::from_value(item)
                    .map_err(|error| PluginError::SerializationError(format!("{context}: {error}")))
            })
            .collect()
    }

    pub async fn unhealthy_workloads(
        &self, owner: Kind, kind: WorkloadKind,
    ) -> PluginResult<Vec<(Target, String)>> {
        let request = Request::new(kind.collection_path());
        let params = ListParams::default()
            .limit(PAGE_LIMIT)
            .labels(&workloads::owner_selector(owner));
        let context = format!("list {}s managed by {owner}s", kind.as_str());

        self.list_all(&request, params, &context)
            .await?
            .into_iter()
            .filter_map(|item| workloads::finding(kind, owner, item).transpose())
            .collect()
    }

    pub async fn deployment(&self, namespace: &str, name: &str) -> PluginResult<Deployment> {
        let request = Request::new(format!("/apis/apps/v1/namespaces/{namespace}/deployments"));

        self.exec(
            request.get(name, &GetParams::default()),
            &format!("get deployment {namespace}/{name}"),
        )
        .await
    }

    pub async fn can_i(
        &self, group: &str, resource: &str, namespace: Option<&str>, verb: &str, name: Option<&str>,
    ) -> PluginResult<bool> {
        let scope = namespace.map_or_else(
            || "all namespaces".to_string(),
            |namespace| format!("namespace {namespace}"),
        );
        let target = name.map_or_else(|| resource.to_string(), |name| format!("{resource}/{name}"));
        let context = format!("check permission to {verb} {target} in {scope}");
        let body = serde_json::to_vec(&json!({
            "apiVersion": "authorization.k8s.io/v1",
            "kind": "SelfSubjectAccessReview",
            "spec": {
                "resourceAttributes": {
                    "namespace": namespace,
                    "verb": verb,
                    "group": group,
                    "resource": resource,
                    "name": name,
                }
            }
        }))
        .map_err(|error| PluginError::SerializationError(format!("{context}: {error}")))?;
        let request = Request::new("/apis/authorization.k8s.io/v1/selfsubjectaccessreviews");
        let review: AccessReview = self
            .exec(request.create(&PostParams::default(), body), &context)
            .await?;

        Ok(review.status.allowed)
    }

    pub async fn namespaces(&self) -> PluginResult<Vec<String>> {
        let request = Request::new("/api/v1/namespaces");
        let mut names = Vec::new();
        let mut token: Option<String> = None;

        loop {
            let page = self
                .list_page(&request, PAGE_LIMIT, token.as_deref(), "list namespaces")
                .await?;
            names.extend(page.items.iter().filter_map(|item| {
                item.pointer("/metadata/name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }));
            match page
                .metadata
                .continue_token
                .filter(|token| !token.is_empty())
            {
                Some(next) => token = Some(next),
                None => {
                    names.sort();
                    return Ok(names);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kubeconfig_yaml(user_auth: &str) -> String {
        format!(
            "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster:\n    server: https://127.0.0.1:6443\ncontexts:\n- name: ctx\n  context:\n    cluster: c\n    user: u\n- name: other\n  context:\n    cluster: c\n    user: plain\ncurrent-context: ctx\nusers:\n- name: u\n  user:\n{user_auth}\n- name: plain\n  user:\n    token: t\n"
        )
    }

    const EXEC_AUTH: &str = "    exec:\n      apiVersion: client.authentication.k8s.io/v1beta1\n      command: aws\n      args: [eks, get-token]";

    fn parse(yaml: &str) -> Kubeconfig {
        Kubeconfig::from_yaml(yaml).unwrap()
    }

    #[test]
    fn exec_auth_is_rejected_on_mobile_for_the_selected_user() {
        let kubeconfig = parse(&kubeconfig_yaml(EXEC_AUTH));

        for context in [None, Some("ctx")] {
            let error = ensure_supported_auth(&kubeconfig, context, true).unwrap_err();
            assert!(
                matches!(&error, PluginError::NotSupported(message) if message.contains("exec credential plugin") && message.contains("not supported on this platform")),
                "{error:?}"
            );
        }
    }

    #[test]
    fn exec_auth_is_allowed_off_mobile_and_for_other_users() {
        let kubeconfig = parse(&kubeconfig_yaml(EXEC_AUTH));

        assert!(ensure_supported_auth(&kubeconfig, None, false).is_ok());
        assert!(ensure_supported_auth(&kubeconfig, Some("other"), true).is_ok());
        assert!(ensure_supported_auth(&kubeconfig, Some("missing"), true).is_ok());

        let token_only = parse(&kubeconfig_yaml("    token: t"));
        assert!(ensure_supported_auth(&token_only, None, true).is_ok());
    }

    #[test]
    fn pasted_content_loads_without_touching_the_filesystem() {
        let source = KubeconfigSource::Content(kubeconfig_yaml("    token: t"));

        let kubeconfig = load_kubeconfig(&source).unwrap();

        assert_eq!(kubeconfig.contexts.len(), 2);
    }

    #[test]
    fn invalid_pasted_content_is_rejected_without_echoing_it() {
        let source = KubeconfigSource::Content("users: {token: s3cr3t-value".to_string());

        let error = load_kubeconfig(&source).unwrap_err();

        assert!(matches!(&error, PluginError::InvalidConfig(_)), "{error:?}");
        assert!(!error.to_string().contains("s3cr3t-value"), "{error}");
    }
}
