use kube::core::{
    params::{
        GetParams,
        ListParams,
    },
    Request,
};
use pipedash_plugin_api::kubernetes;
use pipedash_plugin_api::{
    ClusterId,
    PluginError,
    PluginResult,
};
use serde::Deserialize;

use super::{
    FluxClient,
    PAGE_LIMIT,
};
use crate::pods::{
    Live,
    LiveObject,
};
use crate::types::{
    Reference,
    GIT_REPOSITORY,
};

const SOURCE_API: &str = "/apis/source.toolkit.fluxcd.io/v1";
const IDENTITY_NAMESPACE: &str = "kube-system";

#[derive(Default, Deserialize)]
struct GitRepository {
    #[serde(default)]
    spec: GitRepositorySpec,
}

#[derive(Default, Deserialize)]
struct GitRepositorySpec {
    url: Option<String>,
}

#[derive(Default, Deserialize)]
struct Namespace {
    #[serde(default)]
    metadata: NamespaceMeta,
}

#[derive(Default, Deserialize)]
struct NamespaceMeta {
    uid: Option<String>,
}

impl FluxClient {
    async fn fetch_source_url(&self, namespace: &str, name: &str) -> PluginResult<String> {
        let request = Request::new(format!(
            "{SOURCE_API}/namespaces/{namespace}/gitrepositories"
        ));
        let context = format!("get {GIT_REPOSITORY} {namespace}/{name}");
        let repository: GitRepository = kubernetes::exec(
            &self.client,
            request.get(name, &GetParams::default()),
            &context,
        )
        .await?;

        repository
            .spec
            .url
            .filter(|url| !url.is_empty())
            .ok_or_else(|| PluginError::InvalidConfig(format!("{context}: spec.url is empty")))
    }

    pub async fn source_url(
        &self, reference: &Reference, default_namespace: &str,
    ) -> PluginResult<Option<String>> {
        let namespace = reference
            .namespace
            .as_deref()
            .filter(|namespace| !namespace.is_empty())
            .unwrap_or(default_namespace);
        if reference.kind != GIT_REPOSITORY
            || !kubernetes::is_namespace(namespace)
            || !kubernetes::is_resource_name(&reference.name)
        {
            return Ok(None);
        }

        let cell = self
            .sources
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry((namespace.to_string(), reference.name.clone()))
            .or_default()
            .clone();

        match cell
            .get_or_try_init(|| self.fetch_source_url(namespace, &reference.name))
            .await
        {
            Ok(url) => Ok(Some(url.clone())),
            Err(PluginError::PipelineNotFound(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub async fn cluster_id(&self) -> PluginResult<Option<ClusterId>> {
        self.cluster
            .get_or_try_init(|| async {
                let request = Request::new("/api/v1/namespaces");
                let namespace: Namespace = kubernetes::exec(
                    &self.client,
                    request.get(IDENTITY_NAMESPACE, &GetParams::default()),
                    &format!("get namespace {IDENTITY_NAMESPACE}"),
                )
                .await?;

                Ok(namespace
                    .metadata
                    .uid
                    .as_deref()
                    .and_then(ClusterId::from_uid))
            })
            .await
            .cloned()
    }

    async fn live_objects(
        &self, api: &str, resource: &str, namespace: &str, params: ListParams,
    ) -> PluginResult<Vec<LiveObject>> {
        if !kubernetes::is_namespace(namespace) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid namespace '{namespace}': expected a Kubernetes namespace name"
            )));
        }
        let request = Request::new(format!("{api}/namespaces/{namespace}/{resource}"));
        let context = format!("list {resource} in namespace {namespace}");

        self.list_all(&request, params, &context)
            .await?
            .into_iter()
            .map(|item| {
                serde_json::from_value(item)
                    .map_err(|error| PluginError::SerializationError(format!("{context}: {error}")))
            })
            .collect()
    }

    pub async fn pods(
        &self, namespace: &str, selector: Option<&str>,
    ) -> PluginResult<Vec<LiveObject>> {
        let mut params = ListParams::default().limit(PAGE_LIMIT);
        if let Some(selector) = selector {
            params = params.labels(selector);
        }

        self.live_objects("/api/v1", "pods", namespace, params)
            .await
    }

    pub async fn live(&self, namespace: &str) -> PluginResult<Live> {
        let (pods, replica_sets) = futures::try_join!(
            self.pods(namespace, None),
            self.live_objects(
                "/apis/apps/v1",
                "replicasets",
                namespace,
                ListParams::default().limit(PAGE_LIMIT)
            )
        )?;

        Ok(Live { pods, replica_sets })
    }
}
