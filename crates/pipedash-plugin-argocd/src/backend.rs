use pipedash_plugin_api::logs::tagged::write_line;
use pipedash_plugin_api::{
    LineMarker,
    LogSink,
    PluginError,
    PluginResult,
};
use tracing::warn;

use crate::client::{
    ArgocdClient,
    PodLogRequest,
};
use crate::cluster::{
    pods,
    ClusterClient,
};
use crate::types::logs::{
    ApplicationTree,
    Event,
    Pod,
};
use crate::types::{
    Application,
    SyncRequest,
};

pub(crate) enum Backend {
    Api(ArgocdClient),
    Cluster(ClusterClient),
}

impl Backend {
    pub async fn list_applications(&self) -> PluginResult<Vec<Application>> {
        match self {
            Self::Api(client) => client.list_applications(None).await,
            Self::Cluster(cluster) => cluster.list_applications().await,
        }
    }

    pub async fn get_application(&self, namespace: &str, name: &str) -> PluginResult<Application> {
        match self {
            Self::Api(client) => client.get_application(name).await,
            Self::Cluster(cluster) => cluster.get_application(namespace, name).await,
        }
    }

    pub async fn health_tree(&self, app: &Application) -> PluginResult<ApplicationTree> {
        match self {
            Self::Api(client) => client.resource_tree(&app.metadata.name).await,
            Self::Cluster(_) => Ok(pods::tree(app, &[])),
        }
    }

    pub async fn resource_tree(&self, app: &Application) -> PluginResult<ApplicationTree> {
        match self {
            Self::Api(client) => client.resource_tree(&app.metadata.name).await,
            Self::Cluster(cluster) => {
                let pods = cluster.pods(app).await.unwrap_or_else(|error| {
                    warn!(app = %app.metadata.name, %error, "Pods unavailable, listing managed resources only");
                    Vec::new()
                });
                Ok(pods::tree(app, &pods))
            }
        }
    }

    pub async fn application_events(
        &self, namespace: &str, name: &str,
    ) -> PluginResult<Vec<Event>> {
        match self {
            Self::Api(client) => client.application_events(name).await,
            Self::Cluster(cluster) => cluster.application_events(namespace, name).await,
        }
    }

    pub async fn pod(
        &self, app_namespace: &str, app_name: &str, namespace: &str, pod: &str,
    ) -> PluginResult<Pod> {
        match self {
            Self::Api(client) => client.pod(app_name, namespace, pod).await,
            Self::Cluster(cluster) => {
                let app = cluster.get_application(app_namespace, app_name).await?;
                cluster
                    .pods(&app)
                    .await?
                    .into_iter()
                    .find(|candidate| {
                        candidate.metadata.namespace == namespace && candidate.metadata.name == pod
                    })
                    .ok_or_else(|| {
                        PluginError::PipelineNotFound(format!(
                            "Pod {namespace}/{pod} is not part of application {app_name}"
                        ))
                    })
            }
        }
    }

    pub async fn pod_events(
        &self, app_name: &str, namespace: &str, pod: &str, uid: &str,
    ) -> PluginResult<Vec<Event>> {
        match self {
            Self::Api(client) => client.pod_events(app_name, namespace, pod, uid).await,
            Self::Cluster(cluster) => cluster.pod_events(namespace, pod, uid).await,
        }
    }

    pub async fn stream_pod_log(
        &self, app_name: &str, request: PodLogRequest<'_>, sink: &mut LogSink,
    ) -> PluginResult<usize> {
        match self {
            Self::Api(client) => client.stream_pod_log(app_name, request, sink).await,
            Self::Cluster(cluster) => {
                let text = cluster.pod_log(&request).await?;
                let mut out = Vec::new();
                let mut lines = 0;
                for line in text.lines() {
                    let (stamp, body) = line.split_once(' ').unwrap_or((line, ""));
                    write_line(&mut out, LineMarker::Plain, Some(stamp), body);
                    lines += 1;
                }
                sink.write(out.into()).await?;
                Ok(lines)
            }
        }
    }

    pub async fn sync_application(
        &self, namespace: &str, name: &str, request: &SyncRequest,
    ) -> PluginResult<()> {
        match self {
            Self::Api(client) => client.sync_application(name, request).await,
            Self::Cluster(cluster) => cluster.sync_application(namespace, name, request).await,
        }
    }

    pub async fn terminate_operation(&self, namespace: &str, name: &str) -> PluginResult<()> {
        match self {
            Self::Api(client) => client.terminate_operation(name).await,
            Self::Cluster(cluster) => cluster.terminate_operation(namespace, name).await,
        }
    }
}
