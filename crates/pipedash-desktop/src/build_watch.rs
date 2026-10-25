use std::{
    future::Future,
    sync::{
        Arc,
        Mutex,
        MutexGuard,
        PoisonError,
        Weak,
    },
    time::Duration,
};

use pipedash_core::{
    domain::{
        Pipeline,
        PipelineRun,
        PipelineStatus,
        ProviderConfig,
    },
    CoreContext,
};
use tauri::{
    AppHandle,
    Emitter,
    Manager,
    Runtime,
    State,
};
use tauri_plugin_pipedash_native::{
    BuildWatchMonitoring,
    BuildWatchRunStatus,
    BuildWatchSnapshot,
    BuildWatchStatus,
    BuildWatchStopReason,
    PipedashNativeExt,
};
use tokio::sync::watch;

use crate::{
    app_lock::AppLockState,
    commands::ErrorResponse,
    MaybeCoreContext,
};

const POLL_INTERVAL: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);
const NATIVE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct BuildWatchState(Arc<Mutex<Option<Session>>>);

struct Session {
    id: String,
    provider_id: Option<i64>,
    cancel: watch::Sender<bool>,
}

struct SessionGuard {
    state: Arc<Mutex<Option<Session>>>,
    id: String,
    cancelled: watch::Receiver<bool>,
}

impl BuildWatchState {
    fn inner(&self) -> MutexGuard<'_, Option<Session>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn reserve(&self) -> Result<SessionGuard, ErrorResponse> {
        let mut state = self.inner();
        if state.is_some() {
            return Err(error(
                "Stop the current build watch before starting another",
            ));
        }
        let id = format!("{:032x}", rand::random::<u128>());
        let (cancel, cancelled) = watch::channel(false);
        *state = Some(Session {
            id: id.clone(),
            provider_id: None,
            cancel,
        });
        Ok(SessionGuard {
            state: Arc::clone(&self.0),
            id,
            cancelled,
        })
    }

    fn session_id(&self) -> Option<String> {
        self.inner().as_ref().map(|session| session.id.clone())
    }

    fn provider_session_id(&self, provider_id: i64) -> Option<String> {
        self.inner()
            .as_ref()
            .filter(|session| session.provider_id == Some(provider_id))
            .map(|session| session.id.clone())
    }

    fn cancel(&self, matching: Option<&str>) -> Option<String> {
        let mut state = self.inner();
        if matching.is_some_and(|id| state.as_ref().is_none_or(|session| session.id != id)) {
            return None;
        }
        let session = state.take()?;
        drop(state);
        let _ = session.cancel.send(true);
        Some(session.id)
    }
}

impl SessionGuard {
    fn bind_provider(&self, provider_id: i64) -> Result<(), ErrorResponse> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let session = state
            .as_mut()
            .filter(|session| session.id == self.id)
            .ok_or_else(|| error("Build watching was interrupted"))?;
        if session
            .provider_id
            .is_some_and(|current| current != provider_id)
        {
            return Err(error("The watched provider changed"));
        }
        session.provider_id = Some(provider_id);
        Ok(())
    }

    fn is_current(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|session| session.id == self.id)
    }

    async fn run<T>(
        &self, future: impl Future<Output = Result<T, ErrorResponse>>,
    ) -> Result<T, ErrorResponse> {
        if !self.is_current() {
            return Err(error("Build watching was interrupted"));
        }
        let mut cancelled = self.cancelled.clone();
        tokio::select! {
            biased;
            _ = cancelled.changed() => Err(error("Build watching was interrupted")),
            result = tokio::time::timeout(REQUEST_TIMEOUT, future) => {
                result.map_err(|_| error("The build watch request timed out"))?
            }
        }
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        BuildWatchState(Arc::clone(&self.state)).cancel(Some(&self.id));
    }
}

fn error(message: &str) -> ErrorResponse {
    ErrorResponse {
        error: message.to_string(),
        details: None,
    }
}

