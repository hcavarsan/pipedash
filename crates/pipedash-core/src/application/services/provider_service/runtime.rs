use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Mutex;

use super::{
    ProviderLease,
    ProviderService,
    ProviderSlot,
    ACQUIRE_ATTEMPTS,
};
use crate::application::fetch_scheduler::Generation;
use crate::application::services::health_service::{
    HealthInput,
    ProviderHealthInput,
};
use crate::domain::{
    DomainError,
    DomainResult,
    Provider,
};

impl ProviderService {
    pub(crate) fn commit_lock(&self, id: i64) -> Arc<Mutex<()>> {
        Arc::clone(self.commit_locks.entry(id).or_default().value())
    }

    pub(super) async fn current(&self, id: i64) -> Option<ProviderLease> {
        self.providers
            .read()
            .await
            .get(&id)
            .map(|slot| slot.lease(id))
    }

    pub(crate) async fn generation_of(&self, id: i64) -> Option<Generation> {
        self.providers
            .read()
            .await
            .get(&id)
            .map(|slot| slot.generation)
    }

    pub(crate) async fn health_inputs(&self) -> Vec<ProviderHealthInput> {
        let leases: Vec<ProviderLease> = self
            .providers
            .read()
            .await
            .iter()
            .map(|(id, slot)| slot.lease(*id))
            .collect();
        let mut inputs = Vec::with_capacity(leases.len());
        for lease in leases {
            match self.repository.get_provider(lease.id).await {
                Ok(config) => inputs.push(self.health_input(&config, &lease)),
                Err(e) => {
                    tracing::debug!(provider_id = lease.id, error = %e, "Provider has no config row");
                }
            }
        }

        inputs
    }

    pub(super) async fn install(
        &self, id: i64, provider: Arc<dyn Provider>, interval: i64,
    ) -> ProviderLease {
        let mut providers = self.providers.write().await;
        providers
            .entry(id)
            .or_insert_with(|| {
                let generation = Generation::next();
                self.scheduler
                    .register(id, generation, interval_of(interval));
                ProviderSlot {
                    generation,
                    provider,
                }
            })
            .lease(id)
    }

    pub(super) async fn replace(
        &self, id: i64, provider: Arc<dyn Provider>, interval: i64,
    ) -> ProviderLease {
        let generation = Generation::next();
        let mut providers = self.providers.write().await;
        providers.insert(
            id,
            ProviderSlot {
                generation,
                provider: Arc::clone(&provider),
            },
        );
        self.scheduler.reset(id, generation, interval_of(interval));

        ProviderLease {
            id,
            generation,
            provider,
        }
    }

    pub async fn acquire(&self, id: i64) -> DomainResult<ProviderLease> {
        if let Some(lease) = self.current(id).await {
            if lease.provider.renew_at().is_some_and(|at| at <= Utc::now()) {
                return self.renew(lease).await;
            }
            return Ok(lease);
        }

        for _ in 0..ACQUIRE_ATTEMPTS {
            let config = self.renewed_config(id).await?;
            let provider = (self.provider_factory)(&config)?;
            let lock = self.commit_lock(id);
            let _guard = lock.lock().await;
            if let Some(lease) = self.current(id).await {
                return Ok(lease);
            }
            let row = self.repository.get_provider(id).await?;
            if row.version == config.version {
                let lease = self.install(id, provider, row.refresh_interval).await;
                self.health.send(HealthInput::ProvidersChanged {
                    changed: vec![self.health_input(&row, &lease)],
                });
                return Ok(lease);
            }
        }

        Err(DomainError::ConcurrentModification(format!(
            "Provider {id} kept changing while it was loaded"
        )))
    }

    pub async fn commit<F, Fut, T>(&self, lease: &ProviderLease, f: F) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        self.commit_at(lease.id, lease.generation, f).await
    }

    pub(super) async fn commit_at<F, Fut, T>(
        &self, id: i64, generation: Generation, f: F,
    ) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let lock = self.commit_lock(id);
        let _guard = lock.lock().await;
        let current = self
            .providers
            .read()
            .await
            .get(&id)
            .map(|slot| slot.generation);
        if current != Some(generation) {
            return None;
        }

        Some(f().await)
    }

    pub async fn get_provider(&self, id: i64) -> DomainResult<Arc<dyn Provider>> {
        Ok(self.acquire(id).await?.provider)
    }

    pub async fn load_missing_providers(&self) {
        let ids = match self.repository.list_provider_ids().await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list providers");
                return;
            }
        };
        let loaded: Vec<i64> = self.providers.read().await.keys().copied().collect();
        for id in ids.into_iter().filter(|id| !loaded.contains(id)) {
            if let Err(e) = self.acquire(id).await {
                tracing::debug!(provider_id = id, error = %e, "Provider not loaded yet");
            }
        }
    }

    pub async fn load_all_providers(&self) -> DomainResult<()> {
        let configs = self.repository.list_providers().await?;
        let mut changed = Vec::new();

        for config in configs {
            let Some(id) = config.id else {
                continue;
            };
            if config.token.is_empty() {
                tracing::debug!(
                    provider_id = id,
                    provider_type = %config.provider_type,
                    "Skipping provider load - token not available yet (vault initializing in background)"
                );
                continue;
            }

            match (self.provider_factory)(&config) {
                Ok(provider) => {
                    let lock = self.commit_lock(id);
                    let _guard = lock.lock().await;
                    let lease = self.replace(id, provider, config.refresh_interval).await;
                    changed.push(self.health_input(&config, &lease));
                }
                Err(e) => {
                    tracing::warn!(provider_id = id, error = %e, "Failed to load provider");
                }
            }
        }

        if !changed.is_empty() {
            self.health.send(HealthInput::ProvidersChanged { changed });
        }

        Ok(())
    }
}

pub(super) fn interval_of(seconds: i64) -> Duration {
    Duration::from_secs(u64::try_from(seconds).unwrap_or(0))
}
