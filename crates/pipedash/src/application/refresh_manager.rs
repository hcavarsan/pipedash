use std::sync::Arc;
use std::time::{
    Duration,
    Instant,
};

use serde::{
    Deserialize,
    Serialize,
};
use tauri::{
    AppHandle,
    Emitter,
};
use tokio::sync::{
    Mutex,
    RwLock,
};
use tokio::time::interval;

use crate::application::services::{
    MetricsService,
    PipelineService,
};
use crate::domain::Pipeline;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RefreshMode {
    Active,
    Idle,
}

pub struct RefreshManager {
    pipeline_service: Arc<PipelineService>,
    metrics_service: Option<Arc<MetricsService>>,
    mode: Arc<RwLock<RefreshMode>>,
    running: Arc<RwLock<bool>>,
    last_refresh: Arc<Mutex<Option<Instant>>>,
    last_metrics_cleanup: Arc<Mutex<Option<Instant>>>,
    no_change_count: Arc<Mutex<u32>>,
    current_interval: Arc<Mutex<Duration>>,
}

impl RefreshManager {
    pub fn new(
        pipeline_service: Arc<PipelineService>, metrics_service: Option<Arc<MetricsService>>,
    ) -> Self {
        Self {
            pipeline_service,
            metrics_service,
            mode: Arc::new(RwLock::new(RefreshMode::Idle)),
            running: Arc::new(RwLock::new(false)),
            last_refresh: Arc::new(Mutex::new(None)),
            last_metrics_cleanup: Arc::new(Mutex::new(None)),
            no_change_count: Arc::new(Mutex::new(0)),
            current_interval: Arc::new(Mutex::new(Duration::from_secs(10))),
        }
    }

    pub async fn set_mode(&self, mode: RefreshMode) {
        let mut current_mode = self.mode.write().await;
        *current_mode = mode;
    }

    pub async fn get_mode(&self) -> RefreshMode {
        *self.mode.read().await
    }

    pub async fn start(&self, app_handle: AppHandle) {
        let mut running = self.running.write().await;
        if *running {
            return;
        }
        *running = true;
        drop(running);

        let pipeline_service = Arc::clone(&self.pipeline_service);
        let metrics_service = self.metrics_service.clone();
        let mode = Arc::clone(&self.mode);
        let running = Arc::clone(&self.running);
        let last_refresh = Arc::clone(&self.last_refresh);
        let last_metrics_cleanup = Arc::clone(&self.last_metrics_cleanup);

        let no_change_count = Arc::clone(&self.no_change_count);
        let current_interval = Arc::clone(&self.current_interval);

        tokio::spawn(async move {
            let mut tick_interval = interval(Duration::from_secs(5));

            loop {
                tick_interval.tick().await;

                let is_running = *running.read().await;
                if !is_running {
                    break;
                }

                let current_mode = *mode.read().await;

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

                    match pipeline_service
                        .fetch_pipelines(None, Some(app_handle.clone()))
                        .await
                    {
                        Ok(pipelines) => {
                            let _ = app_handle.emit("pipelines-updated", &pipelines);

                            let has_changes = if let Some(cached) = old_cached {
                                let changes = Self::has_changes(&cached, &pipelines);
                                if changes {
                                    let _ = app_handle.emit("pipeline-status-changed", &pipelines);

                                    // Auto-invalidate run cache for changed pipelines
                                    for new_pipeline in &pipelines {
                                        if let Some(old_pipeline) =
                                            cached.iter().find(|p| p.id == new_pipeline.id)
                                        {
                                            // Check if this specific pipeline changed
                                            if old_pipeline.status != new_pipeline.status
                                                || old_pipeline.last_run != new_pipeline.last_run
                                            {
                                                eprintln!("[REFRESH] Pipeline '{}' changed, invalidating run cache", new_pipeline.name);
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
                                *count = 0;
                                if *interval > Duration::from_secs(10) {
                                    *interval = (*interval * 2) / 3;
                                    eprintln!(
                                        "[REFRESH] Changes detected, decreased interval to {:?}",
                                        *interval
                                    );
                                }
                            } else {
                                *count += 1;
                                if *count >= 3 && *interval < Duration::from_secs(300) {
                                    *interval = (*interval * 3) / 2;
                                    *interval = (*interval).min(Duration::from_secs(300));
                                    eprintln!("[REFRESH] No changes for {} cycles, increased interval to {:?}", *count, *interval);
                                }
                            }
                        }
                        Err(e) => {
                            let error_msg = format!("Refresh error: {e:?}");
                            let _ = app_handle.emit("refresh-error", error_msg);
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
                                eprintln!(
                                    "[METRICS] Cleanup complete: deleted {} old metrics",
                                    deleted
                                );
                            }
                            Err(e) => {
                                eprintln!("[METRICS] Cleanup failed: {:?}", e);
                            }
                        }
                    }
                }
            }
        });
    }

    #[allow(dead_code)]
    pub async fn stop(&self) {
        let mut running = self.running.write().await;
        *running = false;
    }

    fn has_changes(old: &[Pipeline], new: &[Pipeline]) -> bool {
        if old.len() != new.len() {
            return true;
        }

        for new_pipeline in new {
            if let Some(old_pipeline) = old.iter().find(|p| p.id == new_pipeline.id) {
                // Check for status changes
                if old_pipeline.status != new_pipeline.status {
                    return true;
                }
                // Check for new runs
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
