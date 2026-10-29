use std::path::Path;
use std::sync::{
    Mutex,
    PoisonError,
};

use pipedash_core::application::{
    refresh_notifications,
    LocalNotification,
};
use tokio::sync::watch;

use super::{
    store,
    REFRESH_BUDGET,
    REFRESH_TIMEOUT,
};

static ACTIVE: Mutex<Option<watch::Sender<bool>>> = Mutex::new(None);

pub fn cancel() {
    let active = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(cancel) = active {
        let _ = cancel.send(true);
    }
}

pub fn run(data_dir: &Path) -> Vec<LocalNotification> {
    store::set_dir(data_dir.join("background-notifications"));
    let Ok(generation) = store::generation() else {
        return Vec::new();
    };
    let Ok(Some(authorization)) = store::authorization() else {
        return Vec::new();
    };
    let Ok(mut observations) = store::observations() else {
        return Vec::new();
    };
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return Vec::new();
    };
    let (cancel, mut cancelled) = watch::channel(false);
    *ACTIVE.lock().unwrap_or_else(PoisonError::into_inner) = Some(cancel);
    let notices = runtime.block_on(async {
        tokio::select! {
            biased;
            _ = cancelled.changed() => None,
            result = tokio::time::timeout(
                REFRESH_TIMEOUT,
                refresh_notifications(
                    &authorization.providers,
                    &authorization.proxy_url,
                    authorization.status_pages_enabled,
                    &mut observations,
                    REFRESH_BUDGET,
                ),
            ) => result.ok().and_then(Result::ok),
        }
    });
    ACTIVE.lock().unwrap_or_else(PoisonError::into_inner).take();
    match notices {
        Some(notices)
            if matches!(
                store::save_observations_if_unchanged(&observations, generation),
                Ok(true)
            ) =>
        {
            notices
        }
        _ => Vec::new(),
    }
}
