use std::sync::Arc;

use crate::domain::{
    AlertKind,
    DomainError,
    PipelineNotifyState,
    PreferenceChange,
};
use crate::tests::context;
use crate::MemoryTokenStore;

#[tokio::test]
async fn a_change_is_saved_for_the_device_and_returned_with_pipeline_states() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path(), Arc::new(MemoryTokenStore::new())).await;
    let (device, _) = core.pair_device("Pixel", "ios").await.unwrap();
    let service = core.notification_preferences(&device.id);

    let snapshot = service
        .change(PreferenceChange::PipelineWatched {
            pipeline_id: "stub__1__api".into(),
            watched: true,
        })
        .await
        .unwrap();

    assert_eq!(snapshot.preferences.watched_pipelines, ["stub__1__api"]);
    assert_eq!(
        snapshot.pipeline_states["stub__1__api"],
        PipelineNotifyState::Watching
    );
    assert_eq!(service.snapshot().await.unwrap(), snapshot);
    assert_eq!(
        core.devices()
            .notification_preferences()
            .await
            .unwrap()
            .remove(&device.id)
            .unwrap(),
        snapshot.preferences
    );
}

#[tokio::test]
async fn a_rejected_change_leaves_the_stored_preferences_alone() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path(), Arc::new(MemoryTokenStore::new())).await;
    let (device, _) = core.pair_device("Pixel", "ios").await.unwrap();
    let service = core.notification_preferences(&device.id);

    let rejected = service
        .change(PreferenceChange::Kind {
            kind: AlertKind::Finished,
            enabled: false,
        })
        .await;

    assert!(matches!(rejected, Err(DomainError::InvalidConfig(_))));
    assert!(core
        .devices()
        .notification_preferences()
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn an_unknown_device_has_no_preferences() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path(), Arc::new(MemoryTokenStore::new())).await;
    let service = core.notification_preferences("gone");

    assert!(matches!(
        service.snapshot().await,
        Err(DomainError::NotFound(_))
    ));
    assert!(matches!(
        service
            .change(PreferenceChange::CancelNextFinish {
                pipeline_id: "p".into(),
            })
            .await,
        Err(DomainError::NotFound(_))
    ));
}