async fn native_request<T>(
    future: impl Future<Output = tauri_plugin_pipedash_native::Result<T>>,
) -> Result<T, ErrorResponse> {
    tokio::time::timeout(NATIVE_TIMEOUT, future)
        .await
        .map_err(|_| error("The native build watch request timed out"))?
        .map_err(|failure| error(&failure.to_string()))
}

fn is_watching(status: &BuildWatchStatus, session_id: &str) -> bool {
    status.supported
        && status.available
        && status.watch.as_ref().is_some_and(|watch| {
            watch.session_id == session_id && watch.monitoring == BuildWatchMonitoring::Watching
        })
}

fn validate_selection(pipeline_id: &str, run_number: i64) -> Result<(), ErrorResponse> {
    if pipeline_id.trim().is_empty()
        || pipeline_id.len() > 2048
        || pipeline_id.chars().any(char::is_control)
        || !(1..=9_007_199_254_740_991).contains(&run_number)
    {
        return Err(error(
            "Choose a valid pipeline and a positive, safe run number",
        ));
    }
    Ok(())
}

fn snapshot(
    session_id: &str, pipeline: &Pipeline, provider: &ProviderConfig, run: PipelineRun,
    run_number: i64, updated_at: i64,
) -> Result<BuildWatchSnapshot, ErrorResponse> {
    if run.pipeline_id != pipeline.id
        || run.run_number != run_number
        || provider.id != Some(pipeline.provider_id)
        || provider.provider_type != pipeline.provider_type
    {
        return Err(error(
            "The provider returned a different pipeline or build run",
        ));
    }
    let status = match run.status {
        PipelineStatus::Pending => BuildWatchRunStatus::Pending,
        PipelineStatus::Running => BuildWatchRunStatus::Running,
        PipelineStatus::Success => BuildWatchRunStatus::Success,
        PipelineStatus::Failed => BuildWatchRunStatus::Failed,
        PipelineStatus::Cancelled => BuildWatchRunStatus::Cancelled,
        PipelineStatus::Skipped => BuildWatchRunStatus::Skipped,
    };
    let watch = BuildWatchSnapshot {
        session_id: session_id.to_string(),
        pipeline_id: pipeline.id.clone(),
        pipeline_name: pipeline.name.clone(),
        provider_name: provider.display_name().to_string(),
        provider_type: provider.provider_type.clone(),
        run_number,
        branch: run.branch,
        status,
        started_at: run.started_at.timestamp(),
        concluded_at: run.concluded_at.map(|time| time.timestamp()),
        updated_at,
        monitoring: if status.is_terminal() {
            BuildWatchMonitoring::Completed
        } else {
            BuildWatchMonitoring::Watching
        },
        url: BuildWatchSnapshot::run_url(&pipeline.id, run_number),
    };
    watch
        .validate()
        .map_err(|failure| error(&failure.to_string()))?;
    Ok(watch)
}

struct LoadedRun {
    watch: BuildWatchSnapshot,
    provider_id: i64,
    core: Weak<CoreContext>,
}

async fn available_core(app: &AppHandle) -> Result<Arc<CoreContext>, ErrorResponse> {
    if app.state::<AppLockState>().locked() {
        return Err(error("Unlock Pipedash before watching a build"));
    }
    app.state::<MaybeCoreContext>()
        .get()
        .await
        .map_err(|message| error(&message))
}

