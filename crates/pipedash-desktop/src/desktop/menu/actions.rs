use std::time::Duration;

use pipedash_core::domain::{
    RefreshResult,
    RefreshState,
    ViewRef,
};
use tauri::{
    AppHandle,
    Manager,
};

use crate::{
    desktop::{
        notifications,
        projection::wake,
        settings::{
            commit_settings,
            DesktopSettingsPatch,
        },
        show_main_window,
        state::{
            failure,
            DesktopState,
            Ticket,
        },
    },
    error::ErrorResponse,
    native::{
        self,
        NativeIntent,
        NativeUiAction,
    },
    remote::{
        alerts::{
            client_request,
            client_request_within,
        },
        connection::RemoteConnection,
    },
};

pub(super) const REFRESH_FAILED: &str = "Some providers could not refresh";
const REFRESH_TIMEOUT: Duration = Duration::from_secs(60);
const ACTION_FAILED: &str = "Couldn't complete the native menu action. Open Pipedash for details.";

#[derive(Clone)]
pub(in crate::desktop) enum Command {
    Open,
    Navigate(String, Option<u64>),
    Ui(NativeUiAction),
    Unpin(String),
    StopWatch(String),
    Refresh,
    Notifications,
    Lock,
    Notice(String, String, Option<u64>),
    MarkAllRead,
    Quit,
}

pub(in crate::desktop) fn navigate(app: &AppHandle, path: &str, run_number: Option<u64>) {
    if let Err(error) = show_main_window(app) {
        tracing::warn!(%error, "Couldn't show the Pipedash window");
    }
    native::publish(
        app,
        NativeIntent::Link {
            path: path.to_string(),
            run_number,
        },
    );
}

fn show(app: &AppHandle) {
    if let Err(error) = show_main_window(app) {
        tracing::warn!(%error, "Couldn't show Pipedash");
    }
}

fn admits(app: &AppHandle, command: &Command, ticket: Option<Ticket>) -> bool {
    let state = app.state::<DesktopState>();
    let inner = state.lock();
    if ticket.is_some_and(|ticket| ticket != inner.ticket()) {
        return false;
    }
    inner.active()
        || !matches!(
            command,
            Command::Unpin(_)
                | Command::StopWatch(_)
                | Command::Refresh
                | Command::Notifications
                | Command::Lock
        )
}

pub(in crate::desktop) fn dispatch(app: &AppHandle, command: Command, ticket: Option<Ticket>) {
    if !admits(app, &command, ticket) {
        return;
    }
    match command {
        Command::Open => show(app),
        Command::Navigate(path, run) => navigate(app, &path, run),
        Command::Ui(action) => {
            show(app);
            native::publish(app, NativeIntent::Ui { action });
        }
        Command::Quit => app.exit(0),
        Command::Notice(key, path, run) => {
            notifications::mark_read(app, Some(&key));
            navigate(app, &path, run);
        }
        Command::MarkAllRead => {
            notifications::mark_read(app, None);
        }
        command => spawn_execute(app, command, ticket),
    }
}

fn spawn_execute(app: &AppHandle, command: Command, ticket: Option<Ticket>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if execute(&app, command, ticket).await.is_err() {
            record_action_failure(&app, ticket);
        }
    });
}

fn record_action_failure(app: &AppHandle, ticket: Option<Ticket>) {
    {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        if ticket.is_some_and(|ticket| !inner.current(ticket)) {
            return;
        }
        inner.refresh_message = Some(ACTION_FAILED.to_string());
    }
    wake(app);
}

pub(super) fn refresh_message(results: &[RefreshResult]) -> Option<String> {
    let any = |state: RefreshState| results.iter().any(|result| result.state == state);
    if any(RefreshState::Failed) {
        Some(REFRESH_FAILED.to_string())
    } else if any(RefreshState::Pending) {
        Some("Refresh still in progress".to_string())
    } else if any(RefreshState::NotFound) {
        Some("Provider no longer available".to_string())
    } else {
        None
    }
}

