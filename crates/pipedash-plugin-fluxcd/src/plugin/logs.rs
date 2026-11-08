use pipedash_plugin_api::{
    LogCursor,
    LogNode,
    LogRead,
    LogSink,
    PipelineRun,
    PluginError,
    PluginResult,
    RunRef,
};

use super::FluxcdPlugin;
use crate::{
    config::Target,
    logs::{
        self,
        Node,
    },
    mapper,
    types::FluxResource,
};

impl FluxcdPlugin {
    pub(super) async fn runs(
        &self, pipeline_id: &str,
    ) -> PluginResult<(Target, FluxResource, Vec<PipelineRun>)> {
        let target = self.target(pipeline_id)?;
        let resource = self.client().await?.get(&target).await?;
        let runs = mapper::build_runs(&resource)?;

        Ok((target, resource, runs))
    }

    pub(super) async fn run(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<(FluxResource, PipelineRun)> {
        let (target, resource, runs) = self.runs(pipeline_id).await?;
        let mut matching: Vec<PipelineRun> = runs
            .into_iter()
            .filter(|run| run.run_number == run_number)
            .collect();

        match matching.len() {
            0 => Err(PluginError::PipelineNotFound(format!(
                "{target} has no record {run_number}. Flux keeps a bounded status.history, so older records are no longer available"
            ))),
            1 => Ok((resource, matching.remove(0))),
            _ => Err(PluginError::ApiError(format!(
                "{target} has {} records numbered {run_number}; refusing to pick one",
                matching.len()
            ))),
        }
    }

    pub(super) async fn fetch_log_nodes_impl(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (_, record) = self.run(run.pipeline_id, run.run_number).await?;

        Ok(logs::log_nodes(&record))
    }

    pub(super) async fn read_log_impl(
        &self, run: RunRef<'_>, node_id: &str, _from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let node = Node::parse(node_id)?;
        let target = self.target(run.pipeline_id)?;
        let mut out = Vec::new();

        match node {
            Node::Reconciliation => {
                let (resource, record) = self.run(run.pipeline_id, run.run_number).await?;
                logs::write_reconciliation(&resource, &record, &mut out);
            }
            Node::Events => {
                logs::write_events(self.client().await?.events(&target).await?, &mut out);
            }
        }

        sink.write(out.into())
            .await
            .map(|()| LogRead::replace(None))
    }
}
