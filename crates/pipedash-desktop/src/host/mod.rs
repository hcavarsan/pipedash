use serde::Serialize;
use tauri::AppHandle;

pub use self::imp::{
    core_changed,
    start_if_enabled,
    stop,
};
#[cfg(desktop)]
pub use self::imp::{
    HostEvents,
    HostState,
};
use crate::commands::ErrorResponse;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharingStatus {
    pub enabled: bool,
    pub running: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}

#[cfg(desktop)]
mod imp {
    use std::io::ErrorKind;
    use std::net::{
        IpAddr,
        UdpSocket,
    };
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

    use pipedash_core::event::CoreEvent;
    use pipedash_core::CoreContext;
    use pipedash_web::pairing::{
        create_pairing,
        device_response,
    };
    use pipedash_web::state::AppState;
    use pipedash_web::ws::WebSocketEventBus;
    use serde::{
        Deserialize,
        Serialize,
    };
    use tauri::async_runtime::JoinHandle;
    use tauri::{
        AppHandle,
        Manager,
    };
    use tokio::sync::{
        broadcast,
        oneshot,
    };

    use super::SharingStatus;
    use crate::commands::ErrorResponse;
    use crate::MaybeCoreContext;

    const SETTINGS_FILE: &str = "host.json";
    const TMP_SETTINGS_FILE: &str = "host.json.tmp";
    const EVENT_BUFFER: usize = 1024;
    const STOP_TIMEOUT: Duration = Duration::from_secs(3);
    const NO_NETWORK: &str = "Connect this computer to a network to share Pipedash.";

    pub type Device = pipedash_web::pairing::DeviceResponse;
    pub type Pairing = pipedash_web::pairing::PairingResponse;

    pub struct HostEvents(pub broadcast::Sender<CoreEvent>);

    impl Default for HostEvents {
        fn default() -> Self {
            Self(broadcast::channel(EVENT_BUFFER).0)
        }
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(default)]
    struct HostSettings {
        enabled: bool,
        port: Option<u16>,
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
        error: Option<String>,
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

