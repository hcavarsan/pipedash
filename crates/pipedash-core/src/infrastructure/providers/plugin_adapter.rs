use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    ArtifactRepository,
    ChangedFile,
    CommitComparison,
    CommitInfo,
    CommitSha,
    Digest,
    Plugin as PluginTrait,
    Provenance,
    RegistryAuth,
    RepoId,
    RunRef,
    TriggerResult,
};

use super::status::plugin_status;
use crate::domain::pipeline_id;
use crate::domain::{
    DomainResult,
    LogCursor,
    LogDialect,
    LogNode,
    LogRead,
    LogSink,
    Pipeline,
    PipelineRun,
    Provider,
    TriggerParams,
};

pub struct PluginAdapter {
    plugin: Arc<dyn PluginTrait>,
    provider_type: String,
    provider_id: i64,
    renew_at: Option<DateTime<Utc>>,
}

impl PluginAdapter {
    pub fn new(
        plugin: Box<dyn PluginTrait>, provider_type: String, provider_id: i64,
        renew_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            plugin: Arc::from(plugin),
            provider_type,
            provider_id,
            renew_at,
        }
    }

    pub fn global_id(&self, local: &str) -> String {
        pipeline_id::compose(&self.provider_type, self.provider_id, local)
    }

    pub fn local_id<'a>(&self, id: &'a str) -> DomainResult<&'a str> {
        pipeline_id::strip(&self.provider_type, self.provider_id, id)
    }

    fn local_run_id<'a>(&self, id: &'a str) -> &'a str {
        self.local_id(id).unwrap_or(id)
    }

    fn convert_pipeline(&self, plugin_pipeline: pipedash_plugin_api::Pipeline) -> Pipeline {
        Pipeline {
            id: self.global_id(&plugin_pipeline.id),
            provider_id: self.provider_id,
            provider_type: self.provider_type.clone(),
            name: plugin_pipeline.name,
            status: Self::convert_status(plugin_pipeline.status),
            last_run: plugin_pipeline.last_run,
            last_updated: plugin_pipeline.last_updated,
            repository: plugin_pipeline.repository,
            branch: plugin_pipeline.branch,
            workflow_file: plugin_pipeline.workflow_file,
            resource_state: plugin_pipeline.resource_state,
            metadata: plugin_pipeline.metadata,
            overdue_run: None,
            typical_duration_seconds: None,
            actions: Default::default(),
            correlation: plugin_pipeline.correlation,
        }
    }

    fn convert_run(&self, plugin_run: pipedash_plugin_api::PipelineRun) -> PipelineRun {
        PipelineRun {
            id: self.global_id(&plugin_run.id),
            pipeline_id: self.global_id(&plugin_run.pipeline_id),
            run_number: plugin_run.run_number,
            status: Self::convert_status(plugin_run.status),
            started_at: plugin_run.started_at,
            concluded_at: plugin_run.concluded_at,
            duration_seconds: plugin_run.duration_seconds,
            logs_url: plugin_run.logs_url,
            commit_sha: plugin_run.commit_sha,
            commit_message: plugin_run.commit_message,
            branch: plugin_run.branch,
            actor: plugin_run.actor,
            inputs: plugin_run.inputs,
            metadata: plugin_run.metadata,
            actions: Default::default(),
            display: Default::default(),
            pace: Default::default(),
            correlation: plugin_run.correlation,
            evidence: plugin_run.evidence,
        }
    }

    fn run_ref<'a>(&self, run: &'a PipelineRun) -> DomainResult<RunRef<'a>> {
        Ok(RunRef {
            pipeline_id: self.local_id(&run.pipeline_id)?,
            run_number: run.run_number,
            run_id: self.local_run_id(&run.id),
            metadata: &run.metadata,
        })
    }
}

