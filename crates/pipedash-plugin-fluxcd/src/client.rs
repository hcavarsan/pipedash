use std::collections::HashMap;

use kube::core::{
    params::{
        GetParams,
        ListParams,
        Patch,
        PatchParams,
    },
    Request,
};
use pipedash_plugin_api::kubernetes::{
    self,
    KubeconfigSource,
};
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use serde::de::IgnoredAny;
use serde::Deserialize;
use serde_json::{
    json,
    Value,
};

use self::config::load_kubeconfig;
use crate::{
    config::Target,
    types::{
        FluxResource,
        Kind,
    },
};

mod cluster;
pub(crate) mod config;

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

pub(crate) struct FluxClient {
    client: kube::Client,
}

fn list_context(kind: Kind, namespace: Option<&str>) -> String {
    match namespace {
        Some(namespace) => format!("list {kind}s in namespace {namespace}"),
        None => format!("list {kind}s in all namespaces"),
    }
}

impl FluxClient {
    pub async fn connect(
        source: KubeconfigSource<Vec<String>>, context: Option<String>,
    ) -> PluginResult<Self> {
        let client = kubernetes::connect(move || load_kubeconfig(&source), context, true).await?;

        Ok(Self { client })
    }

    async fn list_page(
        &self, request: &Request, limit: u32, token: Option<&str>, context: &str,
    ) -> PluginResult<ObjectList> {
        let mut params = ListParams::default().limit(limit);
        if let Some(token) = token {
            params = params.continue_token(token);
        }

        kubernetes::exec(&self.client, request.list(&params), context).await
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
        let value: Value = kubernetes::exec(
            &self.client,
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

        kubernetes::exec::<IgnoredAny>(
            &self.client,
            request.patch(&target.name, &params, &patch),
            &format!("request reconcile of {target}"),
        )
        .await?;

        Ok(())
    }
}
