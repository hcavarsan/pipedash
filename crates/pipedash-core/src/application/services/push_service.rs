mod alerts;
mod devices;
pub(super) mod subscriptions;

use std::future::Future;
use std::sync::Arc;

use futures::stream::{
    self,
    StreamExt,
};
use serde::Serialize;

use crate::application::{
    PipelineService,
    ProviderService,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::database::{
    DeviceAlertRepository,
    DevicePushRepository,
    DeviceRepository,
    PushRepository,
};
use crate::infrastructure::push_relay::normalize_relay_url;
use crate::infrastructure::{
    ConfigKey,
    ConfigState,
    HttpClientManager,
    OFFICIAL_PUSH_RELAY_URL,
};
use crate::CoreContext;

const FAN_OUT: usize = 16;

enum RelaySource {
    Config,
    Fixed(Option<String>),
}

pub struct PushService {
    subscriptions: PushRepository,
    device_pushes: DevicePushRepository,
    device_alerts: DeviceAlertRepository,
    devices: DeviceRepository,
    config: Arc<ConfigState>,
    http: Arc<HttpClientManager>,
    providers: Arc<ProviderService>,
    pipelines: Arc<PipelineService>,
    relay: RelaySource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelaySettings {
    pub enabled: bool,
    pub relay_url: Option<String>,
    pub from_env: bool,
    pub official_url: &'static str,
}

impl CoreContext {
    pub fn push_service(&self) -> PushService {
        PushService {
            subscriptions: PushRepository::new(
                self.provider_service.repository().cache_pool().clone(),
            ),
            device_pushes: self.device_pushes(),
            device_alerts: self.device_alerts(),
            devices: self.devices(),
            config: Arc::clone(&self.config_state),
            http: Arc::clone(&self.http_client_manager),
            providers: Arc::clone(&self.provider_service),
            pipelines: Arc::clone(&self.pipeline_service),
            relay: RelaySource::Config,
        }
    }
}

pub(super) async fn fan_out<F: Future>(jobs: Vec<F>) {
    stream::iter(jobs)
        .buffer_unordered(FAN_OUT)
        .for_each(|_| std::future::ready(()))
        .await;
}

impl PushService {
    pub fn with_fixed_relay(mut self, relay_url: Option<&str>) -> Self {
        self.relay = RelaySource::Fixed(relay_url.and_then(normalize_relay_url));

        self
    }

    async fn configured_relay_url(&self) -> Option<String> {
        normalize_relay_url(&self.config.get_string(ConfigKey::PushRelayUrl).await)
    }

    pub async fn relay_url(&self) -> Option<String> {
        match &self.relay {
            RelaySource::Fixed(relay_url) => relay_url.clone(),
            RelaySource::Config => self.configured_relay_url().await,
        }
    }

    pub async fn relay_settings(&self) -> DomainResult<RelaySettings> {
        let relay_url = self.configured_relay_url().await;

        Ok(RelaySettings {
            enabled: relay_url.is_some(),
            relay_url,
            from_env: ConfigState::push_relay_from_env(),
            official_url: OFFICIAL_PUSH_RELAY_URL,
        })
    }

    pub async fn set_relay_enabled(&self, enabled: bool) -> DomainResult<RelaySettings> {
        if ConfigState::push_relay_from_env() {
            return Err(DomainError::Conflict {
                code: "PUSH_RELAY_FROM_ENV",
                message: "PIPEDASH_PUSH_RELAY_URL sets the relay. Change it in the server's \
                          environment."
                    .into(),
            });
        }
        let relay_url = if enabled { OFFICIAL_PUSH_RELAY_URL } else { "" };
        self.config.set_push_relay_url(relay_url).await?;

        self.relay_settings().await
    }
}

#[cfg(test)]
mod tests;
