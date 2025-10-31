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

use crate::application::services::PipelineService;
use crate::domain::Pipeline;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RefreshMode {
    Active,
    Idle,
}

pub struct RefreshManager {
    pipeline_service: Arc<PipelineService>,
    mode: Arc<RwLock<RefreshMode>>,
    running: Arc<RwLock<bool>>,
    last_refresh: Arc<Mutex<Option<Instant>>>,
}

impl RefreshManager {
    pub fn new(pipeline_service: Arc<PipelineService>) -> Self {
        Self {
            pipeline_service,
            mode: Arc::new(RwLock::new(RefreshMode::Idle)),
            running: Arc::new(RwLock::new(false)),
            last_refresh: Arc::new(Mutex::new(None)),
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
        let mode = Arc::clone(&self.mode);
        let running = Arc::clone(&self.running);
        let last_refresh = Arc::clone(&self.last_refresh);

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
                    let should_refresh = {
                        let last = last_refresh.lock().await;
                        match *last {
                            Some(last_time) => last_time.elapsed() >= Duration::from_secs(3),
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
                            let _ = app_handle.emit("pipelines-updated", &pipelines);

                            if let Some(cached) = old_cached {
                                if Self::has_status_changes(&cached, &pipelines) {
                                    let _ = app_handle.emit("pipeline-status-changed", &pipelines);
                                }
                            }
                        }
                        Err(e) => {
                            let error_msg = format!("Refresh error: {e:?}");
                            let _ = app_handle.emit("refresh-error", error_msg);
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

    fn has_status_changes(old: &[Pipeline], new: &[Pipeline]) -> bool {
        if old.len() != new.len() {
            return true;
        }

        for new_pipeline in new {
            if let Some(old_pipeline) = old.iter().find(|p| p.id == new_pipeline.id) {
                if old_pipeline.status != new_pipeline.status {
                    return true;
                }
            } else {
                return true;
            }
        }

        false
    }
}
