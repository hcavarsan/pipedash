use std::collections::HashMap;

use chrono::DateTime;
use pipedash_core::{
    application::NotificationTone,
    domain::{
        DomainError,
        Pipeline,
        PipelineStatus,
    },
};

use super::*;

fn pipeline(status: PipelineStatus) -> Pipeline {
    let at = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    Pipeline {
        id: "jenkins__1__ci".to_string(),
        provider_id: 1,
        provider_type: "jenkins".to_string(),
        name: "ci".to_string(),
        status,
        last_run: Some(at),
        last_updated: at,
        repository: "project".to_string(),
        branch: None,
        workflow_file: None,
        resource_state: None,
        metadata: HashMap::new(),
        overdue_run: None,
        typical_duration_seconds: None,
    }
}

#[test]
fn menu_read_failure_keeps_tracking_alerts_and_recovery_does_not_replay_them() {
    let mut baseline = NotificationState::default();
    baseline.seed(&[pipeline(PipelineStatus::Running)], &[]);
    let mut inner = Inner {
        baseline: Some(baseline),
        ..Default::default()
    };
    inner.apply_menu_result(Err(DomainError::DatabaseError(
        "Pinned view unavailable".to_string(),
    )));
    assert!(inner.snapshot.is_none());
    assert!(inner.data_error.is_some());

    let failed = CoreEvent::PipelinesFailed {
        pipelines: vec![pipeline(PipelineStatus::Failed)],
    };
    let notices = inner.baseline.as_mut().unwrap().observe_event(&failed);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].pipeline_id.as_deref(), Some("jenkins__1__ci"));
    assert_eq!(notices[0].tone, NotificationTone::Danger);

    inner.apply_menu_result(Ok(PipelineMenuSnapshot {
        pipelines: Vec::new(),
        shortcuts: Vec::new(),
    }));
    assert!(inner.data_error.is_none());
    assert!(inner
        .baseline
        .as_mut()
        .unwrap()
        .observe_event(&failed)
        .is_empty());
}
