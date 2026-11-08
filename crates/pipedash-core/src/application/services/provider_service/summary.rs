use chrono::Utc;
use tokio::time::Instant;

use super::attention::provider_attention;
use super::plugin::controller_health;
use super::{
    ProviderLease,
    ProviderService,
};
use crate::application::fetch_scheduler::Generation;
use crate::application::services::health_service::ProviderHealthInput;
use crate::domain::provider_attention::ProviderAttention;
use crate::domain::{
    DomainResult,
    FetchStatus,
    HealthLevel,
    ProviderConfig,
    ProviderFreshness,
    ProviderHealth,
    ProviderRecovery,
    ProviderSummary,
    SignInCredential,
};
use crate::event::CoreEvent;
use crate::infrastructure::database::{
    ProviderFetchStatus,
    FETCH_STATUS_ERROR,
    FETCH_STATUS_NEVER,
    FETCH_STATUS_SUCCESS,
};

impl ProviderService {
    pub async fn publish_summary(&self, id: i64, generation: Generation) {
        self.commit_at(id, generation, || self.emit_status(id))
            .await;
    }

    pub(crate) async fn emit_status(&self, id: i64) {
        match self.summary_of(id).await {
            Ok(provider) => {
                self.event_bus
                    .emit(CoreEvent::ProviderStatusUpdated { provider })
                    .await;
            }
            Err(e) => {
                tracing::warn!(provider_id = id, error = %e, "Failed to build provider summary");
            }
        }
    }

    pub(super) async fn summary_of(&self, id: i64) -> DomainResult<ProviderSummary> {
        let config = self.repository.get_provider(id).await?;

        self.build_summary(id, &config).await
    }

    pub(super) async fn build_summary(
        &self, id: i64, config: &ProviderConfig,
    ) -> DomainResult<ProviderSummary> {
        let cached_pipelines = self.repository.get_cached_pipelines(Some(id)).await?;
        let fetch = match self.repository.get_provider_fetch_status(id).await {
            Ok(fetch) => fetch,
            Err(e) => {
                tracing::warn!(provider_id = id, error = %e, "Failed to read provider fetch status");
                ProviderFetchStatus {
                    status: FETCH_STATUS_NEVER.to_string(),
                    error: None,
                    fetched_at: None,
                    succeeded_at: None,
                }
            }
        };
        let last_fetch_status = match fetch.status.as_str() {
            FETCH_STATUS_SUCCESS => FetchStatus::Success,
            FETCH_STATUS_ERROR => FetchStatus::Error,
            _ => FetchStatus::Never,
        };
        let now = Instant::now();
        let wall = Utc::now();
        let view = self.scheduler.view(id, now);
        let generation = self
            .providers
            .read()
            .await
            .get(&id)
            .map(|slot| slot.generation);
        let snapshot = self.health.snapshot();
        let state = snapshot
            .providers
            .get(&id)
            .filter(|state| Some(state.generation) == generation);
        let paused_until = self.scheduler.paused_until(id, wall);
        let level = state.map_or(HealthLevel::Operational, |state| state.level);
        let freshness = ProviderFreshness::compute(
            &last_fetch_status,
            level,
            fetch.fetched_at,
            paused_until,
            config.refresh_interval,
            wall,
        );

        let last_fetch_error_kind = view.as_ref().and_then(|view| view.last_failure);
        let recovery = ProviderRecovery::for_failure(
            matches!(last_fetch_status, FetchStatus::Error)
                .then_some(last_fetch_error_kind)
                .flatten(),
            SignInCredential::decode(&config.token).is_some(),
        );

        let mut summary = ProviderSummary {
            id,
            name: config.display_name().to_string(),
            provider_type: config.provider_type.clone(),
            icon: self
                .plugin_registry
                .metadata_of(&config.provider_type)
                .and_then(|metadata| metadata.icon.clone()),
            pipeline_count: cached_pipelines.len(),
            last_updated: cached_pipelines.iter().map(|p| p.last_updated).max(),
            refresh_interval: config.refresh_interval,
            configured_repositories: self
                .plugin_registry
                .get(&config.provider_type)
                .map(|plugin| plugin.configured_repositories(&config.config))
                .unwrap_or_default(),
            last_fetch_status,
            last_fetch_error: fetch.error,
            last_fetch_at: fetch.fetched_at,
            last_success_at: fetch.succeeded_at,
            paused_until,
            last_fetch_error_kind,
            health: ProviderHealth {
                level,
                signals: state.map(|state| state.signals.clone()).unwrap_or_default(),
                next_attempt_at: view
                    .as_ref()
                    .filter(|view| view.failures > 0)
                    .and_then(|_| {
                        let floor = state
                            .filter(|state| state.outage_floor)
                            .map(|_| self.scheduler.policy().outage_floor);
                        self.scheduler.next_due_in(id, now, floor)
                    })
                    .and_then(|wait| chrono::Duration::from_std(wait).ok())
                    .map(|wait| wall + wait),
                in_flight: view.as_ref().is_some_and(|view| view.in_flight),
                status_page: state.and_then(|state| state.status_page.clone()),
            },
            version: config.version.unwrap_or(1),
            freshness,
            recovery,
            attention: ProviderAttention::default(),
        };
        summary.attention = provider_attention(&summary);

        Ok(summary)
    }

    pub(super) fn health_input(
        &self, config: &ProviderConfig, lease: &ProviderLease,
    ) -> ProviderHealthInput {
        ProviderHealthInput {
            id: lease.id,
            generation: Some(lease.generation),
            display_name: config.display_name().to_string(),
            source: lease.provider.status_source().cloned(),
            has_instance_checks: controller_health(&self.plugin_registry, &config.provider_type),
        }
    }

    pub(super) async fn emit_updated(&self, id: i64) {
        match self.summary_of(id).await {
            Ok(provider) => {
                self.event_bus
                    .emit(CoreEvent::ProviderUpdated {
                        provider,
                        timestamp: Utc::now().timestamp_millis(),
                    })
                    .await;
            }
            Err(e) => {
                tracing::warn!(provider_id = id, error = %e, "Failed to build updated provider summary");
                self.event_bus.emit(CoreEvent::ProvidersChanged).await;
            }
        }
    }

    pub async fn get_provider_config(&self, id: i64) -> DomainResult<ProviderConfig> {
        self.repository.get_provider(id).await
    }

    pub async fn list_providers(&self) -> DomainResult<Vec<ProviderSummary>> {
        let configs = self.repository.list_providers().await?;
        let mut summaries = Vec::with_capacity(configs.len());

        for config in configs {
            if let Some(id) = config.id {
                summaries.push(self.build_summary(id, &config).await?);
            }
        }

        Ok(summaries)
    }
}
