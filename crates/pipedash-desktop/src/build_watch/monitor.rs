use std::sync::{
    Arc,
    Weak,
};

use pipedash_core::{
    application::{
        validate_watch_selection,
        watch_snapshot,
        RunWatch,
        WatchedRun,
    },
    CoreContext,
};
use pipedash_plugin_api::build_watch::{
    BuildWatchMonitoring,
    BuildWatchSnapshot,
    BuildWatchStatus,
    BuildWatchStopReason,
};
use tauri::{
    AppHandle,
    Manager,
};
use tauri_plugin_pipedash_native::PipedashNativeExt;

#[cfg(desktop)]
use super::native::{
    desktop_status,
    unavailable_status,
};
use super::{
    error,
    native::{
        is_watching,
        native_request,
        stop_current,
        stop_native,
    },
    session::{
        BuildWatchState,
        SessionGuard,
    },
};
use crate::{
    app_lock::AppLockState,
    error::ErrorResponse,
    MaybeCoreContext,
};

async fn available_core(app: &AppHandle) -> Result<Arc<CoreContext>, ErrorResponse> {
    if app.state::<AppLockState>().locked() {
        return Err(error("Unlock Pipedash before watching a build"));
    }
    #[cfg(desktop)]
    if !crate::desktop::ready(app) {
        return Err(error(
            "Pipedash is not ready to watch builds. Open General settings to retry.",
        ));
    }
    app.state::<MaybeCoreContext>()
        .get()
        .await
        .map_err(|message| error(&message))
}

async fn ensure_core(app: &AppHandle, generation: &Weak<CoreContext>) -> Result<(), ErrorResponse> {
    if !Weak::ptr_eq(generation, &Arc::downgrade(&available_core(app).await?)) {
        return Err(error(
            "The application session changed; start watching again",
        ));
    }
    Ok(())
}

async fn unavailable_reason(app: &AppHandle) -> Option<&'static str> {
    if app.state::<AppLockState>().locked() {
        return Some("Unlock Pipedash before watching a build");
    }
    if app.state::<MaybeCoreContext>().0.read().await.is_none() {
        return Some("Initialize Pipedash before watching a build");
    }
    #[cfg(desktop)]
    if !crate::desktop::ready(app) {
        return Some("Pipedash is not ready to watch builds. Open General settings to retry.");
    }
    None
}

async fn next_run(
    app: &AppHandle, session: &SessionGuard, core: &Weak<CoreContext>, watch: &mut RunWatch,
) -> Result<Option<WatchedRun>, ErrorResponse> {
    session
        .run(async {
            ensure_core(app, core).await?;
            let run = watch.next().await.transpose()?;
            ensure_core(app, core).await?;
            Ok(run)
        })
        .await
}

pub(super) async fn current_status(app: &AppHandle) -> Result<BuildWatchStatus, ErrorResponse> {
    let mut status = native_request(app.pipedash_native().build_watch_status()).await?;
    if !status.supported {
        return Ok(status);
    }
    if let Some(reason) = unavailable_reason(app).await {
        status = stop_current(app, BuildWatchStopReason::Interrupted).await?;
        #[cfg(desktop)]
        return Ok(unavailable_status(status, reason));
        #[cfg(mobile)]
        {
            status.available = false;
            status.reason = Some(reason.to_string());
        }
    } else if let Some(watch) = &status.watch {
        let state = app.state::<BuildWatchState>();
        if watch.monitoring == BuildWatchMonitoring::Watching {
            if state.session_id().is_none() {
                status =
                    stop_native(app, &watch.session_id, BuildWatchStopReason::Interrupted).await?;
                #[cfg(desktop)]
                if let Some(reason) = unavailable_reason(app).await {
                    return Ok(unavailable_status(status, reason));
                }
            }
        } else {
            state.cancel(Some(&watch.session_id));
        }
    }
    #[cfg(desktop)]
    let status = desktop_status(app, status);
    Ok(status)
}

