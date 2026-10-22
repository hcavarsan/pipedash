use std::{
    collections::HashMap,
    path::Path,
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
    config::Target,
    types::{
        FluxResource,
        Kind,
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

fn blocking_failed(error: tokio::task::JoinError) -> PluginError {
    PluginError::Internal(format!("Kubeconfig loader task failed: {error}"))
}

pub(crate) async fn available_contexts(paths: Vec<String>) -> PluginResult<Vec<String>> {
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

fn map_error(error: kube::Error, context: &str) -> PluginError {
    match error {
        kube::Error::Api(status) => {
            let detail = format!("{context}: {} (HTTP {})", status.message, status.code);
            match status.code {
                401 | 403 => PluginError::AuthenticationFailed(detail),
                404 => PluginError::PipelineNotFound(detail),
                _ => PluginError::ApiError(detail),
            }
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
    pub async fn connect(paths: Vec<String>, context: Option<String>) -> PluginResult<Self> {
        let runtime = tokio::runtime::Handle::current();
        let client = tokio::task::spawn_blocking(move || {
            let kubeconfig = read_kubeconfigs(&paths)?;
            let options = KubeConfigOptions {
                context,
                ..Default::default()
            };
            let config = runtime
                .block_on(kube::Config::from_custom_kubeconfig(kubeconfig, &options))
                .map_err(|error| {
                    PluginError::InvalidConfig(format!("Failed to load kubeconfig: {error}"))
                })?;

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

    pub async fn can_i(
        &self, verb: &str, kind: Kind, namespace: Option<&str>,
    ) -> PluginResult<bool> {
        let scope = namespace.map_or_else(
            || "all namespaces".to_string(),
            |namespace| format!("namespace {namespace}"),
        );
        let context = format!("check permission to {verb} {} in {scope}", kind.plural());
        let body = serde_json::to_vec(&json!({
            "apiVersion": "authorization.k8s.io/v1",
            "kind": "SelfSubjectAccessReview",
            "spec": {
                "resourceAttributes": {
                    "namespace": namespace,
                    "verb": verb,
                    "group": kind.group(),
                    "resource": kind.plural(),
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
