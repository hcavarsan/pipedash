use std::{
    future::Future,
    sync::{
        Arc,
        Mutex,
        MutexGuard,
        PoisonError,
    },
};

use pipedash_plugin_api::build_watch::BuildWatchStatus;
use tauri::async_runtime::JoinHandle;
use tokio::sync::watch;

use super::{
    error,
    native::is_watching,
};
use crate::error::ErrorResponse;

#[derive(Default)]
pub struct BuildWatchState {
    session: Arc<Mutex<Option<Session>>>,
    interruptions: Mutex<Interruptions>,
}

struct Session {
    id: String,
    provider_id: Option<i64>,
    #[cfg(desktop)]
    generation: Option<u64>,
    cancel: watch::Sender<bool>,
}

#[derive(PartialEq)]
pub(super) enum StopTarget {
    Session(String),
    Orphan,
}

#[derive(Default)]
struct Interruptions {
    pending: Vec<StopTarget>,
    worker: Option<JoinHandle<()>>,
}

pub(super) struct SessionGuard {
    state: Arc<Mutex<Option<Session>>>,
    pub(super) id: String,
    cancelled: watch::Receiver<bool>,
}

fn cancel_session(state: &Mutex<Option<Session>>, matching: Option<&str>) -> Option<String> {
    let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
    if matching.is_some_and(|id| state.as_ref().is_none_or(|session| session.id != id)) {
        return None;
    }
    let session = state.take()?;
    drop(state);
    let _ = session.cancel.send(true);
    Some(session.id)
}

impl BuildWatchState {
    fn inner(&self) -> MutexGuard<'_, Option<Session>> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn interruptions(&self) -> MutexGuard<'_, Interruptions> {
        self.interruptions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn reserve(&self) -> Result<SessionGuard, ErrorResponse> {
        let mut state = self.inner();
        if state.is_some() {
            return Err(error(
                "Stop the current build watch before starting another",
            ));
        }
        let id = format!("{:032x}", rand::random::<u128>());
        let (cancel, cancelled) = watch::channel(false);
        *state = Some(Session {
            id: id.clone(),
            provider_id: None,
            #[cfg(desktop)]
            generation: None,
            cancel,
        });
        Ok(SessionGuard {
            state: Arc::clone(&self.session),
            id,
            cancelled,
        })
    }

    pub(super) fn session_id(&self) -> Option<String> {
        self.inner().as_ref().map(|session| session.id.clone())
    }

    #[cfg(any(mobile, test))]
    pub(super) fn provider_session_id(&self, provider_id: i64) -> Option<String> {
        self.inner()
            .as_ref()
            .filter(|session| session.provider_id == Some(provider_id))
            .map(|session| session.id.clone())
    }

    #[cfg(desktop)]
    pub(super) fn provider_session_id_generation(
        &self, generation: u64, provider_id: i64,
    ) -> Option<String> {
        self.inner()
            .as_ref()
            .filter(|session| {
                session.generation == Some(generation) && session.provider_id == Some(provider_id)
            })
            .map(|session| session.id.clone())
    }

    pub(super) fn cancel(&self, matching: Option<&str>) -> Option<String> {
        cancel_session(&self.session, matching)
    }

    pub(super) fn native_changed(&self, status: &BuildWatchStatus) {
        if let Some(watch) = &status.watch {
            if !is_watching(status, &watch.session_id) {
                self.cancel(Some(&watch.session_id));
            }
        }
    }

    pub(super) fn queue_interruption(
        &self, target: StopTarget, start: impl FnOnce() -> JoinHandle<()>,
    ) {
        let mut queue = self.interruptions();
        if !queue.pending.contains(&target) {
            queue.pending.push(target);
        }
        if queue.worker.is_none() {
            queue.worker = Some(start());
        }
    }

    pub(super) fn next_interruption(&self) -> Option<StopTarget> {
        let mut queue = self.interruptions();
        if queue.pending.is_empty() {
            queue.worker = None;
            return None;
        }
        Some(queue.pending.remove(0))
    }
}

impl SessionGuard {
    #[cfg(desktop)]
    pub(super) fn bind_generation(&self, generation: u64) -> Result<(), ErrorResponse> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let session = state
            .as_mut()
            .filter(|session| session.id == self.id)
            .ok_or_else(|| error("Build watching was interrupted"))?;
        session.generation = Some(generation);
        Ok(())
    }

    pub(super) fn bind_provider(&self, provider_id: i64) -> Result<(), ErrorResponse> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let session = state
            .as_mut()
            .filter(|session| session.id == self.id)
            .ok_or_else(|| error("Build watching was interrupted"))?;
        if session
            .provider_id
            .is_some_and(|current| current != provider_id)
        {
            return Err(error("The watched provider changed"));
        }
        session.provider_id = Some(provider_id);
        Ok(())
    }

    pub(super) fn is_current(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|session| session.id == self.id)
    }

    pub(super) async fn run<T>(
        &self, future: impl Future<Output = Result<T, ErrorResponse>>,
    ) -> Result<T, ErrorResponse> {
        if !self.is_current() {
            return Err(error("Build watching was interrupted"));
        }
        let mut cancelled = self.cancelled.clone();
        tokio::select! {
            biased;
            _ = cancelled.changed() => Err(error("Build watching was interrupted")),
            result = future => result,
        }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        cancel_session(&self.state, Some(&self.id));
    }
}