async fn load_run(
    app: &AppHandle, session: &SessionGuard, pipeline_id: &str, run_number: i64,
    previous: Option<&LoadedRun>,
) -> Result<LoadedRun, ErrorResponse> {
    let core = available_core(app).await?;
    if previous.is_some_and(|previous| !Weak::ptr_eq(&previous.core, &Arc::downgrade(&core))) {
        return Err(error(
            "The application session changed; start watching again",
        ));
    }
    let pipelines = core.pipeline_service.get_cached_pipelines(None).await?;
    let pipeline = pipelines
        .into_iter()
        .find(|pipeline| pipeline.id == pipeline_id)
        .ok_or_else(|| error("The watched pipeline is no longer available"))?;
    if previous.is_some_and(|previous| previous.provider_id != pipeline.provider_id) {
        return Err(error("The watched provider changed"));
    }
    session.bind_provider(pipeline.provider_id)?;
    let run = core
        .pipeline_service
        .fetch_run_details(pipeline_id, run_number)
        .await?;
    let provider = core
        .provider_service
        .get_provider_config(pipeline.provider_id)
        .await?;
    if !Arc::ptr_eq(&core, &available_core(app).await?) {
        return Err(error(
            "The application session changed; start watching again",
        ));
    }
    Ok(LoadedRun {
        watch: snapshot(
            &session.id,
            &pipeline,
            &provider,
            run,
            run_number,
            chrono::Utc::now().timestamp(),
        )?,
        provider_id: pipeline.provider_id,
        core: Arc::downgrade(&core),
    })
}

async fn stop_native<R: Runtime>(
    app: &AppHandle<R>, session_id: &str, reason: BuildWatchStopReason,
) -> Result<BuildWatchStatus, ErrorResponse> {
    native_request(app.pipedash_native().stop_build_watch(session_id, reason)).await
}

async fn stop_current(
    app: &AppHandle, reason: BuildWatchStopReason,
) -> Result<BuildWatchStatus, ErrorResponse> {
    let session_id = app.state::<BuildWatchState>().cancel(None);
    if let Some(session_id) = session_id {
        return stop_native(app, &session_id, reason).await;
    }
    stop_orphan(app, reason).await
}

async fn stop_orphan<R: Runtime>(
    app: &AppHandle<R>, reason: BuildWatchStopReason,
) -> Result<BuildWatchStatus, ErrorResponse> {
    let status = native_request(app.pipedash_native().build_watch_status()).await?;
    if let Some(watch) = &status.watch {
        if watch.monitoring == BuildWatchMonitoring::Watching
            && app.state::<BuildWatchState>().session_id().as_deref()
                != Some(watch.session_id.as_str())
        {
            return stop_native(app, &watch.session_id, reason).await;
        }
    }
    Ok(status)
}

pub fn interrupt<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<BuildWatchState>() else {
        return;
    };
    spawn_interruption(app, state.cancel(None));
}

pub fn interrupt_provider<R: Runtime>(app: &AppHandle<R>, provider_id: i64) {
    let Some(state) = app.try_state::<BuildWatchState>() else {
        return;
    };
    if let Some(session_id) = state.provider_session_id(provider_id) {
        if let Some(session_id) = state.cancel(Some(&session_id)) {
            spawn_interruption(app, Some(session_id));
        }
    }
}

fn spawn_interruption<R: Runtime>(app: &AppHandle<R>, session_id: Option<String>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = if let Some(session_id) = session_id {
            stop_native(&app, &session_id, BuildWatchStopReason::Interrupted).await
        } else {
            stop_orphan(&app, BuildWatchStopReason::Interrupted).await
        };
        if result.is_err() {
            tracing::warn!("Could not interrupt the native build watch");
        }
    });
}

pub fn register<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let handle = app.clone();
    app.pipedash_native()
        .on_build_watch_changed(move || {
            if let Err(error) = handle.emit("build-watch-changed", ()) {
                tracing::warn!(%error, "Could not publish the native build watch status change");
            }
        })
        .map_err(|error| tauri::Error::Anyhow(error.into()))
}

