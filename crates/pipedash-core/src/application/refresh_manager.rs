use std::collections::VecDeque;
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::{
    mpsc,
    watch,
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
use super::lifecycle::{
    start_once,
    stop_task,
};
use super::services::metrics_service::MetricsService;
use super::services::pipeline_service::{
    PipelineService,
    FETCH_CONCURRENCY,
};

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
        start_once(&self.cancel, &self.commands, &self.task, |commands| {
            tokio::spawn(run(
                Arc::clone(&self.pipeline_service),
                self.metrics_service.clone(),
                self.mode.subscribe(),
                self.cancel.clone(),
                commands,
            ))
        });
    }

    pub async fn stop(&self) {
        stop_task(&self.cancel, &self.task, "RefreshManager").await;
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
    let mut fetches: JoinSet<()> = JoinSet::new();
    let mut cleanups: JoinSet<()> = JoinSet::new();
    let mut queue: VecDeque<(FetchClaim, ClaimKind)> = VecDeque::new();
    let mut last_cleanup: Option<Instant> = None;

    tracing::info!("RefreshManager started");

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tick.tick() => {
                if *mode.borrow() == RefreshMode::Active {
                    let Some(due) = claim_due(&pipeline_service, &cancel).await else {
                        break;
                    };
                    queue.extend(due.into_iter().map(|claim| (claim, ClaimKind::Scheduled)));
                }
                if let Some(metrics_service) = &metrics_service {
                    if last_cleanup.is_none_or(|last| last.elapsed() >= METRICS_CLEANUP_INTERVAL) {
                        last_cleanup = Some(Instant::now());
                        cleanups.spawn(cleanup_metrics(Arc::clone(metrics_service)));
                    }
                }
            }
            Some(command) = commands.recv() => match command {
                RefreshCommand::FetchNow { claims } => {
                    for claim in claims.into_iter().rev() {
                        queue.push_front((claim, ClaimKind::Forced));
                    }
                }
            },
            Some(finished) = fetches.join_next(), if !fetches.is_empty() => {
                if let Err(e) = finished {
                    tracing::error!(error = %e, "Provider fetch task failed");
                }
            }
            Some(finished) = cleanups.join_next(), if !cleanups.is_empty() => {
                if let Err(e) = finished {
                    tracing::error!(error = %e, "Metrics cleanup task failed");
                }
            }
        }
        while fetches.len() < FETCH_CONCURRENCY {
            let Some((claim, kind)) = queue.pop_front() else {
                break;
            };
            spawn_fetch(&mut fetches, &pipeline_service, claim, kind);
        }
    }

    drop(queue);
    fetches.shutdown().await;
    cleanups.shutdown().await;
    tracing::info!("RefreshManager stopped");
}

async fn claim_due(
    pipeline_service: &PipelineService, cancel: &CancellationToken,
) -> Option<Vec<FetchClaim>> {
    tokio::select! {
        _ = cancel.cancelled() => None,
        due = pipeline_service.claim_due(Instant::now()) => Some(due),
    }
}

async fn cleanup_metrics(metrics_service: Arc<MetricsService>) {
    match metrics_service.cleanup_old_metrics().await {
        Ok(deleted) => {
            tracing::info!(deleted = deleted, "Metrics cleanup complete");
        }
        Err(e) => {
            tracing::error!(error = %e, "Metrics cleanup failed");
        }
    }
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
mod tests;
