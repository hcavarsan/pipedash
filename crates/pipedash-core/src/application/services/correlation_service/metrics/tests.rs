use std::sync::Arc;

use chrono::{
    DateTime,
    TimeZone,
    Utc,
};

use super::super::{
    copy,
    CorrelationDeps,
    CorrelationService,
};
use crate::application::test_support::Harness;
use crate::domain::application::metrics::{
    ApplicationMetrics,
    ApplicationMetricsQuery,
    MetricId,
    MetricResult,
};
use crate::domain::metrics::AggregationPeriod;
use crate::infrastructure::database::{
    ApplicationRow,
    ApplicationTargetRow,
    ArtifactObservationRow,
    ChangeRow,
    ChangeStageRow,
    ObservationWindowRow,
    ObservedTransitionRow,
};
use crate::infrastructure::HttpClientManager;

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 10, hour, minute, 0).unwrap()
}

struct Fixture {
    _harness: Harness,
    service: Arc<CorrelationService>,
    application: i64,
    provider: i64,
}

impl Fixture {
    async fn new() -> Self {
        let harness = Harness::new().await;
        let service = CorrelationService::new(CorrelationDeps {
            repository: harness.repository.clone(),
            provider_service: harness.provider_service.clone(),
            pipeline_service: harness.pipeline_service.clone(),
            log_service: harness.log_service.clone(),
            config_state: harness.config_state.clone(),
            event_bus: harness.bus.clone(),
            presenter: harness.presenter.clone(),
            http: Arc::new(HttpClientManager::new().unwrap()),
            event_interval: super::super::EVENT_INTERVAL,
        });
        let provider: i64 = sqlx::query_scalar(
            "INSERT INTO providers (name, provider_type, token_encrypted) \
             VALUES ('ci', 'stub', 'secret') RETURNING id",
        )
        .fetch_one(&harness.pool)
        .await
        .unwrap();
        let mut conn = service.store.conn();
        let application = conn
            .upsert_application(&ApplicationRow {
                id: 0,
                identity_kind: "unit".into(),
                identity_key: format!("{provider}:api"),
                name: "api".into(),
                hidden: false,
                archived_at: None,
                created_at: at(0, 0),
            })
            .await
            .unwrap();
        let targets: Vec<ApplicationTargetRow> = [("prod", false), ("qa", false), ("pr", true)]
            .into_iter()
            .map(|(name, short_lived)| ApplicationTargetRow {
                application_id: application,
                target_key: format!("cluster|{name}"),
                name: name.into(),
                group_key: name.into(),
                lifetime: if short_lived {
                    "short_lived"
                } else {
                    "standing"
                }
                .into(),
            })
            .collect();
        conn.replace_application_targets(application, &targets)
            .await
            .unwrap();
        Self {
            _harness: harness,
            service,
            application,
            provider,
        }
    }

    fn deploy_pipeline(&self) -> String {
        format!("stub__{}__api-prod", self.provider)
    }

    async fn window(&self, from: DateTime<Utc>, to: DateTime<Utc>) {
        self.service
            .store
            .conn()
            .upsert_observation_window(&ObservationWindowRow {
                provider_id: self.provider,
                started_at: from,
                ended_at: to,
            })
            .await
            .unwrap();
    }

    async fn change(
        &self, commit: &str, authored: Option<DateTime<Utc>>, rows: Vec<ChangeStageRow>,
    ) -> i64 {
        let mut conn = self.service.store.conn();
        let id = conn
            .upsert_change(&ChangeRow {
                id: 0,
                application_id: self.application,
                repo: "git.test/shop/api".into(),
                commit_sha: commit.repeat(40),
                authored_at: authored,
            })
            .await
            .unwrap();
        conn.replace_change_stages(id, &rows).await.unwrap();
        id
    }

    async fn consumed(&self, run_number: i64, digest: &str) {
        let row = ArtifactObservationRow {
            provider_id: self.provider,
            subject_id: self.deploy_pipeline(),
            run_number,
            relation: "consumed".into(),
            ordinal: 0,
            repository: "registry.test/shop/api".into(),
            digest: Some(format!("sha256:{}", digest.repeat(64))),
            tag: None,
            target_ordinal: None,
            origin: "provider".into(),
        };
        self.service
            .store
            .conn()
            .replace_artifact_observations(&self.deploy_pipeline(), run_number, "provider", &[row])
            .await
            .unwrap();
    }

    async fn transition(&self, when: DateTime<Utc>, before: &str, after: &str, revision: &str) {
        self.service
            .store
            .conn()
            .insert_observed_transition(&ObservedTransitionRow {
                id: 0,
                pipeline_id: self.deploy_pipeline(),
                provider_id: self.provider,
                at: when,
                status_before: Some(before.into()),
                status_after: after.into(),
                health_before: None,
                health_after: None,
                revision: Some(revision.repeat(40)),
                digests_json: "[]".into(),
            })
            .await
            .unwrap();
    }

