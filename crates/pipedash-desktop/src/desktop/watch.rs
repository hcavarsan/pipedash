use pipedash_plugin_api::build_watch::{
    BuildWatchMonitoring,
    BuildWatchSnapshot,
    BuildWatchStatus,
};
use tauri::{
    AppHandle,
    Manager,
    Runtime,
};

use super::{
    notifications,
    state::{
        failure,
        DesktopState,
    },
    tray_changed,
};
use crate::error::ErrorResponse;

pub(crate) fn reserve_watch<R: Runtime>(
    app: &AppHandle<R>, session_id: &str,
) -> Result<(), ErrorResponse> {
    let state = app.state::<DesktopState>();
    let mut inner = state.lock();
    if !inner.vault_ready {
        return Err(failure("Unlock Pipedash before watching a run"));
    }
    inner.watch_session = Some((session_id.to_string(), inner.ticket()));
    Ok(())
}

pub(crate) fn owns_watch<R: Runtime>(app: &AppHandle<R>, watch: &BuildWatchSnapshot) -> bool {
    let state = app.state::<DesktopState>();
    let inner = state.lock();
    inner.vault_ready
        && inner
            .watch
            .as_ref()
            .is_some_and(|current| current.session_id == watch.session_id)
}

pub(crate) fn watch_changed<R: Runtime>(app: &AppHandle<R>, status: &BuildWatchStatus) -> bool {
    let Some(watch) = &status.watch else {
        return false;
    };
    let state = app.state::<DesktopState>();
    let (notice, delivery) = {
        let mut inner = state.lock();
        let Some((session, ticket)) = &inner.watch_session else {
            return false;
        };
        if session != &watch.session_id || !inner.matches(*ticket) {
            return false;
        }
        if inner
            .watch
            .as_ref()
            .filter(|old| old.session_id == watch.session_id)
            .is_some_and(|old| {
                watch.updated_at < old.updated_at
                    || old.monitoring != BuildWatchMonitoring::Watching
            })
        {
            return false;
        }
        let completed = watch.monitoring == BuildWatchMonitoring::Completed
            && inner
                .watch
                .as_ref()
                .filter(|old| old.session_id == watch.session_id)
                .is_none_or(|old| old.monitoring == BuildWatchMonitoring::Watching);
        let notice = completed.then(|| notifications::watched(watch));
        let delivery = notice.as_ref().map(|notice| inner.record(notice));
        inner.watch = Some(watch.clone());
        (notice, delivery)
    };
    if let (Some(notice), Some((history, deliver))) = (notice, delivery) {
        notifications::announce(app, &notice, &history, deliver);
    }
    tray_changed(app);
    true
}
