mod settings;

use std::path::{
    Path,
    PathBuf,
};
use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Duration;

use pipedash_core::application::services::device_service::DeviceView;
use pipedash_core::event::CoreEvent;
use pipedash_core::CoreContext;
use pipedash_web::pairing::{
    create_pairing,
    PairingResponse,
};
use pipedash_web::state::AppState;
use pipedash_web::ws::event_bus::WebSocketEventBus;
use tauri::async_runtime::JoinHandle;
use tauri::{
    AppHandle,
    Manager,
};
use tokio::sync::{
    broadcast,
    oneshot,
};

use self::settings::{
    bind_failure,
    failure,
    lan_ip,
    read_settings,
    write_settings,
    HostSettings,
    SharingFailure,
    NO_NETWORK,
};
use super::{
    SharingErrorCode,
    SharingStatus,
};
use crate::error::ErrorResponse;
use crate::MaybeCoreContext;

const EVENT_BUFFER: usize = 1024;
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

pub struct HostEvents(pub broadcast::Sender<CoreEvent>);

impl Default for HostEvents {
    fn default() -> Self {
        Self(broadcast::channel(EVENT_BUFFER).0)
    }
}

struct Running {
    state: AppState,
    url: String,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

struct HostInner {
    settings: HostSettings,
    running: Option<Running>,
    error: Option<SharingFailure>,
}

pub struct HostState {
    dir: PathBuf,
    inner: Mutex<HostInner>,
    op: tokio::sync::Mutex<()>,
}

fn error(message: &str, code: Option<&str>) -> ErrorResponse {
    ErrorResponse {
        error: message.to_owned(),
        details: None,
        code: code.map(str::to_owned),
    }
}

impl HostState {
    pub fn load(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            inner: Mutex::new(HostInner {
                settings: read_settings(dir),
                running: None,
                error: None,
            }),
            op: tokio::sync::Mutex::new(()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, HostInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn status(&self) -> SharingStatus {
        let inner = self.lock();
        SharingStatus {
            enabled: inner.settings.enabled,
            running: inner.running.is_some(),
            url: inner.running.as_ref().map(|running| running.url.clone()),
            error: inner.error.as_ref().map(|failure| failure.message.clone()),
            error_code: inner.error.as_ref().and_then(|failure| failure.code),
        }
    }

    fn update_settings(&self, change: impl FnOnce(&mut HostSettings)) -> Result<(), ErrorResponse> {
        let mut inner = self.lock();
        let mut settings = inner.settings;
        change(&mut settings);
        write_settings(&self.dir, settings).map_err(|e| {
            tracing::warn!(error = %e, "Couldn't save the sharing settings");
            error("Couldn't save the sharing settings.", None)
        })?;
        inner.settings = settings;

        Ok(())
    }
}

async fn launch(app: &AppHandle, host: &HostState) -> Result<Running, SharingFailure> {
    let ip = lan_ip().ok_or_else(|| failure(NO_NETWORK, Some(SharingErrorCode::NoNetwork)))?;
    let saved_port = host.lock().settings.port;
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", saved_port.unwrap_or(0)))
        .await
        .map_err(|e| bind_failure(saved_port, &e))?;
    let port = listener
        .local_addr()
        .map_err(|e| failure(format!("Couldn't start sharing: {e}"), None))?
        .port();
    if saved_port.is_none() {
        if let Err(e) = host.update_settings(|settings| settings.port = Some(port)) {
            tracing::warn!(error = %e.error, "The sharing port won't survive a restart");
        }
    }

    let core = app.state::<MaybeCoreContext>().0.read().await.clone();
    let bus = Arc::new(WebSocketEventBus::from_sender(
        app.state::<HostEvents>().0.clone(),
    ));
    let state = AppState::hosted(
        bus,
        core.map(|core| (*core).clone()),
        &crate::remote::pairing::device_name(),
        crate::plugin_registry::plugin_registry(),
    )
    .with_push_relay(crate::push::relay_url());
    pipedash_web::push::spawn_alert_push(state.clone());

    let (shutdown, stopped) = oneshot::channel();
    let serving = state.clone();
    let task = tauri::async_runtime::spawn(async move {
        let stopped = async {
            let _ = stopped.await;
        };
        if let Err(e) = pipedash_web::serve_host(listener, serving, stopped).await {
            tracing::warn!(error = %e, "The sharing server stopped");
        }
    });

    Ok(Running {
        state,
        url: format!("http://{ip}:{port}"),
        shutdown,
        task,
    })
}

async fn start_locked(app: &AppHandle, host: &HostState) {
    if host.lock().running.is_some() {
        return;
    }

    let launched = launch(app, host).await;
    let mut inner = host.lock();
    match launched {
        Ok(running) => {
            inner.error = None;
            inner.running = Some(running);
        }
        Err(message) => inner.error = Some(message),
    }
}

async fn stop_locked(host: &HostState) {
    let running = {
        let mut inner = host.lock();
        inner.error = None;
        inner.running.take()
    };
    if let Some(running) = running {
        let _ = running.shutdown.send(());
        running.state.set_core(None).await;
        let mut task = running.task;
        if tokio::time::timeout(STOP_TIMEOUT, &mut task).await.is_err() {
            tracing::warn!("The sharing server didn't stop in time, aborting it");
            task.abort();
        }
    }
}

pub(super) async fn start_if_enabled(app: &AppHandle) {
    let host = app.state::<HostState>();
    let _op = host.op.lock().await;
    let enabled = host.lock().settings.enabled;
    if enabled {
        start_locked(app, &host).await;
    }
}

pub(super) async fn stop(app: &AppHandle) {
    let host = app.state::<HostState>();
    let _op = host.op.lock().await;
    stop_locked(&host).await;
}

pub(super) async fn core_changed(app: &AppHandle, core: Option<&Arc<CoreContext>>) {
    let host = app.state::<HostState>();
    let _op = host.op.lock().await;
    let state = host
        .lock()
        .running
        .as_ref()
        .map(|running| running.state.clone());
    if let Some(state) = state {
        state.set_core(core.map(|core| (**core).clone())).await;
    }
}

pub(super) fn status(app: &AppHandle) -> Result<SharingStatus, ErrorResponse> {
    Ok(app.state::<HostState>().status())
}

pub(super) async fn set_sharing(
    app: &AppHandle, enabled: bool,
) -> Result<SharingStatus, ErrorResponse> {
    let host = app.state::<HostState>();
    let _op = host.op.lock().await;
    host.update_settings(|settings| settings.enabled = enabled)?;
    if enabled {
        start_locked(app, &host).await;
    } else {
        stop_locked(&host).await;
    }

    Ok(host.status())
}

pub(super) async fn reset_port(app: &AppHandle) -> Result<SharingStatus, ErrorResponse> {
    let host = app.state::<HostState>();
    let _op = host.op.lock().await;
    stop_locked(&host).await;
    host.update_settings(|settings| settings.port = None)?;
    let enabled = host.lock().settings.enabled;
    if enabled {
        start_locked(app, &host).await;
    }

    Ok(host.status())
}

async fn core(app: &AppHandle) -> Result<Arc<CoreContext>, ErrorResponse> {
    app.state::<MaybeCoreContext>()
        .get()
        .await
        .map_err(|message| error(&message, None))
}

pub(super) async fn list_devices(app: &AppHandle) -> Result<Vec<DeviceView>, ErrorResponse> {
    Ok(core(app)
        .await?
        .list_devices(None, !pipedash_web::demo::enabled())
        .await?)
}

pub(super) async fn create_pairing_code(app: &AppHandle) -> Result<PairingResponse, ErrorResponse> {
    let running = app
        .state::<HostState>()
        .lock()
        .running
        .as_ref()
        .map(|running| (running.state.clone(), running.url.clone()));
    let Some((state, url)) = running else {
        return Err(error("Turn on sharing first.", None));
    };

    create_pairing(&state, &url).map_err(|e| ErrorResponse {
        error: e.error.message,
        details: None,
        code: Some(e.error.code),
    })
}

pub(super) async fn remove_device(app: &AppHandle, id: &str) -> Result<(), ErrorResponse> {
    if core(app).await?.revoke_device(id).await? {
        Ok(())
    } else {
        Err(error(
            "This device is no longer paired.",
            Some("DEVICE_NOT_FOUND"),
        ))
    }
}

#[cfg(test)]
mod tests;