#[tauri::command]
pub async fn get_build_watch(app: AppHandle) -> Result<BuildWatchStatus, ErrorResponse> {
    let mut status = native_request(app.pipedash_native().build_watch_status()).await?;
    if !status.supported {
        return Ok(status);
    }
    let unavailable = if app.state::<AppLockState>().locked() {
        Some("Unlock Pipedash before watching a build")
    } else if app.state::<MaybeCoreContext>().0.read().await.is_none() {
        Some("Initialize Pipedash before watching a build")
    } else {
        None
    };
    if let Some(reason) = unavailable {
        status = stop_current(&app, BuildWatchStopReason::Interrupted).await?;
        status.available = false;
        status.reason = Some(reason.to_string());
    } else if let Some(watch) = &status.watch {
        let state = app.state::<BuildWatchState>();
        if watch.monitoring == BuildWatchMonitoring::Watching {
            if state.session_id().is_none() {
                status =
                    stop_native(&app, &watch.session_id, BuildWatchStopReason::Interrupted).await?;
            }
        } else {
            state.cancel(Some(&watch.session_id));
        }
    }
    Ok(status)
}

#[tauri::command]
pub async fn start_build_watch(
    app: AppHandle, state: State<'_, BuildWatchState>, pipeline_id: String, run_number: i64,
) -> Result<BuildWatchStatus, ErrorResponse> {
    validate_selection(&pipeline_id, run_number)?;
    let session = state.reserve()?;
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
    let selected = session
        .run(load_run(&app, &session, &pipeline_id, run_number, None))
        .await?;
    if selected.watch.status.is_terminal() {
        return Err(error("This build has already finished"));
    }
    let started = session
        .run(native_request(
            app.pipedash_native().start_build_watch(&selected.watch),
        ))
        .await;
    let status = match started {
        Ok(status) if session.is_current() && is_watching(&status, &session.id) => status,
        result => {
            let stopped = stop_native(&app, &session.id, BuildWatchStopReason::Interrupted).await;
            return match result {
                Err(failure) => Err(failure),
                Ok(_) => stopped,
            };
        }
    };
    tauri::async_runtime::spawn(poll(app, session, selected));
    Ok(status)
}

#[tauri::command]
pub async fn stop_build_watch(app: AppHandle) -> Result<BuildWatchStatus, ErrorResponse> {
    stop_current(&app, BuildWatchStopReason::Stopped).await
}

async fn poll(app: AppHandle, session: SessionGuard, selected: LoadedRun) {
    let result = poll_selected(&app, &session, &selected).await;
    if result.is_err() && session.is_current() {
        let _ = stop_native(&app, &session.id, BuildWatchStopReason::Interrupted).await;
        tracing::warn!("Build watching was interrupted before a terminal result");
    }
}

