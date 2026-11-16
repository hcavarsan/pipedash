use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::OwnedMutexGuard;

use super::super::{
    ProviderLease,
    ProviderService,
};
use crate::application::services::health_service::HealthInput;
use crate::domain::{
    DomainError,
    DomainResult,
    Provider,
    ProviderConfig,
};
use crate::infrastructure::config::sync::ProviderSyncService;
use crate::infrastructure::config::ProviderFileConfig;
use crate::infrastructure::reload_shadow::ShadowGuard;
use crate::infrastructure::TokenStore;

struct Prior {
    id: i64,
    row: Option<ProviderConfig>,
    token: Option<String>,
}

pub(super) struct Staged {
    pub(super) key: String,
    pub(super) created: bool,
    prior: Prior,
    config: ProviderConfig,
    instance: Arc<dyn Provider>,
}

pub(super) struct Reload {
    shadow: ShadowGuard,
    locks: Vec<OwnedMutexGuard<()>>,
    pub(super) staged: Vec<Staged>,
}

impl ProviderService {
    pub(super) async fn stage_providers(
        &self, changed: &[(String, ProviderFileConfig)], token_store: &dyn TokenStore,
    ) -> DomainResult<Reload> {
        if changed.is_empty() {
            return Ok(Reload {
                shadow: ShadowGuard::none(),
                locks: Vec::new(),
                staged: Vec::new(),
            });
        }
        let (mut priors, locks) = self.reserve_existing(changed).await?;
        let rows = priors
            .values()
            .filter_map(|prior| Some((prior.row.clone()?, prior.token.clone())))
            .collect();
        let created = changed
            .iter()
            .filter(|(key, _)| !priors.contains_key(key))
            .map(|(key, _)| key.clone())
            .collect();
        let mut reload = Reload {
            shadow: self.repository.shadow().hide(rows, created),
            locks,
            staged: Vec::with_capacity(changed.len()),
        };
        for (key, provider) in changed {
            let one = self
                .stage_one(
                    key,
                    provider,
                    priors.remove(key),
                    token_store,
                    &reload.shadow,
                    &mut reload.locks,
                )
                .await;
            match one {
                Ok(one) => reload.staged.push(one),
                Err(error) => return Err(self.unstage(reload, error).await),
            }
        }

        Ok(reload)
    }

    pub(super) async fn unstage(&self, mut reload: Reload, mut error: DomainError) -> DomainError {
        for one in std::mem::take(&mut reload.staged).into_iter().rev() {
            error = self.undo(&one.prior, error).await;
        }
        drop(reload);

        error
    }

    pub(super) async fn activate(&self, staged: Vec<Staged>) -> Vec<ProviderLease> {
        let mut leases = Vec::with_capacity(staged.len());
        for one in staged {
            let id = one.prior.id;
            self.reset_fetch_status(id).await;
            let lease = self
                .replace(id, one.instance, one.config.refresh_interval)
                .await;
            self.purge_run_logs(id).await;
            self.health.send(HealthInput::ProvidersChanged {
                changed: vec![self.health_input(&one.config, &lease)],
            });
            leases.push(lease);
        }

        leases
    }

    async fn reserve_existing(
        &self, changed: &[(String, ProviderFileConfig)],
    ) -> DomainResult<(HashMap<String, Prior>, Vec<OwnedMutexGuard<()>>)> {
        let rows = self.repository.config_backend().list_providers().await?;
        let mut wanted: Vec<(i64, &str)> = changed
            .iter()
            .filter_map(|(key, _)| {
                let id = rows.iter().find(|p| p.name == *key)?.id?;
                Some((id, key.as_str()))
            })
            .collect();
        wanted.sort_unstable();

        let mut priors = HashMap::with_capacity(wanted.len());
        let mut locks = Vec::with_capacity(wanted.len());
        for (id, key) in wanted {
            locks.push(self.commit_lock(id).lock_owned().await);
            let row = self
                .repository
                .config_backend()
                .get_provider(id)
                .await?
                .ok_or_else(|| DomainError::ProviderNotFound(id.to_string()))?;
            let token = self.repository.stored_token(id).await?;
            let prior = Prior {
                id,
                row: Some(row),
                token,
            };
            priors.insert(key.to_string(), prior);
        }

        Ok((priors, locks))
    }

    async fn stage_one(
        &self, key: &str, provider: &ProviderFileConfig, reserved: Option<Prior>,
        token_store: &dyn TokenStore, shadow: &ShadowGuard, locks: &mut Vec<OwnedMutexGuard<()>>,
    ) -> DomainResult<Staged> {
        let sync = ProviderSyncService::new();
        let backend = self.repository.config_backend().as_ref();
        let prior = match reserved {
            Some(prior) => {
                if let Some(row) = &prior.row {
                    sync.update_from_toml(key, provider, row, backend).await?;
                }
                prior
            }
            None => {
                let id = sync.create_from_toml(key, provider, backend).await?;
                shadow.created(id);
                locks.push(self.commit_lock(id).lock_owned().await);
                Prior {
                    id,
                    row: None,
                    token: None,
                }
            }
        };

        let built = async {
            ProviderSyncService::store_resolved_token(
                prior.id,
                &provider.token,
                prior.row.is_none(),
                token_store,
                &self.plugin_registry,
            )
            .await?;
            let config = self.repository.get_provider(prior.id).await?;
            let instance = (self.provider_factory)(&config)?;

            Ok::<_, DomainError>((config, instance))
        }
        .await;
        match built {
            Ok((config, instance)) => Ok(Staged {
                key: key.to_string(),
                created: prior.row.is_none(),
                prior,
                config,
                instance,
            }),
            Err(error) => Err(self.undo(&prior, error).await),
        }
    }

    async fn undo(&self, prior: &Prior, error: DomainError) -> DomainError {
        match &prior.row {
            None => self.repository.discard_provider(prior.id, error).await,
            Some(row) => {
                self.repository
                    .restore_provider(prior.id, row, prior.token.clone(), error)
                    .await
            }
        }
    }
}