pub(super) async fn start(
    app: AppHandle, state: &BuildWatchState, pipeline_id: String, run_number: i64,
) -> Result<BuildWatchStatus, ErrorResponse> {
    validate_watch_selection(&pipeline_id, run_number)?;
    let session = state.reserve()?;
    #[cfg(desktop)]
    {
        let generation = crate::desktop::generation(&app);
        crate::desktop::watch::reserve_watch(&app, &session.id)?;
        session.bind_generation(generation)?;
        if generation != crate::desktop::generation(&app) {
            return Err(error(
                "The application session changed; start watching again",
            ));
        }
    }
    let status = session
        .run(native_request(app.pipedash_native().build_watch_status()))
        .await?;
    if !status.supported || !status.available {
        return Err(error(
            status
                .reason
                .as_deref()
                .unwrap_or("Build watching is unavailable"),
        ));
    }
    if status
        .watch
        .as_ref()
        .is_some_and(|watch| watch.monitoring == BuildWatchMonitoring::Watching)
    {
        return Err(error(
            "Stop the current build watch before starting another",
        ));
    }
    let (mut watch, core) = session
        .run(async {
            let core = available_core(&app).await?;
            let watch = core
                .pipeline_service
                .watch_run(pipeline_id.clone(), run_number)
                .await?;
            session.bind_provider(watch.provider_id())?;
            Ok((watch, Arc::downgrade(&core)))
        })
        .await?;
    let selected = next_run(&app, &session, &core, &mut watch)
        .await?
        .ok_or_else(|| error("The selected build is no longer available"))?;
    let selected = watch_snapshot(&session.id, &pipeline_id, run_number, selected)?;
    if selected.status.is_terminal() {
        return Err(error("This build has already finished"));
    }
    let started = session
        .run(async {
            #[cfg(desktop)]
            ensure_core(&app, &core).await?;
            let status = native_request(app.pipedash_native().start_build_watch(&selected)).await?;
            #[cfg(desktop)]
            ensure_core(&app, &core).await?;
            Ok(status)
        })
        .await;
    let status = match started {
        Ok(status) if session.is_current() && is_watching(&status, &session.id) => status,
        result => {
            let stopped = stop_native(&app, &session.id, BuildWatchStopReason::Interrupted).await;
            return match result {
                Err(failure) => Err(failure),
                Ok(_) => {
                    let status = stopped?;
                    #[cfg(desktop)]
                    if let Some(reason) = unavailable_reason(&app).await {
                        return Ok(unavailable_status(status, reason));
                    }
                    Ok(status)
                }
            };
        }
    };
    tauri::async_runtime::spawn(monitor(app, session, core, watch, selected));
    Ok(status)
}

pub(super) async fn stop(app: &AppHandle) -> Result<BuildWatchStatus, ErrorResponse> {
    let status = stop_current(app, BuildWatchStopReason::Stopped).await?;
    #[cfg(desktop)]
    if let Some(reason) = unavailable_reason(app).await {
        return Ok(unavailable_status(status, reason));
    }
    #[cfg(desktop)]
    let status = desktop_status(app, status);
    Ok(status)
}

async fn monitor(
    app: AppHandle, session: SessionGuard, core: Weak<CoreContext>, mut watch: RunWatch,
    selected: BuildWatchSnapshot,
) {
    let result = forward_updates(&app, &session, &core, &mut watch, &selected).await;
    if result.is_err() && session.is_current() {
        let _ = stop_native(&app, &session.id, BuildWatchStopReason::Interrupted).await;
        tracing::warn!("Build watching was interrupted before a terminal result");
    }
}

async fn forward_updates(
    app: &AppHandle, session: &SessionGuard, core: &Weak<CoreContext>, watch: &mut RunWatch,
    selected: &BuildWatchSnapshot,
) -> Result<(), ErrorResponse> {
    while let Some(run) = next_run(app, session, core, watch).await? {
        let mut snapshot =
            watch_snapshot(&session.id, &selected.pipeline_id, selected.run_number, run)?;
        let status = session
            .run(native_request(app.pipedash_native().build_watch_status()))
            .await?;
        if !is_watching(&status, &session.id) {
            return Ok(());
        }
        ensure_core(app, core).await?;
        if let Some(current) = &status.watch {
            snapshot.updated_at = snapshot.updated_at.max(current.updated_at);
        }
        let status = session
            .run(native_request(
                app.pipedash_native().update_build_watch(&snapshot),
            ))
            .await?;
        if !is_watching(&status, &session.id) {
            return Ok(());
        }
        if snapshot.status.is_terminal() {
            return Err(error(
                "The native activity did not accept the completed build result",
            ));
        }
    }
    Ok(())
}
