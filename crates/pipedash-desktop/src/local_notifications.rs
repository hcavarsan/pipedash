pub(crate) mod commands;
pub(crate) mod events;
#[cfg(any(target_os = "android", test))]
mod file_vault;
#[cfg(target_os = "android")]
pub(crate) mod headless;
mod preferences;
pub(crate) mod refresh;
pub(crate) mod settings;
pub(crate) mod store;
mod wake;

use std::sync::atomic::{
    AtomicBool,
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Duration;

use pipedash_core::application::{
    ObservedNotification,
    ShownNotification,
};
use pipedash_core::domain::NotificationPreferences;
use tokio::sync::{
    watch,
    Mutex as AsyncMutex,
    Notify,
    OnceCell,
};

use crate::error::ErrorResponse;

const REFRESH_BUDGET: Duration = Duration::from_secs(23);
const REFRESH_TIMEOUT: Duration = Duration::from_secs(26);
const PREFERENCES_RETRY_BUDGET: Duration = Duration::from_secs(3);

type Result<T, E = ErrorResponse> = std::result::Result<T, E>;

static PREFERENCES_SYNC: AsyncMutex<()> = AsyncMutex::const_new(());
static HOST_PREFERENCES_PENDING: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "android")]
pub fn set_storage_dir(app_data_dir: &std::path::Path) {
    store::set_dir(app_data_dir.join("background-notifications"));
}

#[derive(Default)]
pub struct LocalNotificationsState {
    initialized: OnceCell<()>,
    enabled: AtomicBool,
    revision: AtomicU64,
    authorization_revision: AtomicU64,
    settings: AsyncMutex<()>,
    observations: Arc<AsyncMutex<()>>,
    refresh: Mutex<Option<ActiveRefresh>>,
    push_problem: Mutex<Option<String>>,
    preferences: Mutex<NotificationPreferences>,
    sync: Notify,
}

struct ActiveRefresh {
    id: String,
    cancel: watch::Sender<bool>,
}

impl LocalNotificationsState {
    fn start_refresh(&self, id: &str) -> Option<watch::Receiver<bool>> {
        let mut active = self.refresh.lock().unwrap_or_else(PoisonError::into_inner);
        if active.is_some() || !self.enabled.load(Ordering::Acquire) {
            return None;
        }
        let (cancel, cancelled) = watch::channel(false);
        *active = Some(ActiveRefresh {
            id: id.to_owned(),
            cancel,
        });
        Some(cancelled)
    }

    fn cancel_refresh(&self, id: Option<&str>) {
        let mut active = self.refresh.lock().unwrap_or_else(PoisonError::into_inner);
        if id.is_some_and(|id| active.as_ref().is_none_or(|task| task.id != id)) {
            return;
        }
        if let Some(task) = active.take() {
            let _ = task.cancel.send(true);
        }
    }

    fn invalidate(&self) {
        self.revision.fetch_add(1, Ordering::AcqRel);
        self.authorization_revision.fetch_add(1, Ordering::AcqRel);
        self.cancel_refresh(None);
    }

    fn preferences(&self) -> MutexGuard<'_, NotificationPreferences> {
        self.preferences
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    #[cfg(desktop)]
    pub(crate) fn preferences_copy(&self) -> NotificationPreferences {
        self.preferences().clone()
    }

    pub(crate) fn admit(&self, notification: &ObservedNotification) -> Option<ShownNotification> {
        notification.admit(&mut self.preferences())
    }

    pub(crate) fn merge_preferences(
        &self, incoming: &NotificationPreferences,
    ) -> (NotificationPreferences, bool) {
        let mut current = self.preferences();
        let changed = current.merge(incoming);

        (current.clone(), changed)
    }
}

fn error(message: &str) -> ErrorResponse {
    ErrorResponse {
        error: message.to_owned(),
        details: Some("LOCAL_NOTIFICATIONS".to_owned()),
        code: None,
    }
}

async fn storage<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| error("Background notification storage could not be accessed"))?
}

#[cfg(test)]
mod tests;
