use std::sync::Arc;

use chrono::Utc;

use crate::domain::{
    AggregatedMetrics,
    DomainError,
    DomainResult,
    GlobalMetricsConfig,
    MetricsConfig,
    MetricsQuery,
    MetricsStats,
};
use crate::event::{
    CoreEvent,
    EventBus,
};
use crate::infrastructure::database::MetricsRepository;

mod dashboard;
mod extract;
#[cfg(test)]
mod tests;

pub struct MetricsService {
    repository: Arc<MetricsRepository>,
    event_bus: Arc<dyn EventBus>,
}

impl MetricsService {
    pub fn new(repository: Arc<MetricsRepository>, event_bus: Arc<dyn EventBus>) -> Self {
        Self {
            repository,
            event_bus,
        }
    }

    pub fn repository(&self) -> Arc<MetricsRepository> {
        Arc::clone(&self.repository)
    }

    pub async fn get_global_config(&self) -> DomainResult<GlobalMetricsConfig> {
        self.repository.get_global_config().await
    }

    pub async fn update_global_config(
        &self, enabled: bool, default_retention_days: i64,
    ) -> DomainResult<()> {
        if default_retention_days < 1 {
            return Err(DomainError::InvalidConfig(
                "Retention days must be at least 1".to_string(),
            ));
        }

        let previous_config = self.repository.get_global_config().await?;

        self.repository
            .update_global_config(enabled, default_retention_days)
            .await?;
        self.event_bus
            .emit(CoreEvent::MetricsGlobalConfigChanged)
            .await;

        if enabled && !previous_config.enabled {
            tracing::info!("Global metrics re-enabled - resetting all processing states");
            self.repository.reset_all_processing_states().await?;
        }

        Ok(())
    }

    pub async fn get_effective_pipeline_config(
        &self, pipeline_id: &str,
    ) -> DomainResult<MetricsConfig> {
        if let Some(config) = self.repository.get_pipeline_config(pipeline_id).await? {
            Ok(config)
        } else {
            let global_config = self.repository.get_global_config().await?;
            Ok(MetricsConfig {
                pipeline_id: pipeline_id.to_string(),
                enabled: global_config.enabled,
                retention_days: global_config.default_retention_days,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
        }
    }

    pub async fn update_pipeline_config(
        &self, pipeline_id: &str, enabled: bool, retention_days: i64,
    ) -> DomainResult<()> {
        if retention_days < 1 {
            return Err(DomainError::InvalidConfig(
                "Retention days must be at least 1".to_string(),
            ));
        }

        let previous_config = self.get_effective_pipeline_config(pipeline_id).await?;

        self.repository
            .upsert_pipeline_config(pipeline_id, enabled, retention_days)
            .await?;
        self.event_bus
            .emit(CoreEvent::MetricsConfigChanged {
                pipeline_id: pipeline_id.to_string(),
            })
            .await;

        if enabled && !previous_config.enabled {
            tracing::info!(
                pipeline_id = %pipeline_id,
                "Metrics re-enabled for pipeline - resetting processing state"
            );
            self.repository.reset_processing_state(pipeline_id).await?;
        }

        Ok(())
    }

    #[cfg(test)]
    pub(crate) async fn query_metrics(
        &self, query: MetricsQuery,
    ) -> DomainResult<Vec<crate::domain::MetricEntry>> {
        self.repository.query_metrics(query).await
    }

    pub async fn query_aggregated_metrics(
        &self, query: MetricsQuery,
    ) -> DomainResult<AggregatedMetrics> {
        self.repository.query_aggregated_metrics(query).await
    }

    pub async fn cleanup_old_metrics(&self) -> DomainResult<usize> {
        self.repository.delete_old_metrics(None).await
    }

    pub async fn get_storage_stats(&self) -> DomainResult<MetricsStats> {
        self.repository.get_storage_stats().await
    }

    pub async fn flush_metrics(
        &self, pipeline_id: Option<&str>, skip_vacuum: bool,
    ) -> DomainResult<usize> {
        let result = self
            .repository
            .flush_metrics(pipeline_id, skip_vacuum)
            .await?;

        if let Some(pipeline) = pipeline_id {
            self.repository.reset_processing_state(pipeline).await?;
        } else {
            self.repository.reset_all_processing_states().await?;
        }

        Ok(result)
    }
}
