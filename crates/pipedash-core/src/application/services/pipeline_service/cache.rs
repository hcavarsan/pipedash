use super::PipelineService;
use crate::domain::DomainResult;

impl PipelineService {
    pub async fn clear_all_run_history_caches(&self) -> DomainResult<()> {
        self.repository.clear_all_run_history_cache().await
    }

    pub async fn get_pipelines_cache_count(&self) -> DomainResult<i64> {
        self.repository.get_pipelines_cache_count().await
    }

    pub async fn get_run_history_cache_count(&self) -> DomainResult<i64> {
        self.repository.get_run_history_cache_count().await
    }

    pub async fn get_workflow_params_cache_count(&self) -> DomainResult<i64> {
        self.repository.get_workflow_params_cache_count().await
    }

    pub async fn clear_pipelines_cache(&self) -> DomainResult<usize> {
        self.repository.clear_pipelines_cache().await
    }

    pub async fn clear_workflow_params_cache(&self) -> DomainResult<()> {
        self.repository.clear_workflow_parameters_cache().await
    }

    pub async fn clear_all_caches_atomic(&self) -> DomainResult<()> {
        self.repository.clear_all_caches_atomic().await
    }

    pub(super) async fn invalidate_run_cache(&self, pipeline_id: &str) {
        if pipeline_id
            .split_once("__")
            .is_some_and(|(provider_type, _)| {
                self.provider_service.retains_run_history(provider_type)
            })
        {
            return;
        }
        let _ = self.repository.clear_cached_run_history(pipeline_id).await;
    }
}
