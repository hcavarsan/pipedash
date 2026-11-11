use kube::core::{
    params::{
        GetParams,
        ListParams,
        PostParams,
    },
    Request,
};
use pipedash_plugin_api::kubernetes;
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use serde::Deserialize;
use serde_json::{
    json,
    Value,
};

use super::{
    FluxClient,
    ObjectList,
    PAGE_LIMIT,
};
use crate::{
    config::Target,
    health::Deployment,
    types::{
        Event,
        Kind,
    },
    workloads::{
        self,
        WorkloadKind,
    },
};

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

impl FluxClient {
    pub(super) async fn list_all(
        &self, request: &Request, mut params: ListParams, context: &str,
    ) -> PluginResult<Vec<Value>> {
        let mut items = Vec::new();

        loop {
            let page: ObjectList =
                kubernetes::exec(&self.client, request.list(&params), context).await?;
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
    ) -> PluginResult<Vec<workloads::Finding>> {
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

        kubernetes::exec(
            &self.client,
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
        let review: AccessReview = kubernetes::exec(
            &self.client,
            request.create(&PostParams::default(), body),
            &context,
        )
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
