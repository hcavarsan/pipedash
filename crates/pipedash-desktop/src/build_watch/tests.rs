use std::collections::HashMap;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::Arc;
use std::time::Duration;

use chrono::DateTime;
use pipedash_core::application::{
    watch_snapshot,
    WatchedRun,
};
use pipedash_core::domain::{
    Pipeline,
    PipelineRun,
    PipelineStatus,
};
use pipedash_core::DomainError;
use pipedash_plugin_api::build_watch::{
    BuildWatchMonitoring,
    BuildWatchRunStatus,
    BuildWatchSnapshot,
    BuildWatchStatus,
};

#[cfg(desktop)]
use super::native::unavailable_status;
use super::session::BuildWatchState;
use super::*;

fn run(status: PipelineStatus) -> PipelineRun {
    PipelineRun {
        id: "provider-run-id".to_string(),
        pipeline_id: "stub__1__owner/repo/build.yml".to_string(),
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
        actions: Default::default(),
        display: Default::default(),
        pace: Default::default(),
    }
}

fn watched_run(run: PipelineRun) -> WatchedRun {
    let pipeline = Pipeline {
        id: "stub__1__owner/repo/build.yml".to_string(),
        provider_id: 1,
        provider_type: "stub".to_string(),
        name: "Build".to_string(),
        status: PipelineStatus::Success,
        last_run: None,
        last_updated: DateTime::from_timestamp(100, 0).unwrap(),
        repository: "owner/repo".to_string(),
        branch: None,
        workflow_file: None,
        resource_state: None,
        metadata: HashMap::new(),
        overdue_run: None,
        typical_duration_seconds: None,
        actions: Default::default(),
    };
    WatchedRun {
        pipeline,
        provider_name: "My Stub".to_string(),
        run,
        observed_at: DateTime::from_timestamp(110, 0).unwrap(),
    }
}

fn trusted_snapshot(run: PipelineRun) -> Result<BuildWatchSnapshot, DomainError> {
    watch_snapshot(
        "session",
        "stub__1__owner/repo/build.yml",
        7,
        watched_run(run),
    )
}

#[cfg(desktop)]
#[test]
fn unavailable_desktop_status_never_exposes_a_retained_watch() {
    for monitoring in [
        BuildWatchMonitoring::Watching,
        BuildWatchMonitoring::Completed,
        BuildWatchMonitoring::Stopped,
        BuildWatchMonitoring::Interrupted,
    ] {
        let mut watch = trusted_snapshot(run(PipelineStatus::Running)).unwrap();
        watch.monitoring = monitoring;
        if monitoring == BuildWatchMonitoring::Completed {
            watch.status = BuildWatchRunStatus::Success;
        }
        let status = unavailable_status(
            BuildWatchStatus {
                supported: true,
                available: true,
                reason: None,
                watch: Some(watch),
            },
            "Unlock Pipedash before watching a build",
        );
        assert!(status.supported);
        assert!(!status.available);
        assert_eq!(status.watch, None);
        assert_eq!(
            status.reason.as_deref(),
            Some("Unlock Pipedash before watching a build")
        );
    }
}

#[cfg(desktop)]
#[test]
fn provider_removal_only_cancels_its_reserved_core_generation() {
    let state = BuildWatchState::default();
    let old = state.reserve().unwrap();
    old.bind_generation(1).unwrap();
    old.bind_provider(7).unwrap();
    assert_eq!(state.provider_session_id_generation(2, 7), None);
    assert_eq!(state.provider_session_id_generation(1, 8), None);
    let removed = state.provider_session_id_generation(1, 7).unwrap();
    state.cancel(Some(&removed));
    let current = state.reserve().unwrap();
    current.bind_generation(2).unwrap();
    current.bind_provider(7).unwrap();
    assert_eq!(state.provider_session_id_generation(1, 7), None);
    assert_eq!(
        state.provider_session_id_generation(2, 7),
        Some(current.id.clone())
    );
    assert_eq!(state.cancel(Some(&removed)), None);
    assert!(current.is_current());
    assert!(old.bind_generation(3).is_err());
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
            path: "/pipelines/stub__1__owner%2Frepo%2Fbuild.yml".to_string(),
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

#[tokio::test]
async fn matching_native_end_notifications_cancel_a_blocked_core_operation() {
    for monitoring in [
        BuildWatchMonitoring::Interrupted,
        BuildWatchMonitoring::Stopped,
        BuildWatchMonitoring::Completed,
        BuildWatchMonitoring::Watching,
    ] {
        let state = BuildWatchState::default();
        let session = state.reserve().unwrap();
        let mut snapshot = trusted_snapshot(run(PipelineStatus::Running)).unwrap();
        snapshot.session_id = session.id.clone();
        snapshot.monitoring = monitoring;
        if monitoring == BuildWatchMonitoring::Completed {
            snapshot.status = BuildWatchRunStatus::Success;
        }
        let status = BuildWatchStatus {
            supported: true,
            available: monitoring != BuildWatchMonitoring::Watching,
            reason: None,
            watch: Some(snapshot),
        };
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            session
                .run(async {
                    let _ = started_tx.send(());
                    std::future::pending::<Result<(), ErrorResponse>>().await
                })
                .await
        });
        started_rx.await.unwrap();
        state.native_changed(&status);
        assert!(tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .is_err());
    }
}

#[test]
fn native_end_notifications_cannot_cancel_a_replacement_session() {
    let state = BuildWatchState::default();
    let old = state.reserve().unwrap();
    let mut snapshot = trusted_snapshot(run(PipelineStatus::Running)).unwrap();
    snapshot.session_id = old.id.clone();
    snapshot.monitoring = BuildWatchMonitoring::Interrupted;
    state.cancel(None);
    let current = state.reserve().unwrap();
    let mut status = BuildWatchStatus {
        supported: true,
        available: true,
        reason: None,
        watch: Some(snapshot),
    };
    state.native_changed(&status);
    assert!(current.is_current());
    status.watch.as_mut().unwrap().session_id = current.id.clone();
    status.watch.as_mut().unwrap().monitoring = BuildWatchMonitoring::Watching;
    state.native_changed(&status);
    assert!(current.is_current());
    status.watch = None;
    state.native_changed(&status);
    assert!(current.is_current());
}
