use std::path::Path;
use std::sync::Arc;
use std::time::{
    Duration,
    SystemTime,
};

use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{
    interval,
    MissedTickBehavior,
};
use tokio_util::sync::CancellationToken;

use super::ProviderService;
use crate::infrastructure::ConfigState;

const POLL: Duration = Duration::from_secs(5);

type FileStamp = (SystemTime, u64);

pub struct ConfigWatcher {
    provider_service: Arc<ProviderService>,
    config_state: Arc<ConfigState>,
    poll: Duration,
    cancel: CancellationToken,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl ConfigWatcher {
    pub fn new(provider_service: Arc<ProviderService>, config_state: Arc<ConfigState>) -> Self {
        Self {
            provider_service,
            config_state,
            poll: POLL,
            cancel: CancellationToken::new(),
            task: Mutex::new(None),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_poll(mut self, poll: Duration) -> Self {
        self.poll = poll;
        self
    }

    pub async fn start(&self) {
        let mut task = self.task.lock().await;
        if task.is_some() || self.cancel.is_cancelled() {
            return;
        }

        *task = Some(tokio::spawn(watch(
            Arc::clone(&self.provider_service),
            Arc::clone(&self.config_state),
            self.poll,
            self.cancel.clone(),
        )));
    }

    pub async fn stop(&self) {
        self.cancel.cancel();
        let task = self.task.lock().await.take();
        if let Some(task) = task {
            if let Err(e) = task.await {
                tracing::error!(error = %e, "ConfigWatcher task failed");
            }
        }
    }
}

impl Drop for ConfigWatcher {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn stamp(path: &Path) -> Option<FileStamp> {
    let metadata = std::fs::metadata(path).ok()?;

    Some((metadata.modified().ok()?, metadata.len()))
}

async fn watch(
    provider_service: Arc<ProviderService>, config_state: Arc<ConfigState>, poll: Duration,
    cancel: CancellationToken,
) {
    let path = config_state.config_path().clone();
    let mut applied = None;
    let mut failed = None;
    let mut tick = interval(poll);
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = tick.tick() => {
                let current = stamp(&path);
                if current.is_none() || current == applied {
                    continue;
                }

                match provider_service.apply_config_file(&config_state).await {
                    Ok(_) => applied = current,
                    Err(e) if current != failed => {
                        failed = current;
                        tracing::warn!(path = %path.display(), error = %e, "Failed to apply config file changes");
                    }
                    Err(e) => {
                        tracing::debug!(path = %path.display(), error = %e, "Failed to apply config file changes");
                    }
                }
            }
        }
    }
}
