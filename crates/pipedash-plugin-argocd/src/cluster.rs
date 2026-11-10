use std::collections::BTreeSet;

use kube::core::{
    params::{
        GetParams,
        ListParams,
        Patch,
        PatchParams,
    },
    subresource::LogParams,
    Request,
};
use pipedash_plugin_api::kubernetes;
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use serde::de::{
    DeserializeOwned,
    IgnoredAny,
};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::OnceCell;

use crate::client::PodLogRequest;
use crate::config::ClusterSettings;
use crate::logs::POD_LOG_TAIL_LINES;
use crate::types::logs::{
    Event,
    EventList,
    Pod,
};
use crate::types::{
    Application,
    ApplicationList,
    SyncRequest,
};

mod operations;
pub(crate) mod pods;

const ACTOR: &str = "pipedash";
const APPLICATIONS_API: &str = "/apis/argoproj.io/v1alpha1";

#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct Items<T> {
    #[serde(default)]
    items: Vec<T>,
}

fn applications_path(namespace: Option<&str>) -> String {
    match namespace {
        Some(namespace) => format!("{APPLICATIONS_API}/namespaces/{namespace}/applications"),
        None => format!("{APPLICATIONS_API}/applications"),
    }
}

pub(crate) struct ClusterClient {
    settings: ClusterSettings,
    client: OnceCell<kube::Client>,
}

impl ClusterClient {
    pub fn new(settings: ClusterSettings) -> Self {
        Self {
            settings,
            client: OnceCell::new(),
        }
    }

    async fn client(&self) -> PluginResult<&kube::Client> {
        self.client
            .get_or_try_init(|| {
                let source = self.settings.source.clone();
                kubernetes::connect(move || source.load(), self.settings.context.clone(), true)
            })
            .await
    }

