use bytes::Bytes;
use pipedash_plugin_api::logs::rendered::RenderedCursor;
use pipedash_plugin_api::support::run_metadata_str;
use pipedash_plugin_api::{
    LogCursor,
    LogNode,
    LogRead,
    LogSink,
    PluginError,
    PluginResult,
    RunRef,
};

use super::CoolifyPlugin;
use crate::{
    config,
    log_dialect,
    mapper,
    types,
};

impl CoolifyPlugin {
    async fn deployment_uuid(&self, run: &RunRef<'_>, app_uuid: &str) -> PluginResult<String> {
        match run_metadata_str(run, "deployment_uuid") {
            Some(uuid) => Ok(uuid.to_string()),
            None => Ok(self
                .client()?
                .find_deployment(app_uuid, run.run_number)
                .await?
                .0
                .deployment_uuid),
        }
    }

    async fn run_detail(&self, run: &RunRef<'_>) -> PluginResult<types::DeploymentDetail> {
        let app_uuid = config::parse_pipeline_id(run.pipeline_id)?;
        let uuid = self.deployment_uuid(run, app_uuid).await?;
        self.client()?.deployment(&uuid).await
    }

    fn logs_text(detail: &types::DeploymentDetail) -> PluginResult<&str> {
        match &detail.logs {
            None => Err(PluginError::PermissionDenied(
                "Coolify hides deployment logs from this API token. Give the token the \
                 read:sensitive permission to see them"
                    .to_string(),
            )),
            Some(None) => Ok(""),
            Some(Some(text)) => Ok(text),
        }
    }

    pub(super) async fn fetch_log_nodes_impl(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let detail = self.run_detail(&run).await?;
        Self::logs_text(&detail)?;

        Ok(vec![mapper::map_log_node(
            &detail,
            &mapper::web_url(&self.server_url, detail.deployment_url.as_deref()),
        )])
    }

    pub(super) async fn read_log_impl(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        if node_id != mapper::LOG_NODE_ID {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid log node ID: {node_id}"
            )));
        }
        let detail = self.run_detail(&run).await?;
        let entries = log_dialect::parse_entries(Self::logs_text(&detail)?)?;
        let rendered = Bytes::from(log_dialect::render(&entries));
        RenderedCursor::read(rendered, from, sink).await
    }
}
