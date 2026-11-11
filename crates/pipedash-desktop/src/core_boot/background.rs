use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
};

use pipedash_core::event::CoreEvent;
use pipedash_core::CoreContext;
use tauri::{
    AppHandle,
    Manager,
};
use tokio::task::JoinHandle;

use super::BootKind;

#[derive(Default)]
pub(crate) struct BackgroundInit(Mutex<Option<JoinHandle<()>>>);

impl BackgroundInit {
    pub(super) fn start(&self, core: Arc<CoreContext>, kind: BootKind) {
        let task = tokio::spawn(initialize(core, kind));
        let previous = self.slot().replace(task);
        if let Some(previous) = previous {
            previous.abort();
        }
    }

    async fn cancel(&self) {
        let task = self.slot().take();
        let Some(task) = task else {
            return;
        };
        task.abort();
        let _ = task.await;
    }

    fn slot(&self) -> MutexGuard<'_, Option<JoinHandle<()>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub(crate) async fn cancel_init(app: &AppHandle) {
    if let Some(init) = app.try_state::<BackgroundInit>() {
        init.cancel().await;
    }
}

async fn initialize(core: Arc<CoreContext>, kind: BootKind) {
    if let Err(e) = core.warmup_token_store().await {
        tracing::warn!("Token store warmup failed: {}", e);
    }

    if kind == BootKind::Unlock {
        if let Err(e) = core.provider_service.load_all_providers().await {
            tracing::warn!("Failed to load providers after unlock: {}", e);
        }
    }

    core.start_background_tasks().await;

    if kind.announces_unlock() {
        core.event_bus.emit(CoreEvent::VaultUnlocked).await;
    }
}
