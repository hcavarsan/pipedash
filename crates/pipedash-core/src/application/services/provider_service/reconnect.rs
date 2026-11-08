use super::plugin::build_plugin;
use super::{
    ProviderService,
    ACQUIRE_ATTEMPTS,
};
use crate::application::services::health_service::HealthInput;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::providers::PluginAdapter;

impl ProviderService {
    pub async fn reconnect_provider(&self, id: i64, flow: &str) -> DomainResult<()> {
        let owner = self.sign_ins.provider_flow(flow, id)?;
        let token = self.sign_ins.credential(flow).await?.encode()?;

        for _ in 0..ACQUIRE_ATTEMPTS {
            let config = self.repository.get_provider(id).await?;
            if config.provider_type != owner.provider_type || config.token != owner.token {
                return Err(DomainError::ConcurrentModification(
                    "This provider's authentication changed. Start signing in again.".into(),
                ));
            }
            let (plugin, http) = build_plugin(
                &self.plugin_registry,
                &self.http_client_manager,
                &config.provider_type,
                &config.config,
            )?;
            let client = plugin
                .sign_in_client(&config.config, &http)
                .await
                .map_err(PluginAdapter::map_error)?;
            if client != owner.client {
                return Err(DomainError::ConcurrentModification(
                    "This provider's sign-in settings changed. Start signing in again.".into(),
                ));
            }

            let lock = self.commit_lock(id);
            let guard = lock.lock().await;
            self.sign_ins.check_owner(flow, Some(id))?;
            let mut latest = self.repository.get_provider(id).await?;
            if latest.provider_type != config.provider_type
                || latest.config != config.config
                || latest.token != config.token
            {
                continue;
            }
            latest.token.clone_from(&token);
            let provider = (self.provider_factory)(&latest)?;
            self.repository.store_token(id, &token).await?;
            self.reset_fetch_status(id).await;
            let lease = self.replace(id, provider, latest.refresh_interval).await;
            self.health.send(HealthInput::ProvidersChanged {
                changed: vec![self.health_input(&latest, &lease)],
            });
            self.emit_updated(id).await;
            self.sign_ins.forget(flow);
            drop(guard);
            self.queue_fetch(&lease);
            return Ok(());
        }

        Err(DomainError::ConcurrentModification(format!(
            "Provider {id} kept changing while it was reconnected"
        )))
    }
}