    fn guard(&self, namespace: &str, name: &str) -> PluginResult<()> {
        if !kubernetes::is_namespace(namespace) || !kubernetes::is_resource_name(name) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid application '{namespace}/{name}': expected Kubernetes namespace and resource names"
            )));
        }
        let namespaces = &self.settings.namespaces;
        if !namespaces.is_empty() && !namespaces.iter().any(|allowed| allowed == namespace) {
            return Err(PluginError::InvalidConfig(format!(
                "{namespace}/{name} is outside the configured namespaces ({})",
                namespaces.join(", ")
            )));
        }

        Ok(())
    }

    async fn list<T: DeserializeOwned>(
        &self, path: String, params: &ListParams, context: &str,
    ) -> PluginResult<T> {
        let request = Request::new(path);
        kubernetes::exec(self.client().await?, request.list(params), context).await
    }

    async fn fetch<T: DeserializeOwned>(&self, namespace: &str, name: &str) -> PluginResult<T> {
        self.guard(namespace, name)?;
        let request = Request::new(applications_path(Some(namespace)));
        let context = format!("get application {namespace}/{name}");

        kubernetes::exec(
            self.client().await?,
            request.get(name, &GetParams::default()),
            &context,
        )
        .await
    }

    async fn patch(
        &self, namespace: &str, name: &str, body: Value, context: &str,
    ) -> PluginResult<()> {
        let request = Request::new(applications_path(Some(namespace)));
        let params = PatchParams {
            field_manager: Some(ACTOR.to_string()),
            ..Default::default()
        };

        kubernetes::exec::<IgnoredAny>(
            self.client().await?,
            request.patch(name, &params, &Patch::Merge(body)),
            context,
        )
        .await?;

        Ok(())
    }

    pub async fn list_applications(&self) -> PluginResult<Vec<Application>> {
        let scopes: Vec<Option<&str>> = if self.settings.namespaces.is_empty() {
            vec![None]
        } else {
            self.settings
                .namespaces
                .iter()
                .map(|namespace| Some(namespace.as_str()))
                .collect()
        };
        let mut apps = Vec::new();
        for scope in scopes {
            let context = scope.map_or_else(
                || "list applications in all namespaces".to_string(),
                |namespace| format!("list applications in namespace {namespace}"),
            );
            let list: ApplicationList = self
                .list(applications_path(scope), &ListParams::default(), &context)
                .await
                .map_err(|error| match error {
                    PluginError::PipelineNotFound(_) => PluginError::InvalidConfig(
                        "ArgoCD is not installed in this cluster: applications.argoproj.io was not found"
                            .to_string(),
                    ),
                    other => other,
                })?;
            apps.extend(list.items);
        }

        Ok(apps)
    }

    pub async fn get_application(&self, namespace: &str, name: &str) -> PluginResult<Application> {
        self.fetch(namespace, name).await
    }

    pub async fn sync_application(
        &self, namespace: &str, name: &str, request: &SyncRequest,
    ) -> PluginResult<()> {
        let app: Value = self.fetch(namespace, name).await?;
        let body = operations::sync_patch(&app, name, request)?;

        self.patch(
            namespace,
            name,
            body,
            &format!("sync application {namespace}/{name}"),
        )
        .await
    }

    pub async fn terminate_operation(&self, namespace: &str, name: &str) -> PluginResult<()> {
        let app: Value = self.fetch(namespace, name).await?;
        let body = operations::terminate_patch(&app, name)?;

        self.patch(
            namespace,
            name,
            body,
            &format!("terminate operation of application {namespace}/{name}"),
        )
        .await
    }

    async fn events(
        &self, namespace: &str, selector: &str, context: &str,
    ) -> PluginResult<Vec<Event>> {
        let path = format!("/api/v1/namespaces/{namespace}/events");
        let list: EventList = self
            .list(path, &ListParams::default().fields(selector), context)
            .await?;

        Ok(list.items)
    }

    pub async fn application_events(
        &self, namespace: &str, name: &str,
    ) -> PluginResult<Vec<Event>> {
        self.guard(namespace, name)?;
        let selector = format!(
            "involvedObject.kind=Application,involvedObject.name={name},involvedObject.namespace={namespace}"
        );

        self.events(
            namespace,
            &selector,
            &format!("list events of application {namespace}/{name}"),
        )
        .await
    }

    pub async fn pod_events(
        &self, namespace: &str, pod: &str, uid: &str,
    ) -> PluginResult<Vec<Event>> {
        let selector = format!(
            "involvedObject.kind=Pod,involvedObject.name={pod},involvedObject.namespace={namespace},involvedObject.uid={uid}"
        );

        self.events(
            namespace,
            &selector,
            &format!("list events of pod {namespace}/{pod}"),
        )
        .await
    }

    async fn in_namespace<T: DeserializeOwned>(
        &self, api: &str, kind: &str, namespace: &str,
    ) -> PluginResult<Vec<T>> {
        let path = format!("{api}/namespaces/{namespace}/{kind}");
        let context = format!("list {kind} in namespace {namespace}");
        let list: Items<T> = self.list(path, &ListParams::default(), &context).await?;

        Ok(list.items)
    }

    pub async fn pods(&self, app: &Application) -> PluginResult<Vec<Pod>> {
        if !pods::in_cluster(&app.spec.destination) {
            return Ok(Vec::new());
        }

        let namespaces: BTreeSet<&str> = app
            .status
            .resources
            .iter()
            .filter_map(|resource| resource.namespace.as_deref())
            .filter(|namespace| kubernetes::is_namespace(namespace))
            .collect();
        let mut found = Vec::new();
        for namespace in namespaces {
            let (pods, replica_sets, jobs) = (
                self.in_namespace::<Pod>("/api/v1", "pods", namespace)
                    .await?,
                self.in_namespace::<pods::Owned>("/apis/apps/v1", "replicasets", namespace)
                    .await?,
                self.in_namespace::<pods::Owned>("/apis/batch/v1", "jobs", namespace)
                    .await?,
            );
            found.extend(pods::owned_pods(
                &app.status.resources,
                pods,
                &replica_sets,
                &jobs,
            ));
        }

        Ok(found)
    }

    pub async fn pod_log(&self, request: &PodLogRequest<'_>) -> PluginResult<String> {
        let params = LogParams {
            container: Some(request.container.to_string()),
            previous: request.previous,
            timestamps: true,
            tail_lines: POD_LOG_TAIL_LINES.parse().ok(),
            ..Default::default()
        };
        let context = format!("read logs of pod {}/{}", request.namespace, request.pod);
        let http = Request::new(format!("/api/v1/namespaces/{}/pods", request.namespace))
            .logs(request.pod, &params)
            .map_err(|error| PluginError::InvalidConfig(format!("{context}: {error}")))?;

        self.client()
            .await?
            .request_text(http)
            .await
            .map_err(|error| match error {
                kube::Error::Api(status) if status.code == 404 => PluginError::PipelineNotFound(
                    format!("Logs of pod {} are no longer available", request.pod),
                ),
                other => kubernetes::map_error(other, &context),
            })
    }
}
