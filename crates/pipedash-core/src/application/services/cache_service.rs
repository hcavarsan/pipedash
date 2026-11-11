use serde::{
    Deserialize,
    Serialize,
};

use crate::domain::DomainResult;
use crate::CoreContext;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheStats {
    pub pipelines_count: i64,
    pub run_history_count: i64,
    pub workflow_params_count: i64,
    pub metrics_count: i64,
    pub run_logs_bytes: u64,
}

impl CoreContext {
    pub async fn cache_stats(&self) -> DomainResult<CacheStats> {
        let pipelines = &self.pipeline_service;
        let metrics_count = match &self.metrics_service {
            Some(metrics) => metrics
                .get_storage_stats()
                .await
                .map(|stats| stats.total_metrics_count)
                .unwrap_or(0),
            None => 0,
        };

        Ok(CacheStats {
            pipelines_count: pipelines.get_pipelines_cache_count().await.unwrap_or(0),
            run_history_count: pipelines.get_run_history_cache_count().await.unwrap_or(0),
            workflow_params_count: pipelines
                .get_workflow_params_cache_count()
                .await
                .unwrap_or(0),
            metrics_count,
            run_logs_bytes: self.log_service.stats().await.bytes,
        })
    }

    pub async fn clear_all_caches(&self) -> DomainResult<()> {
        self.pipeline_service.clear_all_caches_atomic().await?;
        self.log_service.clear().await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests;
