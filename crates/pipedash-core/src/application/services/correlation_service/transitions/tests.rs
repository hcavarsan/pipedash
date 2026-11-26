use chrono::{
    Duration,
    Utc,
};

use crate::application::services::correlation_service::tests::{
    unit,
    Fx,
};
use crate::domain::PipelineStatus;

#[tokio::test]
async fn observation_window_extends_inside_the_gap_and_splits_after_it() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let start = Utc::now();
    for seconds in [0, 100, 400] {
        fx.svc
            .pipelines_applied_at(gitops, &[], &[], start + Duration::seconds(seconds))
            .await
            .unwrap();
    }

    let mut conn = fx.svc.store.conn();
    let windows = conn
        .list_observation_windows_by_provider(gitops)
        .await
        .unwrap();
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].ended_at, start + Duration::seconds(100));
    assert_eq!(windows[1].started_at, start + Duration::seconds(400));
}

#[tokio::test]
async fn a_transition_is_recorded_only_when_something_changed() {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let api = unit(gitops, "api");
    fx.apply(gitops, vec![api.clone()]).await;
    fx.apply(gitops, vec![api.clone()]).await;
    let mut failing = api.clone();
    failing.status = PipelineStatus::Failed;
    fx.apply(gitops, vec![failing]).await;

    let mut conn = fx.svc.store.conn();
    let rows = conn
        .list_observed_transitions_by_pipeline(&api.id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].status_before, None);
    assert_eq!(rows[1].status_before.as_deref(), Some("success"));
    assert_eq!(rows[1].status_after, "failed");
}