#[async_trait]
impl Provider for PluginAdapter {
    async fn fetch_pipelines(&self) -> DomainResult<Vec<Pipeline>> {
        let pipelines = self
            .plugin
            .fetch_pipelines()
            .await
            .map_err(Self::map_error)?;

        Ok(pipelines
            .into_iter()
            .map(|p| self.convert_pipeline(p))
            .collect())
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        let runs = self
            .plugin
            .fetch_run_history(self.local_id(pipeline_id)?, limit)
            .await
            .map_err(Self::map_error)?;

        Ok(runs.into_iter().map(|r| self.convert_run(r)).collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<PipelineRun> {
        let run = self
            .plugin
            .fetch_run_details(self.local_id(pipeline_id)?, run_number)
            .await
            .map_err(Self::map_error)?;

        Ok(self.convert_run(run))
    }

    async fn fetch_recent_runs(
        &self, pipeline_ids: &[String], limit: usize,
    ) -> DomainResult<HashMap<String, Vec<PipelineRun>>> {
        let local_ids = pipeline_ids
            .iter()
            .map(|id| self.local_id(id).map(str::to_string))
            .collect::<DomainResult<Vec<_>>>()?;
        let runs = self
            .plugin
            .fetch_recent_runs(&local_ids, limit)
            .await
            .map_err(Self::map_error)?;

        Ok(runs
            .into_iter()
            .map(|(id, runs)| {
                let runs = runs.into_iter().map(|r| self.convert_run(r)).collect();
                (self.global_id(&id), runs)
            })
            .collect())
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> DomainResult<TriggerResult> {
        let trigger_params = pipedash_plugin_api::TriggerParams {
            workflow_id: self.local_id(&params.workflow_id)?.to_string(),
            inputs: params.inputs,
        };

        let result = self
            .plugin
            .trigger_pipeline(trigger_params)
            .await
            .map_err(Self::map_error)?;

        Ok(TriggerResult {
            run_id: result.run_id.map(|id| self.global_id(&id)),
            run_number: result.run_number,
        })
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        self.plugin
            .cancel_run(self.local_id(pipeline_id)?, run_number)
            .await
            .map_err(Self::map_error)
    }

    async fn force_cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        self.plugin
            .force_cancel_run(self.local_id(pipeline_id)?, run_number)
            .await
            .map_err(Self::map_error)
    }

    async fn review_run(
        &self, pipeline_id: &str, run_number: i64, decision: pipedash_plugin_api::ReviewDecision,
    ) -> DomainResult<()> {
        self.plugin
            .review_run(self.local_id(pipeline_id)?, run_number, decision)
            .await
            .map_err(Self::map_error)
    }

    async fn rerun_failed_jobs(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        self.plugin
            .rerun_failed_jobs(self.local_id(pipeline_id)?, run_number)
            .await
            .map_err(Self::map_error)
    }

    async fn rerun_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        self.plugin
            .rerun_run(self.local_id(pipeline_id)?, run_number)
            .await
            .map_err(Self::map_error)
    }

    async fn enable_pipeline(&self, pipeline_id: &str) -> DomainResult<()> {
        self.plugin
            .enable_pipeline(self.local_id(pipeline_id)?)
            .await
            .map_err(Self::map_error)
    }

    async fn get_workflow_parameters(
        &self, workflow_id: &str,
    ) -> DomainResult<Vec<pipedash_plugin_api::WorkflowParameter>> {
        self.plugin
            .fetch_workflow_parameters(self.local_id(workflow_id)?)
            .await
            .map_err(Self::map_error)
    }

    async fn fetch_rate_limit(&self) -> DomainResult<Option<pipedash_plugin_api::RateLimit>> {
        self.plugin
            .fetch_rate_limit()
            .await
            .map_err(Self::map_error)
    }

    fn source_hosts(&self, config: &HashMap<String, String>) -> Vec<String> {
        self.plugin.source_hosts(config)
    }

    async fn resolve_commit(&self, repo: &RepoId, rev: &str) -> DomainResult<CommitInfo> {
        self.plugin
            .resolve_commit(repo, rev)
            .await
            .map_err(Self::map_error)
    }

    async fn compare_commits(
        &self, repo: &RepoId, base: &CommitSha, head: &CommitSha,
    ) -> DomainResult<CommitComparison> {
        self.plugin
            .compare_commits(repo, base, head)
            .await
            .map_err(Self::map_error)
    }

    async fn commit_changes(
        &self, repo: &RepoId, sha: &CommitSha,
    ) -> DomainResult<Vec<ChangedFile>> {
        self.plugin
            .commit_changes(repo, sha)
            .await
            .map_err(Self::map_error)
    }

    async fn artifact_provenance(
        &self, repository: &ArtifactRepository, digest: &Digest,
    ) -> DomainResult<Option<Provenance>> {
        let provenance = self
            .plugin
            .artifact_provenance(repository, digest)
            .await
            .map_err(Self::map_error)?;
        Ok(provenance.map(|mut provenance| {
            provenance.pipeline_id = provenance.pipeline_id.map(|id| self.global_id(&id));
            provenance
        }))
    }

    async fn registry_auth(&self, host: &str) -> DomainResult<Option<RegistryAuth>> {
        self.plugin
            .registry_auth(host)
            .await
            .map_err(Self::map_error)
    }

    fn capabilities(&self) -> &pipedash_plugin_api::PluginCapabilities {
        &self.plugin.metadata().capabilities
    }

    fn run_history_policy(&self) -> pipedash_plugin_api::RunHistoryPolicy {
        self.plugin.metadata().run_history
    }

    fn can_archive_run(&self, pipeline: &Pipeline, run: &PipelineRun) -> bool {
        self.plugin.can_archive_run(
            &pipeline.metadata,
            &run.metadata,
            plugin_status(&run.status),
        )
    }

    fn renew_at(&self) -> Option<DateTime<Utc>> {
        self.renew_at
    }

    fn status_source(&self) -> Option<&pipedash_plugin_api::StatusSource> {
        self.plugin.status_source()
    }

    async fn check_health(&self) -> DomainResult<Vec<pipedash_plugin_api::ComponentHealth>> {
        self.plugin.check_health().await.map_err(Self::map_error)
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        self.plugin.log_dialect()
    }

    fn log_tree_version(&self) -> u32 {
        self.plugin.log_tree_version()
    }

    async fn fetch_log_nodes(&self, run: &PipelineRun) -> DomainResult<Vec<LogNode>> {
        let nodes = self
            .plugin
            .fetch_log_nodes(self.run_ref(run)?)
            .await
            .map_err(Self::map_error)?;
        Ok(nodes
            .into_iter()
            .map(|n| LogNode {
                id: n.id,
                parent_id: n.parent_id,
                kind: n.kind,
                name: n.name,
                status: Self::convert_status(n.status),
                started_at: n.started_at,
                finished_at: n.finished_at,
                log: n.log,
                web_url: n.web_url,
            })
            .collect())
    }

    async fn read_log(
        &self, run: &PipelineRun, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> DomainResult<LogRead> {
        self.plugin
            .read_log(self.run_ref(run)?, node_id, from, sink)
            .await
            .map_err(Self::map_error)
    }
}

mod convert;
#[cfg(test)]
mod tests;