async fn execute_remote(
    app: &AppHandle, command: Command, ticket: Option<Ticket>,
) -> Result<(), ErrorResponse> {
    {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        if !inner.active() || ticket.is_some_and(|ticket| !inner.current(ticket)) {
            return Ok(());
        }
        if matches!(command, Command::Refresh) {
            if inner.refresh_active {
                return Ok(());
            }
            inner.refresh_active = true;
        }
    }
    let remote = app.state::<RemoteConnection>();
    match command {
        Command::Unpin(pipeline_id) => {
            let body = serde_json::json!({ "pipeline_id": pipeline_id, "member": false });
            let (status, _) = client_request(
                &remote,
                "PUT",
                "/pipeline-views/pinned/membership",
                Some(&body.to_string()),
            )
            .await?;
            if status != 200 {
                return Err(failure("The server couldn't unpin the pipeline"));
            }
        }
        Command::Refresh => {
            let answer =
                client_request_within(&remote, "POST", "/refresh/all", None, REFRESH_TIMEOUT).await;
            let message = match answer {
                Ok((200, body)) => serde_json::from_str::<Vec<RefreshResult>>(&body).map_or_else(
                    |_| Some(REFRESH_FAILED.to_string()),
                    |r| refresh_message(&r),
                ),
                _ => Some(REFRESH_FAILED.to_string()),
            };
            let state = app.state::<DesktopState>();
            let mut inner = state.lock();
            inner.refresh_active = false;
            inner.refresh_message = message;
        }
        _ => {}
    }
    wake(app);
    Ok(())
}

async fn toggle_notifications(
    app: &AppHandle, ticket: Option<Ticket>,
) -> Result<(), ErrorResponse> {
    let state = app.state::<DesktopState>();
    let _guard = state.settings_gate.lock().await;
    let (next, request) = {
        let inner = state.lock();
        if ticket.is_some_and(|ticket| !inner.current(ticket)) {
            return Ok(());
        }
        let request = DesktopSettingsPatch {
            notifications_enabled: Some(!inner.saved.settings.notifications_enabled),
            ..Default::default()
        };
        (inner.saved.patch(request)?, request)
    };
    commit_settings(app, next, request).await.map(|_| ())
}

async fn execute(
    app: &AppHandle, command: Command, ticket: Option<Ticket>,
) -> Result<(), ErrorResponse> {
    if matches!(command, Command::Notifications) {
        return toggle_notifications(app, ticket).await;
    }
    if app.state::<DesktopState>().lock().client.is_some() {
        return execute_remote(app, command, ticket).await;
    }
    execute_local(app, command, ticket).await
}

async fn execute_local(
    app: &AppHandle, command: Command, ticket: Option<Ticket>,
) -> Result<(), ErrorResponse> {
    let (core, current) = {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        if !inner.vault_ready || ticket.is_some_and(|ticket| !inner.matches(ticket)) {
            return Ok(());
        }
        let core = inner
            .core
            .clone()
            .ok_or_else(|| failure("Pipedash is not ready"))?;
        if matches!(command, Command::Refresh) {
            if inner.refresh_active {
                return Ok(());
            }
            inner.refresh_active = true;
        }
        (core, inner.ticket())
    };
    match command {
        Command::Unpin(pipeline_id) => {
            let lock = app.state::<crate::app_lock::AppLockState>();
            let _transition = lock.transition().await;
            if !app.state::<DesktopState>().lock().matches(current) {
                return Ok(());
            }
            core.pipeline_view_service
                .set_membership(ViewRef::Pinned, &pipeline_id, false)
                .await?;
        }
        Command::StopWatch(session_id) => {
            crate::build_watch::stop_matching(app, &session_id).await?;
        }
        Command::Lock => {
            crate::app_lock::lock_session(app).await;
        }
        Command::Refresh => {
            wake(app);
            let result = core.pipeline_service.refresh_all().await;
            let state = app.state::<DesktopState>();
            let mut inner = state.lock();
            if !inner.matches(current) {
                return Ok(());
            }
            inner.refresh_active = false;
            inner.refresh_message = result.map_or_else(
                |_| Some(REFRESH_FAILED.to_string()),
                |results| refresh_message(&results),
            );
        }
        _ => {}
    }
    wake(app);
    Ok(())
}
