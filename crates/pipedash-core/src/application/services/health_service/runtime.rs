use std::collections::HashMap;
use std::future::pending;
use std::sync::{
    Arc,
    Mutex,
};

use tokio::sync::{
    mpsc,
    watch,
};
use tokio::task::JoinSet;
use tokio::time::{
    sleep_until,
    Instant,
};
use tokio_util::sync::CancellationToken;

use super::alerts::AlertTracker;
use super::{
    Actor,
    HealthInput,
    HealthReceivers,
    Parts,
    ProviderHealthService,
};
use crate::application::lifecycle::{
    start_once,
    stop_task,
};
use crate::application::refresh_manager::RefreshMode;
use crate::application::services::provider_service::ProviderService;
use crate::event::EventBus;
use crate::infrastructure::{
    ConfigKey,
    ConfigState,
    HttpClientManager,
};

async fn wake_at(at: Option<Instant>) {
    match at {
        Some(at) => sleep_until(at).await,
        None => pending().await,
    }
}

async fn run(
    mut actor: Actor, mut inputs: mpsc::UnboundedReceiver<HealthInput>,
    mut mode: watch::Receiver<RefreshMode>, cancel: CancellationToken,
) {
    actor.enabled = actor.config.get_bool(ConfigKey::StatusPagesEnabled).await;
    actor.active = *mode.borrow_and_update() != RefreshMode::Idle;
    let mut mode_open = true;
    tracing::info!(enabled = actor.enabled, "ProviderHealthService started");

    loop {
        let wake = actor.next_wake(Instant::now());
        tokio::select! {
            _ = cancel.cancelled() => break,
            input = inputs.recv() => match input {
                Some(input) => actor.handle(input).await,
                None => break,
            },
            changed = mode.changed(), if mode_open => match changed {
                Ok(()) => {
                    let active = *mode.borrow_and_update() != RefreshMode::Idle;
                    actor.set_active(active).await;
                }
                Err(_) => mode_open = false,
            },
            Some(joined) = actor.tasks.join_next(), if !actor.tasks.is_empty() => {
                actor.finish(joined).await;
            }
            _ = wake_at(wake) => actor.tick().await,
        }
    }

    actor.tasks.shutdown().await;
    tracing::info!("ProviderHealthService stopped");
}

impl ProviderHealthService {
    pub fn new(
        receivers: HealthReceivers, provider_service: Arc<ProviderService>,
        http: Arc<HttpClientManager>, event_bus: Arc<dyn EventBus>, config: Arc<ConfigState>,
        mode: watch::Receiver<RefreshMode>,
    ) -> Self {
        let actor = Actor {
            provider_service,
            http,
            event_bus,
            config,
            snapshot: receivers.snapshot,
            enabled: true,
            active: true,
            feeds: HashMap::new(),
            providers: HashMap::new(),
            alerts: AlertTracker::default(),
            published: HashMap::new(),
            outbox: Vec::new(),
            recoveries: Vec::new(),
            tasks: JoinSet::new(),
            next_token: 0,
        };

        Self {
            parts: Mutex::new(Some(Parts {
                actor,
                inputs: receivers.inputs,
                mode,
            })),
            cancel: CancellationToken::new(),
            task: Mutex::new(None),
        }
    }

    pub async fn start(&self) {
        start_once(&self.cancel, &self.parts, &self.task, |parts| {
            tokio::spawn(run(
                parts.actor,
                parts.inputs,
                parts.mode,
                self.cancel.clone(),
            ))
        });
    }

    pub async fn stop(&self) {
        stop_task(&self.cancel, &self.task, "ProviderHealthService").await;
    }
}

impl Drop for ProviderHealthService {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