    fn read_settings(dir: &Path) -> HostSettings {
        match std::fs::read(dir.join(SETTINGS_FILE)) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Ignoring unreadable sharing settings");
                HostSettings::default()
            }),
            Err(_) => HostSettings::default(),
        }
    }

    fn write_settings(dir: &Path, settings: HostSettings) -> std::io::Result<()> {
        let tmp = dir.join(TMP_SETTINGS_FILE);
        std::fs::write(&tmp, serde_json::to_vec(&settings)?)?;
        std::fs::rename(&tmp, dir.join(SETTINGS_FILE))
    }

    fn lan_ip() -> Option<IpAddr> {
        let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect("192.0.2.1:9").ok()?;
        let ip = socket.local_addr().ok()?.ip();

        (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
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
                error: inner.error.clone(),
            }
        }

        fn update_settings(
            &self, change: impl FnOnce(&mut HostSettings),
        ) -> Result<(), ErrorResponse> {
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

    async fn launch(app: &AppHandle, host: &HostState) -> Result<Running, String> {
        let ip = lan_ip().ok_or_else(|| NO_NETWORK.to_owned())?;
        let saved_port = host.lock().settings.port;
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", saved_port.unwrap_or(0)))
            .await
            .map_err(|e| match saved_port {
                Some(port) if e.kind() == ErrorKind::AddrInUse => {
                    format!("Port {port} is in use by another app.")
                }
                _ => format!("Couldn't start sharing: {e}"),
            })?;
        let port = listener
            .local_addr()
            .map_err(|e| format!("Couldn't start sharing: {e}"))?
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
            &crate::remote::device_name(),
        );

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

    pub async fn start_if_enabled(app: &AppHandle) {
        let host = app.state::<HostState>();
        let _op = host.op.lock().await;
        let enabled = host.lock().settings.enabled;
        if enabled {
            start_locked(app, &host).await;
        }
    }

    pub async fn stop(app: &AppHandle) {
        let host = app.state::<HostState>();
        let _op = host.op.lock().await;
        stop_locked(&host).await;
    }

    pub async fn core_changed(app: &AppHandle, core: Option<&Arc<CoreContext>>) {
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

    pub fn status(app: &AppHandle) -> Result<SharingStatus, ErrorResponse> {
        Ok(app.state::<HostState>().status())
    }

    pub async fn set_sharing(
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

    pub async fn reset_port(app: &AppHandle) -> Result<SharingStatus, ErrorResponse> {
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

    pub async fn list_devices(app: &AppHandle) -> Result<Vec<Device>, ErrorResponse> {
        let devices = core(app).await?.devices().list().await?;

        Ok(devices
            .into_iter()
            .map(|device| device_response(device, None))
            .collect())
    }

    pub async fn create_pairing_code(app: &AppHandle) -> Result<Pairing, ErrorResponse> {
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

    pub async fn remove_device(app: &AppHandle, id: &str) -> Result<(), ErrorResponse> {
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
    mod tests {
        use super::*;

        #[test]
        fn settings_default_when_missing_or_unreadable_and_survive_a_restart() {
            let dir = tempfile::tempdir().unwrap();
            assert_eq!(read_settings(dir.path()), HostSettings::default());

            std::fs::write(dir.path().join(SETTINGS_FILE), b"{oops").unwrap();
            assert_eq!(read_settings(dir.path()), HostSettings::default());

            let settings = HostSettings {
                enabled: true,
                port: Some(52411),
            };
            write_settings(dir.path(), settings).unwrap();
            assert_eq!(read_settings(dir.path()), settings);
            assert!(!dir.path().join(TMP_SETTINGS_FILE).exists());
        }

        #[test]
        fn status_reports_what_the_settings_and_errors_say() {
            let dir = tempfile::tempdir().unwrap();
            let host = HostState::load(dir.path());
            assert_eq!(
                host.status(),
                SharingStatus {
                    enabled: false,
                    running: false,
                    url: None,
                    error: None,
                }
            );

            host.update_settings(|settings| settings.enabled = true)
                .unwrap();
            host.lock().error = Some("Port 1 is in use by another app.".to_owned());
            let status = host.status();
            assert!(status.enabled && !status.running);
            assert_eq!(
                status.error.as_deref(),
                Some("Port 1 is in use by another app.")
            );
            assert!(read_settings(dir.path()).enabled);
        }
    }
}

#[cfg(not(desktop))]
mod imp {
    use std::convert::Infallible;
    use std::sync::Arc;

    use pipedash_core::CoreContext;
    use tauri::AppHandle;

    use super::SharingStatus;
    use crate::commands::ErrorResponse;

    pub type Device = Infallible;
    pub type Pairing = Infallible;

    fn unsupported<T>() -> Result<T, ErrorResponse> {
        Err(ErrorResponse {
            error: "Sharing is available only on computers.".to_owned(),
            details: None,
            code: None,
        })
    }

    pub async fn start_if_enabled(_app: &AppHandle) {}

    pub async fn stop(_app: &AppHandle) {}

    pub async fn core_changed(_app: &AppHandle, _core: Option<&Arc<CoreContext>>) {}

    pub fn status(_app: &AppHandle) -> Result<SharingStatus, ErrorResponse> {
        unsupported()
    }

    pub async fn set_sharing(
        _app: &AppHandle, _enabled: bool,
    ) -> Result<SharingStatus, ErrorResponse> {
        unsupported()
    }

    pub async fn reset_port(_app: &AppHandle) -> Result<SharingStatus, ErrorResponse> {
        unsupported()
    }

    pub async fn list_devices(_app: &AppHandle) -> Result<Vec<Device>, ErrorResponse> {
        unsupported()
    }

    pub async fn create_pairing_code(_app: &AppHandle) -> Result<Pairing, ErrorResponse> {
        unsupported()
    }

    pub async fn remove_device(_app: &AppHandle, _id: &str) -> Result<(), ErrorResponse> {
        unsupported()
    }
}

#[tauri::command]
pub fn get_sharing_status(app: AppHandle) -> Result<SharingStatus, ErrorResponse> {
    imp::status(&app)
}

#[tauri::command]
pub async fn set_sharing(app: AppHandle, enabled: bool) -> Result<SharingStatus, ErrorResponse> {
    imp::set_sharing(&app, enabled).await
}

#[tauri::command]
pub async fn reset_sharing_port(app: AppHandle) -> Result<SharingStatus, ErrorResponse> {
    imp::reset_port(&app).await
}

#[tauri::command]
pub async fn list_paired_devices(app: AppHandle) -> Result<Vec<imp::Device>, ErrorResponse> {
    imp::list_devices(&app).await
}

#[tauri::command]
pub async fn create_pairing_code(app: AppHandle) -> Result<imp::Pairing, ErrorResponse> {
    imp::create_pairing_code(&app).await
}

#[tauri::command]
pub async fn remove_paired_device(app: AppHandle, id: String) -> Result<(), ErrorResponse> {
    imp::remove_device(&app, &id).await
}
