use std::sync::Arc;

use pipedash_plugin_api::{
    is_no_auth,
    ActionAvailability,
    RerunStrategy,
    ReviewDecision,
    TriggerResult,
};

use super::PipelineService;
use super::READ_ONLY_PROVIDER;
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
    Provider,
    TriggerParams,
};
use crate::event::CoreEvent;

fn require(availability: &ActionAvailability, action: &str) -> DomainResult<()> {
    if availability.allowed {
        return Ok(());
    }
    Err(DomainError::NotSupported(
        availability
            .reason
            .clone()
            .unwrap_or_else(|| format!("{action} is not available for this pipeline")),
    ))
}

impl PipelineService {
    pub(super) async fn presented_pipeline(&self, pipeline_id: &str) -> DomainResult<Pipeline> {
        let mut pipeline = self.repository.get_cached_pipeline(pipeline_id).await?;
        self.presenter
            .pipelines(std::slice::from_mut(&mut pipeline))
            .await?;
        Ok(pipeline)
    }

    async fn presented_run(
        &self, provider: &dyn Provider, pipeline: &Pipeline, run_number: i64,
    ) -> DomainResult<PipelineRun> {
        let mut run = self
            .fetch_run_details_from(provider, pipeline, run_number)
            .await?;
        self.presenter
            .runs(pipeline, std::slice::from_mut(&mut run))
            .await?;
        Ok(run)
    }

    async fn writable(&self, pipeline_id: &str) -> DomainResult<(Pipeline, Arc<dyn Provider>)> {
        let pipeline = self.presented_pipeline(pipeline_id).await?;
        self.refuse_read_only(pipeline.provider_id).await?;
        let provider = self
            .provider_service
            .get_provider(pipeline.provider_id)
            .await?;
        Ok((pipeline, provider))
    }

    async fn writable_run(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<(Pipeline, Arc<dyn Provider>, PipelineRun)> {
        let (pipeline, provider) = self.writable(pipeline_id).await?;
        let run = self
            .presented_run(provider.as_ref(), &pipeline, run_number)
            .await?;
        Ok((pipeline, provider, run))
    }

    pub async fn trigger_pipeline(&self, params: TriggerParams) -> DomainResult<TriggerResult> {
        let (pipeline, provider) = self.writable(&params.workflow_id).await?;
        require(&pipeline.actions.trigger, "Trigger")?;
        let result = provider.trigger_pipeline(params.clone()).await?;
        self.invalidate_run_cache(&params.workflow_id).await;

        self.event_bus
            .emit(CoreEvent::RunTriggered {
                workflow_id: params.workflow_id,
            })
            .await;

        Ok(result)
    }

    pub async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        let (pipeline, provider) = self.writable(pipeline_id).await?;
        require(&pipeline.actions.cancel, "Cancel")?;
        let run = self
            .presented_run(provider.as_ref(), &pipeline, run_number)
            .await?;
        require(&run.actions.cancel, "Cancel")?;
        provider.cancel_run(pipeline_id, run_number).await?;
        self.invalidate_run_cache(pipeline_id).await;

        self.event_bus
            .emit(CoreEvent::RunCancelled {
                pipeline_id: pipeline_id.to_string(),
            })
            .await;

        Ok(())
    }

    pub async fn force_cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        let (pipeline, provider) = self.writable(pipeline_id).await?;
        require(&pipeline.actions.force_cancel, "Force cancellation")?;
        let run = self
            .presented_run(provider.as_ref(), &pipeline, run_number)
            .await?;
        require(&run.actions.force_cancel, "Force cancellation")?;
        provider.force_cancel_run(pipeline_id, run_number).await?;
        self.invalidate_run_cache(pipeline_id).await;
        self.event_bus
            .emit(CoreEvent::RunCancelled {
                pipeline_id: pipeline_id.to_string(),
            })
            .await;

        Ok(())
    }

    pub async fn review_run(
        &self, pipeline_id: &str, run_number: i64, decision: ReviewDecision,
    ) -> DomainResult<()> {
        let (_, provider, run) = self.writable_run(pipeline_id, run_number).await?;
        require(&run.actions.review, "Review")?;
        provider
            .review_run(pipeline_id, run_number, decision)
            .await?;

        self.invalidate_run_cache(pipeline_id).await;

        Ok(())
    }

    pub async fn rerun_failed_jobs(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        let (_, provider, run) = self.writable_run(pipeline_id, run_number).await?;
        require(&run.actions.rerun_failed, "Rerun failed jobs")?;
        provider.rerun_failed_jobs(pipeline_id, run_number).await?;

        self.invalidate_run_cache(pipeline_id).await;

        Ok(())
    }

    pub async fn rerun_run(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<TriggerResult> {
        let (pipeline, provider, run) = self.writable_run(pipeline_id, run_number).await?;
        require(&run.actions.rerun, "Rerun")?;

        let inputs = match pipeline.actions.rerun_strategy {
            RerunStrategy::InPlace => {
                provider.rerun_run(pipeline_id, run_number).await?;
                self.invalidate_run_cache(pipeline_id).await;
                return Ok(TriggerResult::default());
            }
            RerunStrategy::ReplayInputs => {
                let draft = self
                    .prepare_trigger(pipeline_id, Some(run_number), None)
                    .await?;
                Some(serde_json::Value::Object(draft.values))
            }
            RerunStrategy::ReconcileCurrent => None,
        };

        self.trigger_pipeline(TriggerParams {
            workflow_id: pipeline_id.to_string(),
            inputs,
        })
        .await
    }

    pub async fn enable_pipeline(&self, pipeline_id: &str) -> DomainResult<()> {
        let (pipeline, provider) = self.writable(pipeline_id).await?;
        require(&pipeline.actions.enable, "Enable")?;
        provider.enable_pipeline(pipeline_id).await
    }

    async fn refuse_read_only(&self, provider_id: i64) -> DomainResult<()> {
        let config = self
            .provider_service
            .get_provider_config(provider_id)
            .await?;
        if is_no_auth(&config.config) {
            return Err(DomainError::NotSupported(READ_ONLY_PROVIDER.to_string()));
        }
        Ok(())
    }
}
