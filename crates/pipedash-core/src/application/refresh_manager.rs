use std::sync::Arc;
use std::time::Duration;

use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::{
    mpsc,
    watch,
    Mutex,
};
use tokio::task::{
    JoinHandle,
    JoinSet,
};
use tokio::time::{
    interval,
    Instant,
    MissedTickBehavior,
};
use tokio_util::sync::CancellationToken;

use super::fetch_scheduler::{
    ClaimKind,
    FetchClaim,
};
use super::services::metrics_service::MetricsService;
use super::services::pipeline_service::PipelineService;

const TICK: Duration = Duration::from_secs(5);
const METRICS_CLEANUP_INTERVAL: Duration = Duration::from_secs(6 * 3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefreshMode {
    Active,
    Idle,
}

impl RefreshMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RefreshMode::Active => "active",
            RefreshMode::Idle => "idle",
        }
    }
}

impl std::str::FromStr for RefreshMode {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "active" => RefreshMode::Active,
            _ => RefreshMode::Idle,
        })
    }
}

pub enum RefreshCommand {
    FetchNow { claims: Vec<FetchClaim> },
}

#[derive(Clone)]
pub struct RefreshDispatcher(mpsc::UnboundedSender<RefreshCommand>);

impl RefreshDispatcher {
    pub fn channel() -> (Self, mpsc::UnboundedReceiver<RefreshCommand>) {
        let (tx, rx) = mpsc::unbounded_channel();

        (Self(tx), rx)
    }

    pub fn fetch_now(&self, claims: Vec<FetchClaim>) {
        let _ = self.0.send(RefreshCommand::FetchNow { claims });
    }
}

pub struct RefreshManager {
    pipeline_service: Arc<PipelineService>,
    metrics_service: Option<Arc<MetricsService>>,
    mode: watch::Sender<RefreshMode>,
    commands: Mutex<Option<mpsc::UnboundedReceiver<RefreshCommand>>>,
    cancel: CancellationToken,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl RefreshManager {
    pub fn new(
        pipeline_service: Arc<PipelineService>, metrics_service: Option<Arc<MetricsService>>,
        mode: watch::Sender<RefreshMode>, commands: mpsc::UnboundedReceiver<RefreshCommand>,
    ) -> Self {
        Self {
            pipeline_service,
            metrics_service,
            mode,
            commands: Mutex::new(Some(commands)),
            cancel: CancellationToken::new(),
            task: Mutex::new(None),
        }
    }

    pub async fn start(&self) {
        let mut task = self.task.lock().await;
        if task.is_some() || self.cancel.is_cancelled() {
            return;
        }
        let Some(commands) = self.commands.lock().await.take() else {
            return;
        };

        *task = Some(tokio::spawn(run(
            Arc::clone(&self.pipeline_service),
            self.metrics_service.clone(),
            self.mode.subscribe(),
            self.cancel.clone(),
            commands,
        )));
    }

    pub async fn stop(&self) {
        self.cancel.cancel();
        let task = self.task.lock().await.take();
        if let Some(task) = task {
            if let Err(e) = task.await {
                tracing::error!(error = %e, "RefreshManager task failed");
            }
        }
    }

    pub fn get_mode(&self) -> RefreshMode {
        *self.mode.borrow()
    }

    pub fn set_mode(&self, mode: RefreshMode) {
        if self.mode.send_if_modified(|current| {
            let changed = *current != mode;
            *current = mode;
            changed
        }) {
            tracing::debug!(mode = mode.as_str(), "Refresh mode changed");
        }
    }
}

impl Drop for RefreshManager {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

async fn run(
    pipeline_service: Arc<PipelineService>, metrics_service: Option<Arc<MetricsService>>,
    mode: watch::Receiver<RefreshMode>, cancel: CancellationToken,
    mut commands: mpsc::UnboundedReceiver<RefreshCommand>,
) {
    let mut tick = interval(TICK);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut attempts: JoinSet<()> = JoinSet::new();
    let mut last_cleanup: Option<Instant> = None;

    tracing::info!("RefreshManager started");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tick.tick() => {
                let active = *mode.borrow() == RefreshMode::Active;
                if active {
                    for claim in pipeline_service.claim_due(Instant::now()).await {
                        spawn_fetch(&mut attempts, &pipeline_service, claim, ClaimKind::Scheduled);
                    }
                }
                if let Some(metrics_service) = &metrics_service {
                    if last_cleanup.is_none_or(|last| last.elapsed() >= METRICS_CLEANUP_INTERVAL) {
                        last_cleanup = Some(Instant::now());
                        let metrics_service = Arc::clone(metrics_service);
                        attempts.spawn(async move {
                            match metrics_service.cleanup_old_metrics().await {
                                Ok(deleted) => {
                                    tracing::info!(deleted = deleted, "Metrics cleanup complete");
                                }
                                Err(e) => {
                                    tracing::error!(error = %e, "Metrics cleanup failed");
                                }
                            }
                        });
                    }
                }
            }
            Some(command) = commands.recv() => match command {
                RefreshCommand::FetchNow { claims } => {
                    for claim in claims {
                        spawn_fetch(&mut attempts, &pipeline_service, claim, ClaimKind::Forced);
                    }
                }
            },
            Some(finished) = attempts.join_next(), if !attempts.is_empty() => {
                if let Err(e) = finished {
                    tracing::error!(error = %e, "Provider fetch task failed");
                }
            }
        }
    }

