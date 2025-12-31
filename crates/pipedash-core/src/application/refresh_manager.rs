use std::sync::Arc;
use std::time::{
    Duration,
    Instant,
};

use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::{
    Mutex,
    RwLock,
};
use tokio::time::interval;

use super::services::metrics_service::MetricsService;
use super::services::pipeline_service::PipelineService;
use crate::domain::Pipeline;
use crate::event::{
    CoreEvent,
    EventBus,
};

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

pub struct RefreshManager {
    pipeline_service: Arc<PipelineService>,
    metrics_service: Option<Arc<MetricsService>>,
    event_bus: Arc<dyn EventBus>,
    mode: Arc<RwLock<RefreshMode>>,
    running: Arc<RwLock<bool>>,
    last_refresh: Arc<Mutex<Option<Instant>>>,
    last_metrics_cleanup: Arc<Mutex<Option<Instant>>>,
    no_change_count: Arc<Mutex<u32>>,
    current_interval: Arc<Mutex<Duration>>,
    priority_queue: Arc<Mutex<Vec<i64>>>,
}

impl RefreshManager {
    pub fn new(
        pipeline_service: Arc<PipelineService>, metrics_service: Option<Arc<MetricsService>>,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            pipeline_service,
            metrics_service,
            event_bus,
            mode: Arc::new(RwLock::new(RefreshMode::Active)),
            running: Arc::new(RwLock::new(false)),
            last_refresh: Arc::new(Mutex::new(None)),
            last_metrics_cleanup: Arc::new(Mutex::new(None)),
            no_change_count: Arc::new(Mutex::new(0)),
            current_interval: Arc::new(Mutex::new(Duration::from_secs(10))),
            priority_queue: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn prioritize_provider(&self, provider_id: i64) {
        let mut queue = self.priority_queue.lock().await;
        if !queue.contains(&provider_id) {
            queue.push(provider_id);
            tracing::debug!(
                provider_id = provider_id,
                "Provider added to priority queue"
            );
        }
    }

    pub async fn start(&self) {
        let mut running = self.running.write().await;
        if *running {
            return; // Already running
        }
        *running = true;
        drop(running);

        let pipeline_service = Arc::clone(&self.pipeline_service);
        let metrics_service = self.metrics_service.clone();
        let event_bus = Arc::clone(&self.event_bus);
        let mode = Arc::clone(&self.mode);
        let running = Arc::clone(&self.running);
        let last_refresh = Arc::clone(&self.last_refresh);
        let last_metrics_cleanup = Arc::clone(&self.last_metrics_cleanup);
        let no_change_count = Arc::clone(&self.no_change_count);
        let current_interval = Arc::clone(&self.current_interval);
        let priority_queue = Arc::clone(&self.priority_queue);

        tokio::spawn(async move {
            let mut tick_interval = interval(Duration::from_secs(5));

            tracing::info!("RefreshManager started");

            loop {
                tick_interval.tick().await;

                let is_running = *running.read().await;
                if !is_running {
                    break;
                }

                let current_mode = *mode.read().await;

                {
                    let mut queue = priority_queue.lock().await;
                    if !queue.is_empty() {
                        let providers_to_fetch: Vec<i64> = queue.drain(..).collect();

                        tracing::debug!(
                            count = providers_to_fetch.len(),
                            "Processing priority queue"
                        );

                        for provider_id in providers_to_fetch {
                            let service = Arc::clone(&pipeline_service);
                            tokio::spawn(async move {
                                if let Err(e) = service.fetch_pipelines(Some(provider_id)).await {
                                    tracing::warn!(
                                        provider_id = provider_id,
                                        error = %e,
                                        "Priority provider fetch failed"
                                    );
                                }
                            });
                        }
                    }
                }

                if current_mode == RefreshMode::Active {
                    let refresh_interval = *current_interval.lock().await;
                    let should_refresh = {
                        let last = last_refresh.lock().await;
                        match *last {
                            Some(last_time) => last_time.elapsed() >= refresh_interval,
                            None => true,
                        }
                    };

                    if !should_refresh {
                        continue;
                    }

                    {
                        let mut last = last_refresh.lock().await;
                        *last = Some(Instant::now());
                    }

                    let old_cached = pipeline_service.get_cached_pipelines(None).await.ok();

                    match pipeline_service.fetch_pipelines(None).await {
                        Ok(pipelines) => {
                            let timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as i64;

                            event_bus
                                .emit(CoreEvent::PipelinesUpdated {
                                    pipelines: pipelines.clone(),
                                    provider_id: None,
                                    timestamp,
                                })
                                .await;

                            let has_changes = if let Some(cached) = old_cached {
                                let changes = Self::has_changes(&cached, &pipelines);
                                if changes {
                                    event_bus
                                        .emit(CoreEvent::PipelineStatusChanged {
                                            pipelines: pipelines.clone(),
                                        })
                                        .await;

                                    for new_pipeline in &pipelines {
                                        if let Some(old_pipeline) =
                                            cached.iter().find(|p| p.id == new_pipeline.id)
                                        {
                                            if old_pipeline.status != new_pipeline.status
                                                || old_pipeline.last_run != new_pipeline.last_run
                                            {
                                                tracing::debug!(
                                                    pipeline = %new_pipeline.name,
                                                    "Pipeline changed, invalidating run cache"
                                                );
                                                pipeline_service
                                                    .invalidate_run_cache(&new_pipeline.id)
                                                    .await;
                                            }
                                        }
                                    }
                                }
                                changes
                            } else {
                                false
                            };

                            let mut interval = current_interval.lock().await;
                            let mut count = no_change_count.lock().await;

                            if has_changes {
                                *interval = Duration::from_secs(10);
                                *count = 0;
                                if *interval > Duration::from_secs(10) {
                                    tracing::debug!(
                                        interval_secs = interval.as_secs(),
                                        "Changes detected, decreased interval"
                                    );
                                }
                            } else {
                                *count += 1;
                                if *count >= 3 && *interval < Duration::from_secs(300) {
                                    *interval = Duration::from_secs((*interval).as_secs() * 2)
                                        .min(Duration::from_secs(300));
                                    tracing::debug!(
                                        no_change_cycles = *count,
                                        interval_secs = interval.as_secs(),
                                        "No changes, increased interval"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            let error_msg = format!("Refresh error: {e:?}");
                            event_bus
                                .emit(CoreEvent::RefreshError { error: error_msg })
                                .await;
                        }
                    }
                }

                if let Some(ref metrics_svc) = metrics_service {
                    let should_cleanup = {
                        let last = last_metrics_cleanup.lock().await;
                        match *last {
                            Some(last_time) => last_time.elapsed() >= Duration::from_secs(6 * 3600),
                            None => true,
                        }
                    };

                    if should_cleanup {
                        {
                            let mut last = last_metrics_cleanup.lock().await;
                            *last = Some(Instant::now());
                        }

                        match metrics_svc.cleanup_old_metrics().await {
                            Ok(deleted) => {
                                tracing::info!(deleted = deleted, "Metrics cleanup complete");
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "Metrics cleanup failed");
                            }
                        }
                    }
                }
            }

            tracing::info!("RefreshManager stopped");
        });
    }

    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
    }

    pub async fn get_mode(&self) -> RefreshMode {
        *self.mode.read().await
    }

    pub async fn set_mode(&self, mode: RefreshMode) {
        let mut current = self.mode.write().await;
        *current = mode;
        tracing::debug!(mode = mode.as_str(), "Refresh mode changed");
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn get_interval(&self) -> Duration {
        *self.current_interval.lock().await
    }

    pub async fn reset_interval(&self) {
        let mut interval = self.current_interval.lock().await;
        let mut count = self.no_change_count.lock().await;
        *interval = Duration::from_secs(10);
        *count = 0;
    }

    fn has_changes(old: &[Pipeline], new: &[Pipeline]) -> bool {
        if old.len() != new.len() {
            return true;
        }

        for new_pipeline in new {
            if let Some(old_pipeline) = old.iter().find(|p| p.id == new_pipeline.id) {
                if old_pipeline.status != new_pipeline.status {
                    return true;
                }
                if old_pipeline.last_run != new_pipeline.last_run {
                    return true;
                }
            } else {
                return true;
            }
        }

        false
    }
}
