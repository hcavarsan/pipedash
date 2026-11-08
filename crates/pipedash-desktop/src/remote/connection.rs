mod persist;

use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Duration;

use pipedash_core::domain::ServerUrl;
use tauri::async_runtime::JoinHandle;
use tauri::AppHandle;

use super::model::{
    display_host,
    ConnectionInfo,
    RemoteError,
    RemoteStatus,
};
use super::session::connect::WsStream;
use super::session::{
    driver,
    Session,
};
use super::store::{
    self,
    SavedConnection,
    SavedMode,
};

const LEAVE_TIMEOUT: Duration = Duration::from_secs(3);

pub(super) struct Inner {
    pub(super) saved: SavedConnection,
    pub(super) token: Option<String>,
    pub(super) status: RemoteStatus,
    pub(super) attempts: u32,
    pub(super) generation: u64,
    pub(super) session: Option<Arc<Session>>,
    pub(super) task: Option<JoinHandle<()>>,
}

pub(super) struct Departed {
    session: Option<Arc<Session>>,
    task: Option<JoinHandle<()>>,
}

impl Departed {
    pub(super) async fn leave(self, device_id: Option<&str>) {
        if let (Some(session), Some(device_id)) = (&self.session, device_id) {
            let path = format!("/devices/{device_id}");
            if let Err(error) = session.request("DELETE", &path, None, LEAVE_TIMEOUT).await {
                tracing::debug!(%error, "Couldn't tell the old host this device left");
            }
        }
        if let Some(task) = self.task {
            task.abort();
        }
    }
}

enum Readiness {
    Ready(Arc<Session>),
    Waiting(RemoteError),
    Failed(RemoteError),
}

pub struct RemoteConnection {
    inner: Mutex<Inner>,
    pub(super) op: tokio::sync::Mutex<()>,
    changed: tokio::sync::watch::Sender<u64>,
}

impl RemoteConnection {
    pub fn new(saved: SavedConnection) -> Self {
        let client = matches!(saved.mode, SavedMode::Client { .. });
        let (token, status) = if client {
            match store::get_token() {
                Ok(Some(token)) => (Some(token), RemoteStatus::Connecting),
                Ok(None) => (None, RemoteStatus::Removed),
                Err(error) => {
                    tracing::warn!(%error, "Couldn't read the device token yet");
                    (None, RemoteStatus::Connecting)
                }
            }
        } else {
            (None, RemoteStatus::Connecting)
        };

        Self {
            inner: Mutex::new(Inner {
                saved,
                token,
                status,
                attempts: 0,
                generation: 0,
                session: None,
                task: None,
            }),
            op: tokio::sync::Mutex::new(()),
            changed: tokio::sync::watch::channel(0).0,
        }
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn is_client(&self) -> bool {
        matches!(self.lock().saved.mode, SavedMode::Client { .. })
    }

    #[cfg(desktop)]
    pub(crate) fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changed.subscribe()
    }

    #[cfg(desktop)]
    pub(crate) fn tray_link(&self) -> Option<(String, RemoteStatus, String)> {
        let inner = self.lock();
        match &inner.saved.mode {
            SavedMode::Client {
                server_url,
                device_id,
                host_name,
                ..
            } => Some((
                display_host(server_url, host_name.as_deref()),
                inner.status,
                format!("{device_id}@{server_url}"),
            )),
            _ => None,
        }
    }

    pub(super) fn saved(&self) -> SavedConnection {
        self.lock().saved.clone()
    }

    pub(super) fn info(&self) -> ConnectionInfo {
        let inner = self.lock();
        ConnectionInfo::new(&inner.saved, inner.status)
    }

    pub(super) fn can_reconnect(&self) -> bool {
        let inner = self.lock();
        matches!(inner.saved.mode, SavedMode::Client { .. })
            && inner.status != RemoteStatus::Removed
    }

    pub(super) fn alerts_position(&self, session: &Session) -> Option<(u64, Option<i64>)> {
        let inner = self.lock();
        inner
            .session
            .as_ref()
            .is_some_and(|current| std::ptr::eq(current.as_ref(), session))
            .then_some((inner.generation, inner.saved.alerts_seen))
    }

