use std::{
    path::PathBuf,
    sync::{
        atomic::AtomicU64,
        Arc,
        Mutex,
        MutexGuard,
        PoisonError,
    },
};

use pipedash_core::{
    domain::{
        notification_center::{
            CenterNotification,
            CenterTone,
            NotificationCenterSnapshot,
        },
        DomainResult,
        NotificationPreferences,
        PipelineMenuSnapshot,
    },
    CoreContext,
};
use pipedash_plugin_api::build_watch::BuildWatchSnapshot;
use serde::Serialize;
use tauri::{
    AppHandle,
    Emitter,
    Manager,
    Runtime,
};

use super::{
    settings::{
        DesktopSettings,
        SavedSettings,
        SettingsRetry,
    },
    tray::{
        self,
        ClientLink,
        TrayStatus,
    },
};
use crate::{
    error::ErrorResponse,
    local_notifications::LocalNotificationsState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Ticket {
    pub(super) generation: u64,
    pub(super) epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Core(u64),
    Server,
}

#[derive(Clone, Serialize)]
pub struct DesktopStatus {
    revision: u64,
    settings: DesktopSettings,
    tray_state: &'static str,
    tray_status: TrayStatus,
    unread: usize,
    ready: bool,
    notification_preference_imported: bool,
    error: Option<String>,
}

pub(super) struct Tasks {
    pub(super) stop: tokio::sync::watch::Sender<()>,
    pub(super) handles: Vec<tauri::async_runtime::JoinHandle<()>>,
}

impl Tasks {
    pub(super) fn stop(self) {
        let _ = self.stop.send(());
        for handle in self.handles {
            handle.abort();
        }
    }
}

#[derive(Default)]
pub(super) struct Inner {
    pub(super) saved: SavedSettings,
    pub(super) revision: u64,
    pub(super) generation: u64,
    pub(super) epoch: u64,
    pub(super) vault_ready: bool,
    pub(super) core: Option<Arc<CoreContext>>,
    pub(super) snapshot: Option<PipelineMenuSnapshot>,
    pub(super) settings_error: Option<String>,
    pub(super) data_error: Option<String>,
    pub(super) tray_error: Option<String>,
    pub(super) tray_ready: bool,
    pub(super) refresh_active: bool,
    pub(super) refresh_message: Option<String>,
    pub(super) watch_session: Option<(String, Ticket)>,
    pub(super) watch: Option<BuildWatchSnapshot>,
    pub(super) history: NotificationCenterSnapshot,
    pub(super) client: Option<ClientLink>,
    pub(super) server: Option<Option<String>>,
    pub(super) retry: SettingsRetry,
    pub(super) tasks: Option<Tasks>,
}

impl Inner {
    pub(super) fn ticket(&self) -> Ticket {
        Ticket {
            generation: self.generation,
            epoch: self.epoch,
        }
    }
    pub(super) fn matches(&self, ticket: Ticket) -> bool {
        self.vault_ready && self.core.is_some() && self.ticket() == ticket
    }
    pub(super) fn current(&self, ticket: Ticket) -> bool {
        self.active() && self.ticket() == ticket
    }
    pub(super) fn active(&self) -> bool {
        match &self.client {
            Some(link) => link.connected(),
            None => self.vault_ready,
        }
    }
    pub(super) fn unread(&self) -> usize {
        self.history.unread
    }
    pub(super) fn unread_danger(&self) -> bool {
        self.history
            .items
            .iter()
            .any(|notice| !notice.read && notice.tone == CenterTone::Danger)
    }
    pub(super) fn accepts(&self, origin: Origin) -> bool {
        match origin {
            Origin::Core(generation) => {
                generation == self.generation && self.vault_ready && self.core.is_some()
            }
            Origin::Server => self.core.is_none(),
        }
    }
    pub(super) fn record(
        &mut self, notice: &CenterNotification,
    ) -> (NotificationCenterSnapshot, bool) {
        self.history.push(notice.clone());
        (
            self.history.clone(),
            self.saved.settings.notifications_enabled,
        )
    }
    pub(super) fn follow(&mut self, link: Option<ClientLink>, server: Option<String>) -> bool {
        self.client = link;
        self.adopt_server(server)
    }
    pub(super) fn adopt_server(&mut self, server: Option<String>) -> bool {
        let moved = self.server.as_ref().is_some_and(|seen| *seen != server);
        self.server = Some(server);
        if moved {
            self.epoch += 1;
            self.history.clear();
            self.snapshot = None;
            self.data_error = None;
        }
        moved
    }
    pub(super) fn apply_menu_result(&mut self, result: DomainResult<PipelineMenuSnapshot>) {
        match result {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                self.data_error = None;
            }
            Err(_) => {
                self.snapshot = None;
                self.data_error =
                    Some("Couldn't load pinned pipelines. Retry in General settings.".to_string());
            }
        }
    }
    pub(super) fn status(&self, prefs: &NotificationPreferences) -> DesktopStatus {
        DesktopStatus {
            revision: self.revision,
            settings: self.saved.settings,
            tray_state: if self.tray_error.is_some() {
                "error"
            } else if self.tray_ready {
                "ready"
            } else {
                "off"
            },
            tray_status: tray::tray_status(self, prefs),
            unread: self.unread(),
            ready: self.vault_ready,
            notification_preference_imported: self.saved.notification_preference_imported,
            error: self
                .settings_error
                .as_ref()
                .or(self.tray_error.as_ref())
                .or(self.data_error.as_ref())
                .or(self.retry.error.as_ref())
                .cloned(),
        }
    }
}

pub struct DesktopState {
    pub(super) inner: Mutex<Inner>,
    pub(super) path: PathBuf,
    pub(super) settings_gate: tokio::sync::Mutex<()>,
    pub(super) projection_gate: tokio::sync::Mutex<()>,
    pub(super) refresh: tokio::sync::Notify,
    pub(super) generations: AtomicU64,
    pub(super) render: Box<dyn Fn() + Send + Sync>,
}

impl DesktopState {
    pub(super) fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(super) fn failure(message: &str) -> ErrorResponse {
    ErrorResponse {
        error: message.to_string(),
        details: None,
        code: None,
    }
}

pub(super) fn publish_status<R: Runtime>(app: &AppHandle<R>) {
    let prefs = preferences(app);
    let status = {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        inner.revision += 1;
        inner.status(&prefs)
    };
    if let Err(error) = app.emit("desktop-status-changed", status) {
        tracing::warn!(%error, "Couldn't publish desktop integration status");
    }
}

pub(super) fn preferences<R: Runtime>(app: &AppHandle<R>) -> NotificationPreferences {
    app.try_state::<LocalNotificationsState>()
        .map(|state| state.preferences_copy())
        .unwrap_or_default()
}

pub(super) fn current_status<R: Runtime>(app: &AppHandle<R>) -> DesktopStatus {
    let prefs = preferences(app);
    app.state::<DesktopState>().lock().status(&prefs)
}
