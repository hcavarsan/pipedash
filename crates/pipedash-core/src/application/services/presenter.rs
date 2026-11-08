use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use pipedash_plugin_api::{
    is_no_auth,
    ActionAvailability,
    PipelineActionState,
    PipelineFacts,
    PluginRegistry,
    RunActionState,
    RunFacts,
};

use super::pipeline_service::overdue::typical_duration;
use super::pipeline_service::timing::run_pace;
use super::provider_service::ProviderService;
use crate::domain::pipeline_id;
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
};
use crate::infrastructure::providers::status::plugin_status;

pub(crate) const READ_ONLY_PROVIDER: &str = concat!(
    "This provider connects without credentials, so it is read-only. ",
    "Edit it and choose an access token or single sign-on to start or stop runs."
);

pub(crate) struct Presenter {
    registry: Arc<PluginRegistry>,
    provider_service: Arc<ProviderService>,
}

impl Presenter {
    pub(crate) fn new(
        registry: Arc<PluginRegistry>, provider_service: Arc<ProviderService>,
    ) -> Self {
        Self {
            registry,
            provider_service,
        }
    }

    pub(crate) async fn pipelines(&self, pipelines: &mut [Pipeline]) -> DomainResult<()> {
        let mut read_only = HashMap::new();
        for pipeline in pipelines.iter() {
            self.note_read_only(&mut read_only, pipeline.provider_id)
                .await?;
        }

        for pipeline in pipelines.iter_mut() {
            pipeline.actions = self.pipeline_state(pipeline, read_only[&pipeline.provider_id]);
        }
        Ok(())
    }

    pub(crate) async fn runs(
        &self, pipeline: &Pipeline, runs: &mut [PipelineRun],
    ) -> DomainResult<()> {
        let mut read_only = HashMap::new();
        self.note_read_only(&mut read_only, pipeline.provider_id)
            .await?;
        let read_only = read_only[&pipeline.provider_id];

        let typical = typical_duration(runs).or(pipeline.typical_duration_seconds);
        let now = Utc::now();
        let plugin = self.registry.get(&pipeline.provider_type);
        let facts = pipeline_facts(pipeline);

        for run in runs.iter_mut() {
            run.pace = run_pace(run, typical, now);
            let Some(plugin) = plugin else {
                continue;
            };
            let run_facts = run_facts(run);
            let mut actions = plugin.run_actions(&facts, &run_facts);
            let display = plugin.run_display(&facts, &run_facts);
            if read_only {
                block_run_actions(&mut actions);
            }
            run.actions = actions;
            run.display = display;
        }
        Ok(())
    }

    async fn note_read_only(
        &self, read_only: &mut HashMap<i64, bool>, provider_id: i64,
    ) -> DomainResult<()> {
        if read_only.contains_key(&provider_id) {
            return Ok(());
        }
        let blocked = match self.provider_service.get_provider_config(provider_id).await {
            Ok(config) => is_no_auth(&config.config),
            Err(DomainError::ProviderNotFound(_)) => true,
            Err(error) => return Err(error),
        };
        read_only.insert(provider_id, blocked);
        Ok(())
    }

    fn pipeline_state(&self, pipeline: &Pipeline, read_only: bool) -> PipelineActionState {
        let Some(plugin) = self.registry.get(&pipeline.provider_type) else {
            return PipelineActionState::default();
        };
        let mut state = plugin.pipeline_actions(&pipeline_facts(pipeline));
        if read_only {
            for action in [
                &mut state.trigger,
                &mut state.cancel,
                &mut state.force_cancel,
                &mut state.enable,
            ] {
                *action = read_only_block();
            }
        }
        state
    }
}

fn read_only_block() -> ActionAvailability {
    ActionAvailability::blocked(READ_ONLY_PROVIDER)
}

fn block_run_actions(actions: &mut RunActionState) {
    for action in [
        &mut actions.cancel,
        &mut actions.force_cancel,
        &mut actions.rerun,
        &mut actions.rerun_failed,
        &mut actions.review,
    ] {
        *action = read_only_block();
    }
}

pub(crate) fn pipeline_facts(pipeline: &Pipeline) -> PipelineFacts<'_> {
    let local_id = pipeline_id::strip(&pipeline.provider_type, pipeline.provider_id, &pipeline.id)
        .unwrap_or(&pipeline.id);
    PipelineFacts {
        id: local_id,
        status: plugin_status(&pipeline.status),
        branch: pipeline.branch.as_deref(),
        workflow_file: pipeline.workflow_file.as_deref(),
        resource_state: pipeline.resource_state.as_ref(),
        metadata: &pipeline.metadata,
    }
}

pub(crate) fn run_facts(run: &PipelineRun) -> RunFacts<'_> {
    RunFacts {
        run_number: run.run_number,
        status: plugin_status(&run.status),
        branch: run.branch.as_deref(),
        actor: run.actor.as_deref(),
        commit_message: run.commit_message.as_deref(),
        inputs: run.inputs.as_ref(),
        metadata: &run.metadata,
    }
}

#[cfg(test)]
mod tests;