    fn build(&self, run_number: i64, start: DateTime<Utc>, end: DateTime<Utc>) -> ChangeStageRow {
        stage(
            "build",
            "",
            format!("stub__{}__build", self.provider),
            run_number,
            (start, Some(end)),
            "success",
            (true, true, true),
        )
    }

    fn deploy(
        &self, group: &str, run_number: i64, span: (DateTime<Utc>, Option<DateTime<Utc>>),
        status: &str, flags: (bool, bool, bool),
    ) -> ChangeStageRow {
        stage(
            "deploy",
            group,
            self.deploy_pipeline(),
            run_number,
            span,
            status,
            flags,
        )
    }

    fn success_only(
        &self, group: &str, run_number: i64, deployed: DateTime<Utc>,
    ) -> ChangeStageRow {
        self.deploy(
            group,
            run_number,
            (deployed, Some(deployed)),
            "success",
            (false, false, false),
        )
    }

    async fn metrics(&self) -> ApplicationMetrics {
        self.service
            .metrics(ApplicationMetricsQuery {
                application_id: self.application,
                group_key: None,
                from: at(0, 0),
                to: at(23, 59),
                period: AggregationPeriod::Daily,
            })
            .await
            .unwrap()
    }
}

fn stage(
    kind: &str, group: &str, pipeline_id: String, run_number: i64,
    span: (DateTime<Utc>, Option<DateTime<Utc>>), status: &str, flags: (bool, bool, bool),
) -> ChangeStageRow {
    ChangeStageRow {
        change_id: 0,
        stage: kind.into(),
        group_key: group.into(),
        pipeline_id,
        run_number,
        started_at: span.0,
        concluded_at: span.1,
        status: status.into(),
        outcome_recorded: flags.0,
        timing_recorded: flags.1,
        attempt: flags.2,
    }
}

fn metric(metrics: &ApplicationMetrics, id: MetricId) -> &MetricResult {
    metrics.metrics.iter().find(|m| m.id == id).unwrap()
}

#[tokio::test]
async fn wait_is_five_minutes_deploy_is_ten() {
    let f = Fixture::new().await;
    f.change(
        "a",
        None,
        vec![
            f.build(1, at(9, 50), at(10, 0)),
            f.deploy(
                "prod",
                1,
                (at(10, 5), Some(at(10, 15))),
                "success",
                (true, true, true),
            ),
        ],
    )
    .await;
    let metrics = f.metrics().await;
    assert_eq!(metric(&metrics, MetricId::WaitTime).value, Some(300.0));
    assert_eq!(metric(&metrics, MetricId::DeployTime).value, Some(600.0));
    assert_eq!(metric(&metrics, MetricId::BuildTime).value, Some(600.0));
    assert_eq!(
        metric(&metrics, MetricId::BuildToDeployed).value,
        Some(900.0)
    );
    assert_eq!(metric(&metrics, MetricId::DeliveryTime).value, Some(1500.0));
}

#[tokio::test]
async fn deployed_time_only_makes_wait_unavailable() {
    let f = Fixture::new().await;
    f.change(
        "a",
        None,
        vec![
            f.build(1, at(9, 50), at(10, 0)),
            f.success_only("prod", 1, at(10, 15)),
        ],
    )
    .await;
    let metrics = f.metrics().await;
    let wait = metric(&metrics, MetricId::WaitTime);
    assert_eq!(wait.value, None);
    assert_eq!(wait.unavailable_reason, Some(copy::no_recorded_start()));
    assert_eq!(
        metric(&metrics, MetricId::BuildToDeployed).value,
        Some(900.0)
    );
}

#[tokio::test]
async fn success_only_history_makes_deploy_metrics_unavailable() {
    let f = Fixture::new().await;
    f.window(at(9, 0), at(12, 0)).await;
    f.change("a", None, vec![f.success_only("prod", 1, at(10, 15))])
        .await;
    f.change("b", None, vec![f.success_only("prod", 2, at(11, 0))])
        .await;
    let metrics = f.metrics().await;
    for id in [
        MetricId::DeployTime,
        MetricId::DeployFailures,
        MetricId::RecoveryTime,
    ] {
        let result = metric(&metrics, id);
        assert_eq!(result.value, None, "{id:?}");
        assert!(result.unavailable_reason.is_some(), "{id:?}");
    }
    assert_eq!(
        metric(&metrics, MetricId::DeployTime).unavailable_reason,
        Some(copy::no_attempt_records())
    );
    assert_eq!(metric(&metrics, MetricId::DeployFrequency).value, Some(2.0));
}

