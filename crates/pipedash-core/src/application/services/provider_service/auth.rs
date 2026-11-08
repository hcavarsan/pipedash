use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::Mutex;

use super::plugin::build_plugin;
use super::{
    ProviderLease,
    ProviderService,
};
use crate::application::services::sign_in_service::ProviderSignIn;
use crate::domain::{
    DomainError,
    DomainResult,
    ProviderConfig,
    SignInCredential,
    SignInRef,
    SignInStart,
    SignInStatus,
    SIGN_IN_CONFIG_KEY,
};
use crate::infrastructure::oauth;
use crate::infrastructure::providers::PluginAdapter;

pub(super) enum TakenSignIn {
    None,
    Flow(String),
    Kept,
}

impl ProviderService {
    fn renewal_lock(&self, id: i64) -> Arc<Mutex<()>> {
        Arc::clone(self.renewal_locks.entry(id).or_default().value())
    }

    pub(super) async fn resolve_sign_in(
        &self, config: &mut HashMap<String, String>,
    ) -> DomainResult<()> {
        let Some(reference) = config.remove(SIGN_IN_CONFIG_KEY) else {
            return Ok(());
        };
        let token = match SignInRef::parse(&reference) {
            SignInRef::Flow(flow) => self.sign_ins.credential(&flow).await?.token,
            SignInRef::Provider(id) => {
                SignInCredential::bearer(&self.renewed_config(id).await?.token)
            }
        };
        config.insert("token".to_string(), token);
        Ok(())
    }

    pub(super) async fn take_sign_in(
        &self, config: &mut ProviderConfig, owner: Option<i64>,
    ) -> DomainResult<TakenSignIn> {
        let Some(reference) = config.config.remove(SIGN_IN_CONFIG_KEY) else {
            return Ok(TakenSignIn::None);
        };
        match SignInRef::parse(&reference) {
            SignInRef::Flow(flow) => {
                self.sign_ins.check_owner(&flow, owner)?;
                config.token = self.sign_ins.credential(&flow).await?.encode()?;
                Ok(TakenSignIn::Flow(flow))
            }
            SignInRef::Provider(id) if Some(id) == owner => {
                config.token = self.renewed_config(id).await?.token;
                Ok(TakenSignIn::Kept)
            }
            SignInRef::Provider(_) => Err(DomainError::InvalidConfig(
                "This sign-in belongs to another provider".into(),
            )),
        }
    }

    pub(super) async fn renewed_config(&self, id: i64) -> DomainResult<ProviderConfig> {
        let renewal = self.renewal_lock(id);
        let _renewal = renewal.lock().await;
        let config = self.repository.get_provider(id).await?;
        let Some(current) = SignInCredential::decode(&config.token)
            .filter(|credential| credential.renew_at().is_some_and(|at| at <= Utc::now()))
        else {
            return Ok(config);
        };

        let (plugin, http) = build_plugin(
            &self.plugin_registry,
            &self.http_client_manager,
            &config.provider_type,
            &config.config,
        )?;
        let token = oauth::refresh(&http, &current, plugin.sign_in_refresh_rejections())
            .await?
            .encode()?;

        let commit = self.commit_lock(id);
        let _commit = commit.lock().await;
        let mut latest = self.repository.get_provider(id).await?;
        if latest.token != config.token {
            return Ok(latest);
        }
        self.repository.store_token(id, &token).await?;
        tracing::info!(provider_id = id, "Renewed the provider sign-in");
        latest.token = token;
        Ok(latest)
    }

    pub(super) async fn renew(&self, stale: ProviderLease) -> DomainResult<ProviderLease> {
        let config = self.renewed_config(stale.id).await?;
        let provider = (self.provider_factory)(&config)?;
        let mut providers = self.providers.write().await;
        let slot = providers
            .get_mut(&stale.id)
            .ok_or_else(|| DomainError::ProviderNotFound(stale.id.to_string()))?;
        if slot.generation == stale.generation {
            slot.provider = provider;
        }
        Ok(slot.lease(stale.id))
    }

    pub async fn provider_for_editing(&self, id: i64) -> DomainResult<ProviderConfig> {
        let mut config = self.repository.get_provider(id).await?;
        if SignInCredential::decode(&config.token).is_some() {
            config.token.clear();
            config.config.insert(
                SIGN_IN_CONFIG_KEY.to_string(),
                SignInRef::Provider(id).to_string(),
            );
        }
        Ok(config)
    }

    pub async fn start_sign_in(
        &self, provider_type: &str, config: HashMap<String, String>,
    ) -> DomainResult<SignInStart> {
        self.start_sign_in_with_owner(provider_type, config, None)
            .await
    }

    pub async fn start_provider_sign_in(&self, id: i64) -> DomainResult<SignInStart> {
        let config = self.repository.get_provider(id).await?;
        self.start_sign_in_with_owner(
            &config.provider_type,
            config.config,
            Some((id, config.token)),
        )
        .await
    }

    async fn start_sign_in_with_owner(
        &self, provider_type: &str, config: HashMap<String, String>, owner: Option<(i64, String)>,
    ) -> DomainResult<SignInStart> {
        let (plugin, http) = build_plugin(
            &self.plugin_registry,
            &self.http_client_manager,
            provider_type,
            &config,
        )?;
        let client = plugin
            .sign_in_client(&config, &http)
            .await
            .map_err(PluginAdapter::map_error)?;
        let owner = owner.map(|(id, token)| ProviderSignIn {
            id,
            provider_type: provider_type.to_string(),
            token,
            client: client.clone(),
        });
        self.sign_ins
            .start(client, http, plugin.sign_in_refresh_rejections(), owner)
            .await
    }

    pub async fn sign_in_status(&self, reference: &str) -> DomainResult<SignInStatus> {
        match SignInRef::parse(reference) {
            SignInRef::Flow(flow) => Ok(self.sign_ins.status(&flow)),
            SignInRef::Provider(id) => match self.renewed_config(id).await {
                Ok(config) => Ok(SignInCredential::decode(&config.token).map_or(
                    SignInStatus::Expired,
                    |credential| SignInStatus::Completed {
                        account: credential.account,
                    },
                )),
                Err(DomainError::ProviderAuthenticationFailed(message)) => {
                    Ok(SignInStatus::Failed { message })
                }
                Err(e) => Err(e),
            },
        }
    }

    pub async fn complete_sign_in(
        &self, flow: &str, redirect_url: &str,
    ) -> DomainResult<SignInStatus> {
        self.sign_ins.complete(flow, redirect_url).await
    }

    pub async fn cancel_sign_in(&self, flow: &str) {
        self.sign_ins.cancel(flow).await;
    }

    pub async fn wait_for_sign_in(&self, flow: &str) -> SignInStatus {
        self.sign_ins.wait(flow).await
    }
}
