use std::future::Future;
use std::sync::{
    Mutex as StdMutex,
    PoisonError,
};
use std::time::Duration;

use tokio::sync::oneshot;
use tokio::task::JoinSet;

use super::ProviderService;
use crate::domain::{
    DomainError,
    DomainResult,
};

const RELOAD_DRAIN_WARNING: Duration = Duration::from_secs(120);

#[derive(Default)]
pub(super) struct Reloads {
    closed: bool,
    set: JoinSet<()>,
}

struct Draining<'a> {
    reloads: &'a StdMutex<Reloads>,
    set: JoinSet<()>,
}

impl Drop for Draining<'_> {
    fn drop(&mut self) {
        self.reloads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .set = std::mem::take(&mut self.set);
    }
}

impl ProviderService {
    pub(super) fn own_reload<T: Send + 'static>(
        &self, work: impl Future<Output = T> + Send + 'static,
    ) -> DomainResult<oneshot::Receiver<T>> {
        let (done, result) = oneshot::channel();
        let mut reloads = self.reloads.lock().unwrap_or_else(PoisonError::into_inner);
        if reloads.closed {
            return Err(DomainError::ServiceUnavailable {
                message: "The app is shutting down, so the config file was not applied".into(),
                retry_after: None,
            });
        }
        while reloads.set.try_join_next().is_some() {}
        reloads.set.spawn(async move {
            let _ = done.send(work.await);
        });

        Ok(result)
    }

    pub async fn drain_reloads(&self) {
        let set = {
            let mut reloads = self.reloads.lock().unwrap_or_else(PoisonError::into_inner);
            reloads.closed = true;
            std::mem::take(&mut reloads.set)
        };
        let mut draining = Draining {
            reloads: &self.reloads,
            set,
        };
        loop {
            match tokio::time::timeout(RELOAD_DRAIN_WARNING, draining.set.join_next()).await {
                Ok(None) => return,
                Ok(Some(_)) => {}
                Err(_) => tracing::warn!(
                    waited_secs = RELOAD_DRAIN_WARNING.as_secs(),
                    "Shutdown is still waiting for a config reload to commit or restore"
                ),
            }
        }
    }
}