    fn readiness(&self) -> Readiness {
        let inner = self.lock();
        let SavedMode::Client {
            server_url,
            host_name,
            ..
        } = &inner.saved.mode
        else {
            return Readiness::Failed(RemoteError::NotConnected);
        };
        let unreachable =
            || RemoteError::Unreachable(display_host(server_url, host_name.as_deref()));

        match (&inner.session, inner.status) {
            (Some(session), RemoteStatus::Connected) => Readiness::Ready(Arc::clone(session)),
            (_, RemoteStatus::Removed) => Readiness::Failed(unreachable()),
            _ => Readiness::Waiting(unreachable()),
        }
    }

    pub(super) fn session(&self) -> Result<Arc<Session>, RemoteError> {
        match self.readiness() {
            Readiness::Ready(session) => Ok(session),
            Readiness::Waiting(error) | Readiness::Failed(error) => Err(error),
        }
    }

    pub(super) async fn ready_session(
        &self, timeout: Duration,
    ) -> Result<Arc<Session>, RemoteError> {
        let mut changes = self.changed.subscribe();
        let waited = tokio::time::timeout(timeout, async {
            loop {
                match self.readiness() {
                    Readiness::Ready(session) => return Ok(session),
                    Readiness::Failed(error) => return Err(error),
                    Readiness::Waiting(_) => {}
                }
                if changes.changed().await.is_err() {
                    return Err(RemoteError::NotConnected);
                }
            }
        })
        .await;

        waited.unwrap_or_else(|_| self.session())
    }

    fn notify(&self) {
        self.changed.send_modify(|version| *version += 1);
    }

    pub(super) fn launch(&self, app: &AppHandle, first: Option<WsStream>) {
        let mut inner = self.lock();
        let SavedMode::Client { server_url, .. } = &inner.saved.mode else {
            return;
        };
        let token = inner.token.clone();
        let parsed = ServerUrl::parse(server_url);
        let Ok(url) = parsed else {
            inner.status = RemoteStatus::Removed;
            self.notify();
            return;
        };

        if let Some(task) = inner.task.take() {
            task.abort();
        }
        inner.generation += 1;
        inner.status = RemoteStatus::Connecting;
        inner.attempts = 0;
        inner.session = None;
        self.notify();
        let generation = inner.generation;
        inner.task = Some(tauri::async_runtime::spawn(driver::run(
            app.clone(),
            generation,
            url,
            token,
            first,
        )));
    }

    pub(super) fn replace(
        &self, saved: SavedConnection, token: Option<String>,
    ) -> (SavedMode, Departed) {
        let mut inner = self.lock();
        inner.generation += 1;
        inner.status = RemoteStatus::Connecting;
        inner.attempts = 0;
        inner.token = token;
        self.notify();
        let previous = std::mem::replace(&mut inner.saved, saved);
        let departed = Departed {
            session: inner.session.take(),
            task: inner.task.take(),
        };

        (previous.mode, departed)
    }

    pub(super) fn attach(&self, generation: u64, session: Arc<Session>) -> bool {
        let mut inner = self.lock();
        if inner.generation != generation {
            return false;
        }
        inner.session = Some(session);
        inner.status = RemoteStatus::Connected;
        inner.attempts = 0;
        self.notify();

        true
    }

    pub(super) fn lost(&self, generation: u64) -> Option<u32> {
        let mut inner = self.lock();
        if inner.generation != generation {
            return None;
        }
        inner.session = None;
        inner.status = RemoteStatus::Reconnecting;
        inner.attempts += 1;
        self.notify();

        Some(inner.attempts)
    }

    pub(super) fn remember_token(&self, generation: u64, token: &str) {
        let mut inner = self.lock();
        if inner.generation == generation {
            inner.token = Some(token.to_owned());
        }
    }

    pub(super) fn mark_removed(&self, generation: u64) -> bool {
        let mut inner = self.lock();
        if inner.generation != generation {
            return false;
        }
        inner.session = None;
        inner.task = None;
        inner.status = RemoteStatus::Removed;
        self.notify();

        true
    }
}