async fn poll_selected(
    app: &AppHandle, session: &SessionGuard, selected: &LoadedRun,
) -> Result<(), ErrorResponse> {
    loop {
        session
            .run(async {
                tokio::time::sleep(POLL_INTERVAL).await;
                Ok(())
            })
            .await?;
        let status = session
            .run(native_request(app.pipedash_native().build_watch_status()))
            .await?;
        if !status.available {
            return Err(error("Build watching is unavailable"));
        }
        if !is_watching(&status, &session.id) {
            return Ok(());
        }
        let mut loaded = session
            .run(load_run(
                app,
                session,
                &selected.watch.pipeline_id,
                selected.watch.run_number,
                Some(selected),
            ))
            .await?;
        let status = session
            .run(native_request(app.pipedash_native().build_watch_status()))
            .await?;
        if !status.available {
            return Err(error("Build watching is unavailable"));
        }
        if !is_watching(&status, &session.id) {
            return Ok(());
        }
        if !Weak::ptr_eq(&loaded.core, &Arc::downgrade(&available_core(app).await?)) {
            return Err(error(
                "The application session changed; start watching again",
            ));
        }
        if let Some(current) = &status.watch {
            loaded.watch.updated_at = loaded.watch.updated_at.max(current.updated_at);
        }
        let status = session
            .run(native_request(
                app.pipedash_native().update_build_watch(&loaded.watch),
            ))
            .await?;
        if !is_watching(&status, &session.id) {
            return Ok(());
        }
        if loaded.watch.status.is_terminal() {
            return Err(error(
                "The native activity did not accept the completed build result",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{
        AtomicBool,
        Ordering,
    };

    use chrono::DateTime;

    use super::*;

    fn run(status: PipelineStatus) -> PipelineRun {
        PipelineRun {
            id: "provider-run-id".to_string(),
            pipeline_id: "github__1__owner/repo/build.yml".to_string(),
            run_number: 7,
            status,
            started_at: DateTime::from_timestamp(100, 0).unwrap(),
            concluded_at: None,
            duration_seconds: None,
            logs_url: "https://example.com/private?token=secret".to_string(),
            commit_sha: None,
            commit_message: None,
            branch: Some("main".to_string()),
            actor: None,
            inputs: None,
            metadata: HashMap::new(),
        }
    }

    fn trusted_snapshot(run: PipelineRun) -> Result<BuildWatchSnapshot, ErrorResponse> {
        let pipeline = Pipeline {
            id: "github__1__owner/repo/build.yml".to_string(),
            provider_id: 1,
            provider_type: "github".to_string(),
            name: "Build".to_string(),
            status: PipelineStatus::Success,
            last_run: None,
            last_updated: DateTime::from_timestamp(100, 0).unwrap(),
            repository: "owner/repo".to_string(),
            branch: None,
            workflow_file: None,
            metadata: HashMap::new(),
            overdue_run: None,
        };
        let provider = ProviderConfig {
            id: Some(1),
            name: "My GitHub".to_string(),
            provider_type: "github".to_string(),
            token: "secret".to_string(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: None,
        };
        snapshot("session", &pipeline, &provider, run, 7, 110)
    }

    #[test]
    fn only_one_session_can_be_reserved_including_during_startup() {
        let state = BuildWatchState::default();
        let session = state.reserve().unwrap();
        assert!(state.reserve().is_err());
        drop(session);
        assert!(state.reserve().is_ok());
    }

    #[test]
    fn an_old_session_cannot_cancel_its_replacement() {
        let state = BuildWatchState::default();
        let old = state.reserve().unwrap();
        state.cancel(None);
        let current = state.reserve().unwrap();
        assert!(!old.is_current());
        drop(old);
        assert!(current.is_current());
    }

    #[test]
    fn removing_an_unrelated_provider_does_not_interrupt_the_watch() {
        let state = BuildWatchState::default();
        let session = state.reserve().unwrap();
        session.bind_provider(1).unwrap();
        assert!(state.provider_session_id(2).is_none());
        assert!(session.is_current());
        let removed = state.provider_session_id(1).unwrap();
        state.cancel(Some(&removed));
        assert!(!session.is_current());
    }

    #[test]
    fn a_stale_provider_removal_cannot_cancel_a_new_watch() {
        let state = BuildWatchState::default();
        let old = state.reserve().unwrap();
        old.bind_provider(1).unwrap();
        let removed = state.provider_session_id(1).unwrap();
        state.cancel(None);
        let current = state.reserve().unwrap();
        current.bind_provider(2).unwrap();
        state.cancel(Some(&removed));
        assert!(current.is_current());
        assert!(old.bind_provider(1).is_err());
        assert!(current.bind_provider(1).is_err());
    }

    #[test]
    fn native_activity_links_open_the_existing_selected_run_route() {
        let watch = trusted_snapshot(run(PipelineStatus::Running)).unwrap();
        let intent = crate::native::parse_deep_link(&tauri::Url::parse(&watch.url).unwrap());
        assert_eq!(
            intent,
            Some(crate::native::NativeIntent::Link {
                path: "/pipelines/github__1__owner%2Frepo%2Fbuild.yml".to_string(),
                run_number: Some(7),
            })
        );
    }

    #[tokio::test]
    async fn cancellation_drops_an_in_flight_provider_request() {
        struct OnDrop(Arc<AtomicBool>);
        impl Drop for OnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let state = BuildWatchState::default();
        let session = state.reserve().unwrap();
        let dropped = Arc::new(AtomicBool::new(false));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let task_dropped = Arc::clone(&dropped);
        let task = tokio::spawn(async move {
            session
                .run(async {
                    let _drop = OnDrop(task_dropped);
                    let _ = started_tx.send(());
                    std::future::pending::<Result<(), ErrorResponse>>().await
                })
                .await
        });
        started_rx.await.unwrap();
        state.cancel(None);
        assert!(task.await.unwrap().is_err());
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn a_cancelled_reservation_cannot_start_a_request() {
        let state = BuildWatchState::default();
        let session = state.reserve().unwrap();
        state.cancel(None);
        let called = AtomicBool::new(false);
        let result = session
            .run(async {
                called.store(true, Ordering::SeqCst);
                Ok(())
            })
            .await;
        assert!(result.is_err());
        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn selected_run_validation_rejects_unsafe_numbers_and_empty_identity() {
        for run in [i64::MIN, -1, 0, 9_007_199_254_740_992, i64::MAX] {
            assert!(validate_selection("pipeline", run).is_err());
        }
        assert!(validate_selection("", 1).is_err());
        assert!(validate_selection("pipeline\n", 1).is_err());
        assert!(validate_selection("pipeline", 9_007_199_254_740_991).is_ok());
    }

    #[test]
    fn a_provider_response_must_match_the_selected_run() {
        let mut wrong_pipeline = run(PipelineStatus::Running);
        wrong_pipeline.pipeline_id = "another-pipeline".to_string();
        assert!(trusted_snapshot(wrong_pipeline).is_err());
        let mut wrong_run = run(PipelineStatus::Running);
        wrong_run.run_number = 8;
        assert!(trusted_snapshot(wrong_run).is_err());
    }

    #[test]
    fn snapshots_only_include_selected_provider_data_and_never_credentials() {
        let snapshot = trusted_snapshot(run(PipelineStatus::Running)).unwrap();
        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(snapshot.pipeline_name, "Build");
        assert_eq!(snapshot.provider_name, "My GitHub");
        assert_eq!(snapshot.status, BuildWatchRunStatus::Running);
        assert_eq!(snapshot.branch.as_deref(), Some("main"));
        assert!(!json.contains("secret"));
        assert!(!json.contains("logs"));
    }

    #[test]
    fn every_terminal_provider_outcome_ends_monitoring_without_fabricating_a_finish_time() {
        for outcome in [
            PipelineStatus::Success,
            PipelineStatus::Failed,
            PipelineStatus::Cancelled,
            PipelineStatus::Skipped,
        ] {
            let snapshot = trusted_snapshot(run(outcome)).unwrap();
            assert_eq!(snapshot.monitoring, BuildWatchMonitoring::Completed);
            assert_eq!(snapshot.concluded_at, None);
        }
    }

    #[test]
    fn a_terminal_snapshot_keeps_the_providers_real_finish_time() {
        let mut completed = run(PipelineStatus::Success);
        completed.concluded_at = DateTime::from_timestamp(105, 0);
        let snapshot = trusted_snapshot(completed).unwrap();
        assert_eq!(snapshot.concluded_at, Some(105));
    }

    #[test]
    fn native_expiration_and_a_different_session_stop_polling() {
        let snapshot = trusted_snapshot(run(PipelineStatus::Running)).unwrap();
        let mut status = BuildWatchStatus {
            supported: true,
            available: true,
            reason: None,
            watch: Some(snapshot),
        };
        assert!(is_watching(&status, "session"));
        assert!(!is_watching(&status, "other-session"));
        status.watch.as_mut().unwrap().monitoring = BuildWatchMonitoring::Interrupted;
        assert!(!is_watching(&status, "session"));
        status.watch.as_mut().unwrap().monitoring = BuildWatchMonitoring::Watching;
        status.available = false;
        assert!(!is_watching(&status, "session"));
    }
}
