#[cfg(desktop)]
use std::sync::{
    atomic::{
        AtomicBool,
        Ordering,
    },
    Arc,
};

use tauri::ipc::{
    CommandArg,
    CommandItem,
    InvokeError,
};
use tauri::Runtime;
#[cfg(desktop)]
use tauri::{
    AppHandle,
    Manager,
};
#[cfg(desktop)]
use tokio::sync::{
    Mutex,
    OwnedRwLockReadGuard,
    OwnedRwLockWriteGuard,
    RwLock,
};

#[cfg(desktop)]
use crate::{
    commands::ErrorResponse,
    MaybeCoreContext,
};

pub struct OperationLease {
    #[cfg(desktop)]
    _guard: OwnedRwLockReadGuard<()>,
}

impl<'de, R: Runtime> CommandArg<'de, R> for OperationLease {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        #[cfg(desktop)]
        {
            let lifecycle = command
                .message
                .webview_ref()
                .try_state::<DesktopLifecycle>()
                .ok_or_else(|| InvokeError::from(updating_error()))?;
            lifecycle.operation().map_err(InvokeError::from)
        }
        #[cfg(mobile)]
        {
            let _ = command;
            Ok(Self {})
        }
    }
}

#[cfg(desktop)]
#[derive(Default)]
pub struct DesktopLifecycle {
    gate: Arc<RwLock<()>>,
    admission_closed: Arc<AtomicBool>,
    startup: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    startup_failed: AtomicBool,
}

#[cfg(desktop)]
impl DesktopLifecycle {
    pub fn busy(&self) -> bool {
        self.admission_closed.load(Ordering::SeqCst)
    }

    fn operation(&self) -> Result<OperationLease, ErrorResponse> {
        if self.busy() {
            return Err(updating_error());
        }
        let guard = Arc::clone(&self.gate)
            .try_read_owned()
            .map_err(|_| updating_error())?;
        if self.busy() {
            return Err(updating_error());
        }
        Ok(OperationLease { _guard: guard })
    }

    pub async fn acquire_exclusive(&self) -> Result<ExclusiveOperation, ErrorResponse> {
        self.admission_closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| updating_error())?;
        let mut exclusive = ExclusiveOperation {
            guard: None,
            admission_closed: Arc::clone(&self.admission_closed),
        };
        exclusive.guard = Some(Arc::clone(&self.gate).write_owned().await);
        Ok(exclusive)
    }

    pub async fn track_startup(&self, startup: tauri::async_runtime::JoinHandle<()>) {
        self.startup_failed.store(false, Ordering::SeqCst);
        *self.startup.lock().await = Some(startup);
    }

    pub async fn join_startup(&self) -> Result<(), String> {
        if self.startup_failed.load(Ordering::SeqCst) {
            return Err("Application startup failed. Reopen Pipedash before updating.".to_owned());
        }
        let startup = self.startup.lock().await.take();
        if let Some(startup) = startup {
            if let Err(error) = startup.await {
                self.startup_failed.store(true, Ordering::SeqCst);
                return Err(format!("Application startup failed: {error}"));
            }
        }
        Ok(())
    }

    pub async fn drain_core(&self, app: &AppHandle) -> Result<bool, String> {
        self.join_startup().await?;
        let core = app.state::<MaybeCoreContext>().0.write().await.take();
        let existed = core.is_some();
        crate::build_watch::interrupt(app);
        if let Some(core) = core {
            core.shutdown().await;
        }
        Ok(existed)
    }
}

#[cfg(desktop)]
pub struct ExclusiveOperation {
    guard: Option<OwnedRwLockWriteGuard<()>>,
    admission_closed: Arc<AtomicBool>,
}

#[cfg(desktop)]
impl Drop for ExclusiveOperation {
    fn drop(&mut self) {
        self.guard.take();
        self.admission_closed.store(false, Ordering::SeqCst);
    }
}

#[cfg(desktop)]
fn updating_error() -> ErrorResponse {
    ErrorResponse {
        error: "Pipedash is preparing an update. Try again shortly.".to_string(),
        details: None,
        code: Some("APP_UPDATING".to_string()),
    }
}

#[cfg(all(test, desktop))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn exclusive_admission_waits_for_existing_work_and_rejects_new_work() {
        let lifecycle = DesktopLifecycle::default();
        let operation = lifecycle.operation().unwrap();
        let exclusive = lifecycle.acquire_exclusive();
        tokio::pin!(exclusive);
        assert!(futures::poll!(&mut exclusive).is_pending());
        assert_eq!(
            lifecycle.operation().err().unwrap().code.as_deref(),
            Some("APP_UPDATING")
        );
        drop(operation);
        let exclusive = exclusive.await.unwrap();
        assert!(lifecycle.acquire_exclusive().await.is_err());
        drop(exclusive);
        assert!(lifecycle.operation().is_ok());
    }

    #[tokio::test]
    async fn cancelling_exclusive_admission_reopens_operations() {
        let lifecycle = DesktopLifecycle::default();
        let operation = lifecycle.operation().unwrap();
        {
            let exclusive = lifecycle.acquire_exclusive();
            tokio::pin!(exclusive);
            assert!(futures::poll!(&mut exclusive).is_pending());
        }
        assert!(!lifecycle.busy());
        assert!(lifecycle.operation().is_ok());
        drop(operation);
    }

    #[tokio::test]
    async fn startup_drain_waits_for_both_spawned_layers() {
        let lifecycle = DesktopLifecycle::default();
        let (started, entered) = tokio::sync::oneshot::channel();
        let (finish, wait) = tokio::sync::oneshot::channel();
        lifecycle
            .track_startup(tauri::async_runtime::spawn(async move {
                tokio::spawn(async move {
                    started.send(()).unwrap();
                    wait.await.unwrap();
                })
                .await
                .unwrap();
            }))
            .await;
        entered.await.unwrap();
        let drain = lifecycle.join_startup();
        tokio::pin!(drain);
        assert!(futures::poll!(&mut drain).is_pending());
        finish.send(()).unwrap();
        drain.await.unwrap();
        lifecycle.join_startup().await.unwrap();
    }

    #[tokio::test]
    async fn failed_startup_drain_is_not_successful_preparation() {
        let lifecycle = DesktopLifecycle::default();
        lifecycle
            .track_startup(tauri::async_runtime::spawn(async {
                let inner: tokio::task::JoinHandle<()> = tokio::spawn(async {
                    panic!("nested startup failed");
                });
                if let Err(error) = inner.await {
                    std::panic::resume_unwind(Box::new(error));
                }
            }))
            .await;
        assert!(lifecycle.join_startup().await.is_err());
        assert!(lifecycle.join_startup().await.is_err());
    }
}
