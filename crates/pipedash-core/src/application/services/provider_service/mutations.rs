use chrono::Utc;
use tokio::time::Instant;

use super::auth::TakenSignIn;
use super::plugin::keep_selection_with_ci;
use super::runtime::interval_of;
use super::{
    ProviderLease,
    ProviderService,
};
use crate::application::fetch_scheduler::ForcedClaim;
use crate::application::services::health_service::{
    HealthInput,
    ProviderHealthInput,
};
use crate::domain::{
    DomainError,
    DomainResult,
    ProviderConfig,
};
use crate::event::CoreEvent;

const REFRESH_INTERVAL_SECONDS: std::ops::RangeInclusive<i64> = 5..=300;

impl ProviderService {
    pub(super) async fn purge_run_logs(&self, id: i64) {
        if let Err(e) = self.log_store.purge_provider(id).await {
            tracing::warn!(provider_id = id, error = %e, "Failed to purge cached run logs");
        }
    }

    pub(super) async fn reset_fetch_status(&self, id: i64) {
        if let Err(e) = self.repository.reset_provider_fetch_status(id).await {
            tracing::warn!(provider_id = id, error = %e, "Failed to reset fetch status");
        }
    }

    pub(super) fn queue_fetch(&self, lease: &ProviderLease) {
        if let ForcedClaim::Claimed(claim) =
            self.scheduler
                .claim_forced(lease.id, lease.generation, Instant::now())
        {
            self.dispatcher.fetch_now(vec![claim]);
        }
    }

    pub async fn add_provider(&self, mut config: ProviderConfig) -> DomainResult<i64> {
        let sign_in = self.take_sign_in(&mut config, None).await?;
        let plugin = self
            .init_plugin(&config.provider_type, config.plugin_config(), 0)
            .await?;

        let (validation_result, permission_status) =
            tokio::join!(plugin.validate_credentials(), plugin.check_permissions());

        validation_result.map_err(|e| {
            DomainError::InvalidConfig(format!("Credential validation failed: {e}"))
        })?;

        keep_selection_with_ci(plugin.as_ref(), &mut config).await?;

        let permission_status = permission_status.ok();

        let id = self.repository.add_provider(&config).await?;

        if let Some(status) = &permission_status {
            self.store_permissions(id, status).await;
        }

        let mut config_with_id = config.clone();
        config_with_id.id = Some(id);
        let provider = (self.provider_factory)(&config_with_id)?;

        let lock = self.commit_lock(id);
        let _guard = lock.lock().await;
        let lease = self.install(id, provider, config.refresh_interval).await;
        self.health.send(HealthInput::ProvidersChanged {
            changed: vec![self.health_input(&config_with_id, &lease)],
        });
        let provider = self.summary_of(id).await?;
        self.event_bus
            .emit(CoreEvent::ProviderAdded {
                provider,
                timestamp: Utc::now().timestamp_millis(),
            })
            .await;
        if let TakenSignIn::Flow(flow) = sign_in {
            self.sign_ins.forget(&flow);
        }

        Ok(id)
    }

    pub async fn update_provider(&self, id: i64, mut config: ProviderConfig) -> DomainResult<()> {
        let saved_name = self.repository.get_provider(id).await?.name;
        if self
            .config_state
            .get()
            .await
            .providers
            .contains_key(&saved_name)
        {
            let display_name = std::mem::replace(&mut config.name, saved_name);
            config
                .config
                .insert("display_name".to_string(), display_name);
        }
        let sign_in = self.take_sign_in(&mut config, Some(id)).await?;
        let plugin = self
            .init_plugin(&config.provider_type, config.plugin_config(), id)
            .await?;

        plugin.validate_credentials().await.map_err(|e| {
            DomainError::InvalidConfig(format!("Credential validation failed: {e}"))
        })?;

        keep_selection_with_ci(plugin.as_ref(), &mut config).await?;

        let permission_status = plugin.check_permissions().await.ok();

        let lock = self.commit_lock(id);
        let guard = lock.lock().await;

        let current = self.repository.get_provider(id).await?;
        if matches!(sign_in, TakenSignIn::Kept) {
            config.token.clone_from(&current.token);
        }
        let mut config_with_id = config.clone();
        config_with_id.id = Some(id);
        let new_provider = (self.provider_factory)(&config_with_id)?;
        let success = self
            .repository
            .update_provider_with_version(id, &config, current.version.unwrap_or(1))
            .await?;

        if !success {
            return Err(DomainError::ConcurrentModification(format!(
                "Provider {} was modified by another client",
                id
            )));
        }

        self.reset_fetch_status(id).await;
        let lease = self
            .replace(id, new_provider, config.refresh_interval)
            .await;
        self.purge_run_logs(id).await;
        self.health.send(HealthInput::ProvidersChanged {
            changed: vec![self.health_input(&config_with_id, &lease)],
        });
        self.emit_updated(id).await;
        drop(guard);

        if let Some(status) = &permission_status {
            self.store_permissions(id, status).await;
        }

        self.queue_fetch(&lease);
        if let TakenSignIn::Flow(flow) = sign_in {
            self.sign_ins.forget(&flow);
        }

        Ok(())
    }

    pub async fn update_provider_refresh_interval(
        &self, id: i64, refresh_interval: i64,
    ) -> DomainResult<()> {
        if !REFRESH_INTERVAL_SECONDS.contains(&refresh_interval) {
            return Err(DomainError::InvalidConfig(
                "Enter a whole number from 5 to 300 seconds.".into(),
            ));
        }

        let lock = self.commit_lock(id);
        let _guard = lock.lock().await;

        let mut config = self.repository.get_provider(id).await?;
        config.refresh_interval = refresh_interval;
        self.repository.update_provider(id, &config).await?;

        let generation = self
            .providers
            .read()
            .await
            .get(&id)
            .map(|slot| slot.generation);
        if let Some(generation) = generation {
            self.scheduler
                .set_interval(id, generation, interval_of(refresh_interval));
        }

        Ok(())
    }

    pub async fn remove_provider(&self, id: i64) -> DomainResult<()> {
        let _apply = self.config_apply.lock().await;
        let lock = self.commit_lock(id);
        let _guard = lock.lock().await;

        let config = self.repository.get_provider(id).await?;
        let provider = self.build_summary(id, &config).await?;

        self.repository
            .remove_provider(id, &self.config_state)
            .await?;
        self.providers.write().await.remove(&id);
        self.purge_run_logs(id).await;
        self.scheduler.remove(id);
        self.health.send(HealthInput::ProvidersChanged {
            changed: vec![ProviderHealthInput {
                id,
                generation: None,
                display_name: config.display_name().to_string(),
                source: None,
                has_instance_checks: false,
            }],
        });
        self.event_bus
            .emit(CoreEvent::ProviderRemoved {
                provider,
                timestamp: Utc::now().timestamp_millis(),
            })
            .await;

        Ok(())
    }
}

#[cfg(test)]
mod tests;