#[tokio::test]
async fn short_lived_target_excluded_from_frequency() {
    let f = Fixture::new().await;
    f.change(
        "a",
        None,
        vec![
            f.success_only("prod", 1, at(10, 0)),
            f.success_only("pr", 2, at(10, 5)),
        ],
    )
    .await;
    let metrics = f.metrics().await;
    assert_eq!(metric(&metrics, MetricId::DeployFrequency).value, Some(1.0));
    assert!(metrics.groups.iter().all(|group| group.key != "pr"));
}

#[tokio::test]
async fn failure_rate_counts_only_inside_windows() {
    let f = Fixture::new().await;
    f.window(at(9, 0), at(12, 0)).await;
    let flags = (true, true, true);
    f.change(
        "a",
        None,
        vec![f.deploy("prod", 1, (at(10, 0), Some(at(10, 10))), "failed", flags)],
    )
    .await;
    f.change(
        "b",
        None,
        vec![f.deploy("prod", 2, (at(11, 0), Some(at(11, 10))), "success", flags)],
    )
    .await;
    f.change(
        "c",
        None,
        vec![f.deploy("prod", 3, (at(14, 0), Some(at(14, 10))), "failed", flags)],
    )
    .await;
    let metrics = f.metrics().await;
    let rate = metric(&metrics, MetricId::ChangeFailureRate);
    assert_eq!(rate.value, Some(50.0));
    assert_eq!(rate.coverage.counted, 2);
    assert_eq!(rate.coverage.excluded, 1);
    assert_eq!(rate.coverage.since, Some(at(9, 0)));
}

#[tokio::test]
async fn rollback_marks_change_failed() {
    let f = Fixture::new().await;
    f.window(at(9, 0), at(15, 0)).await;
    f.change(
        "a",
        None,
        vec![
            f.success_only("prod", 1, at(10, 0)),
            f.success_only("prod", 3, at(12, 0)),
        ],
    )
    .await;
    f.change("b", None, vec![f.success_only("prod", 2, at(11, 0))])
        .await;
    f.consumed(1, "a").await;
    f.consumed(2, "b").await;
    f.consumed(3, "a").await;
    let metrics = f.metrics().await;
    let rate = metric(&metrics, MetricId::ChangeFailureRate);
    assert_eq!(rate.value, Some(50.0));
    assert_eq!(rate.coverage.counted, 2);
}

#[tokio::test]
async fn degraded_after_deploy_inside_an_hour_marks_change_failed() {
    let f = Fixture::new().await;
    f.window(at(9, 0), at(15, 0)).await;
    f.change("a", None, vec![f.success_only("prod", 1, at(10, 0))])
        .await;
    f.transition(at(10, 20), "success", "degraded", "a").await;
    f.transition(at(10, 50), "degraded", "success", "a").await;
    let metrics = f.metrics().await;
    assert_eq!(
        metric(&metrics, MetricId::ChangeFailureRate).value,
        Some(100.0)
    );
    assert_eq!(metric(&metrics, MetricId::RecoveryTime).value, Some(1800.0));
}

#[tokio::test]
async fn coverage_counts_match() {
    let f = Fixture::new().await;
    for (commit, authored, run) in [
        ("a", Some(at(9, 0)), 1),
        ("b", Some(at(9, 30)), 2),
        ("c", None, 3),
    ] {
        f.change(
            commit,
            authored,
            vec![f.success_only("prod", run, at(10, run as u32 * 10))],
        )
        .await;
    }
    let metrics = f.metrics().await;
    let lead = metric(&metrics, MetricId::LeadTime);
    assert_eq!(lead.coverage.counted, 2);
    assert_eq!(lead.coverage.excluded, 1);
    assert_eq!(lead.coverage.counted + lead.coverage.excluded, 3);
    assert_eq!(lead.value, Some(3600.0));
    assert_eq!(lead.p50, Some(3600.0));
    assert!(lead.p95.is_some());
    assert_eq!(lead.coverage.label, copy::coverage(2, 1));
    let frequency = metric(&metrics, MetricId::DeployFrequency);
    assert_eq!(frequency.coverage.counted, 3);
    assert_eq!(frequency.coverage.excluded, 0);
    assert_eq!(frequency.series.len(), 1);
}

#[tokio::test]
async fn lead_time_without_commit_times_is_unavailable() {
    let f = Fixture::new().await;
    f.change("a", None, vec![f.success_only("prod", 1, at(10, 0))])
        .await;
    let metrics = f.metrics().await;
    let lead = metric(&metrics, MetricId::LeadTime);
    assert_eq!(lead.value, None);
    assert_eq!(lead.unavailable_reason, Some(copy::no_commit_times()));
}
