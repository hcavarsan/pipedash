use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use tracing::warn;

use crate::backend::Backend;
use crate::cluster::pods::in_cluster;
use crate::types::logs::Pod;
use crate::types::Application;

pub(super) async fn cluster_uid<'a>(
    backend: &'a Backend, apps: &[Application],
) -> PluginResult<Option<&'a str>> {
    if !apps.iter().any(|app| in_cluster(&app.spec.destination)) {
        return Ok(None);
    }
    match backend.cluster_uid().await {
        Err(error @ PluginError::RateLimited { .. }) => Err(error),
        Err(error) => {
            warn!(%error, "Cluster identity unavailable, in-cluster targets are not reported");
            Ok(None)
        }
        uid => uid,
    }
}

pub(super) async fn destination_pods(
    backend: &Backend, app: &Application,
) -> PluginResult<Option<Vec<Pod>>> {
    match backend.destination_pods(app).await {
        Err(error @ PluginError::RateLimited { .. }) => Err(error),
        Err(error) => {
            warn!(
                app = %app.metadata.name,
                %error,
                "Pods unavailable, using the images ArgoCD reports"
            );
            Ok(None)
        }
        pods => pods,
    }
}