    attempts.shutdown().await;
    tracing::info!("RefreshManager stopped");
}

fn spawn_fetch(
    attempts: &mut JoinSet<()>, pipeline_service: &Arc<PipelineService>, claim: FetchClaim,
    kind: ClaimKind,
) {
    let pipeline_service = Arc::clone(pipeline_service);
    attempts.spawn(async move {
        pipeline_service.fetch_provider(claim, kind).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::test_support::{
        Gate,
        Harness,
        StubProvider,
    };
    use crate::domain::RefreshState;
    use crate::event::CoreEvent;

    #[tokio::test]
    async fn the_first_tick_fetches_each_due_provider_once() {
        let h = Harness::new().await;
        let (_, first) = h.add_stub(StubProvider::new).await;
        let (_, second) = h.add_stub(StubProvider::new).await;
        h.provider_service.load_all_providers().await.unwrap();

        h.start().await;
        h.start().await;
        h.bus
            .wait_for("both providers", |events| {
                events
                    .iter()
                    .filter(|event| matches!(event, CoreEvent::PipelinesUpdated { .. }))
                    .count()
                    == 2
            })
            .await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!((first.fetch_count(), second.fetch_count()), (1, 1));
        assert_eq!(h.bus.count("pipelines-updated"), 2);
    }

    #[tokio::test]
    async fn providers_missing_from_the_startup_load_are_fetched_on_schedule() {
        let h = Harness::new().await;
        let (_, stub) = h.add_stub(StubProvider::new).await;

        h.start().await;
        h.bus
            .wait_for("the unloaded provider", |events| {
                events
                    .iter()
                    .any(|event| matches!(event, CoreEvent::PipelinesUpdated { .. }))
            })
            .await;

        assert_eq!(stub.fetch_count(), 1);
    }

    #[tokio::test]
    async fn idle_mode_keeps_scheduled_fetches_from_starting() {
        let h = Harness::new().await;
        let (_, stub) = h.add_stub(StubProvider::new).await;
        h.provider_service.load_all_providers().await.unwrap();
        h.refresh_manager.set_mode(RefreshMode::Idle);
        assert_eq!(h.refresh_manager.get_mode(), RefreshMode::Idle);

        h.start().await;
        tokio::time::sleep(Duration::from_millis(300)).await;

        assert_eq!(stub.fetch_count(), 0);
        h.refresh_manager.set_mode(RefreshMode::Active);
        assert_eq!(h.refresh_manager.get_mode(), RefreshMode::Active);
    }

    #[tokio::test]
    async fn stopping_cancels_running_attempts_and_their_waiters() {
        let h = Harness::new().await;
        h.start().await;
        let gate = Gate::new();
        let (id, stub) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
        let waiting = tokio::spawn({
            let service = Arc::clone(&h.pipeline_service);
            async move { service.fetch_now(&[id], Duration::from_secs(10)).await }
        });
        gate.arrived().await;

        h.refresh_manager.stop().await;
        let results = waiting.await.unwrap();
        h.refresh_manager.start().await;

        assert_eq!(results[0].state, RefreshState::Pending);
        let view = h.scheduler.view(id, Instant::now()).unwrap();
        assert_eq!((view.failures, view.in_flight), (0, false));
        assert_eq!(stub.fetch_count(), 1);
        assert_eq!(h.bus.count("pipelines-updated"), 0);
        assert!(h.refresh_manager.task.lock().await.is_none());
    }

    #[tokio::test]
    async fn a_stopped_manager_hands_new_claims_back_as_cancelled() {
        let h = Harness::new().await;
        h.start().await;
        h.refresh_manager.stop().await;
        let (id, _) = h.add_stub(StubProvider::new).await;

        let results = h
            .pipeline_service
            .fetch_now(&[id], Duration::from_millis(300))
            .await;

        assert_eq!(results[0].state, RefreshState::Pending);
    }
}
