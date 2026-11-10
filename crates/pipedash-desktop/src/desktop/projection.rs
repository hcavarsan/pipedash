use std::sync::Arc;

use pipedash_core::{
    domain::{
        DomainError,
        DomainResult,
        PipelineMenuSnapshot,
    },
    event::CoreEvent,
    CoreContext,
};
use tauri::{
    AppHandle,
    Manager,
    Runtime,
};

use super::{
    menu,
    notifications,
    state::{
        publish_status,
        DesktopState,
        Tasks,
    },
    tray::ClientLink,
};
use crate::remote::alerts::client_request;
use crate::remote::connection::RemoteConnection;

pub(super) fn wake<R: Runtime>(app: &AppHandle<R>) {
    if let Some(state) = app.try_state::<DesktopState>() {
        state.refresh.notify_one();
    }
}

pub(super) fn start_tasks(app: &AppHandle) {
    let (stop, stopped) = tokio::sync::watch::channel(());
    let mut handles = vec![tauri::async_runtime::spawn(project(
        app.clone(),
        stopped.clone(),
    ))];
    if let Some(remote) = app.try_state::<RemoteConnection>() {
        handles.push(tauri::async_runtime::spawn(follow_remote(
            app.clone(),
            remote.subscribe(),
            stopped,
        )));
    }
    let previous = app
        .state::<DesktopState>()
        .lock()
        .tasks
        .replace(Tasks { stop, handles });
    if let Some(previous) = previous {
        previous.stop();
    }
}

fn stop_tasks<R: Runtime>(app: &AppHandle<R>) {
    let tasks = app
        .try_state::<DesktopState>()
        .and_then(|state| state.lock().tasks.take());
    if let Some(tasks) = tasks {
        tasks.stop();
    }
}

async fn project(app: AppHandle, mut stopped: tokio::sync::watch::Receiver<()>) {
    let state = app.state::<DesktopState>();
    loop {
        tokio::select! {
            biased;
            _ = stopped.changed() => return,
            () = async {
                state.refresh.notified().await;
                refresh_projection(&app).await;
            } => {}
        }
    }
}

async fn follow_remote(
    app: AppHandle, mut changes: tokio::sync::watch::Receiver<u64>,
    mut stopped: tokio::sync::watch::Receiver<()>,
) {
    loop {
        tokio::select! {
            biased;
            _ = stopped.changed() => return,
            changed = changes.changed() => {
                if changed.is_err() {
                    return;
                }
            }
        }
        wake(&app);
    }
}

pub(crate) async fn attach(
    app: &AppHandle, core: Arc<CoreContext>, generation: u64, vault_ready: bool,
) {
    {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        inner.epoch += 1;
        inner.generation = generation;
        inner.core = Some(core);
        inner.vault_ready = vault_ready;
        inner.snapshot = None;
        inner.data_error = None;
        inner.watch = None;
        inner.watch_session = None;
        inner.history.clear();
    }
    let history = app.state::<DesktopState>().lock().history.clone();
    notifications::publish(app, &history);
    refresh_projection(app).await;
    publish_status(app);
    super::updater::core_attached(app);
}

pub(crate) fn detach(app: &AppHandle) {
    if reset(app) {
        start_tasks(app);
        wake(app);
    }
}

pub(super) fn shutdown(app: &AppHandle) {
    stop_tasks(app);
    reset(app);
}

fn reset(app: &AppHandle) -> bool {
    let Some(state) = app.try_state::<DesktopState>() else {
        return false;
    };
    let history = {
        let mut inner = state.lock();
        inner.epoch += 1;
        inner.vault_ready = false;
        inner.core = None;
        inner.snapshot = None;
        inner.watch_session = None;
        inner.watch = None;
        inner.refresh_active = false;
        inner.refresh_message = None;
        inner.data_error = None;
        inner.history.clear();
        inner.history.clone()
    };
    crate::build_watch::interrupt(app);
    notifications::publish(app, &history);
    publish_status(app);
    menu::schedule_render(app);
    true
}

pub(super) fn parse_menu(status: u16, body: &str) -> DomainResult<PipelineMenuSnapshot> {
    if status != 200 {
        return Err(DomainError::ApiError(format!(
            "The server answered {status}"
        )));
    }
    serde_json::from_str(body).map_err(|_| {
        DomainError::ApiError("The server sent pinned pipelines Pipedash can't read".to_string())
    })
}

async fn client_menu(app: &AppHandle) -> DomainResult<PipelineMenuSnapshot> {
    let remote = app.state::<RemoteConnection>();
    let (status, body) = client_request(&remote, "GET", "/pipeline-views/menu", None)
        .await
        .map_err(|error| DomainError::NetworkError(error.to_string()))?;
    parse_menu(status, &body)
}

pub(super) async fn refresh_projection(app: &AppHandle) {
    let state = app.state::<DesktopState>();
    let _projection = state.projection_gate.lock().await;
    let (link, server) = match app
        .try_state::<RemoteConnection>()
        .and_then(|remote| remote.tray_link())
    {
        Some((host, status, server)) => (Some(ClientLink { host, status }), Some(server)),
        None => (None, None),
    };
    let (input, history) = {
        let mut inner = state.lock();
        let moved = inner.follow(link, server);
        if inner.client.as_ref().is_some_and(|link| !link.connected()) {
            inner.snapshot = None;
            inner.data_error = None;
        }
        let input = inner
            .core
            .as_ref()
            .filter(|_| inner.vault_ready && inner.client.is_none())
            .map(|core| (Arc::clone(core), inner.ticket()));
        (input, moved.then(|| inner.history.clone()))
    };
    if let Some(history) = history {
        notifications::publish(app, &history);
    }
    if let Some((core, ticket)) = input {
        let projection = core.pipeline_view_service.menu_snapshot().await;
        {
            let mut inner = state.lock();
            if !inner.matches(ticket) {
                return;
            }
            inner.apply_menu_result(projection);
        }
        publish_status(app);
    } else {
        let ticket = {
            let inner = state.lock();
            inner
                .client
                .as_ref()
                .is_some_and(ClientLink::connected)
                .then(|| inner.ticket())
        };
        if let Some(ticket) = ticket {
            let projection = client_menu(app).await;
            let mut inner = state.lock();
            if inner.current(ticket) {
                inner.apply_menu_result(projection);
            }
        }
        publish_status(app);
    }
    menu::render(app);
}

pub(super) fn waking(name: &str) -> bool {
    matches!(
        name,
        "pipeline-views-changed"
            | "pipelines-updated"
            | "pipeline-status-changed"
            | "providers-changed"
            | "provider-added"
            | "provider-updated"
            | "provider-removed"
            | "provider-status-updated"
    )
}

pub(crate) fn observe<R: Runtime>(app: &AppHandle<R>, event: &CoreEvent) {
    if waking(event.event_name())
        || matches!(event, CoreEvent::MigrationComplete { success: true, .. })
    {
        wake(app);
    }
}

pub(crate) fn observe_server_event<R: Runtime>(app: &AppHandle<R>, name: &str) {
    if waking(name) {
        wake(app);
    }
}
