use std::sync::Arc;

use chrono::{
    DateTime,
    TimeZone,
    Utc,
};

use super::segments::segments;
use super::StageRun;
use crate::application::services::correlation_service::vocab::{
    commit_node_id,
    ARTIFACT,
    ATTESTATION_COMMIT,
    BUILT_FROM,
    COMMIT,
};
use crate::application::services::correlation_service::{
    CorrelationDeps,
    CorrelationService,
};
use crate::application::test_support::Harness;
use crate::domain::application::dto::SegmentKind;
use crate::infrastructure::database::{
    ApplicationMemberRow,
    ApplicationRow,
    ApplicationTargetRow,
    ArtifactObservationRow,
    CorrelationEdgeRow,
    RunEvidenceRow,
    SourceObservationRow,
    TargetObservationRow,
    PIPELINE_LEVEL,
};
use crate::infrastructure::HttpClientManager;

const REPO: &str = "git.test/shop/api";

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 10, hour, minute, 0).unwrap()
}

fn run(
    started: DateTime<Utc>, concluded: Option<DateTime<Utc>>, flags: (bool, bool, bool),
) -> StageRun {
    StageRun {
        group_key: "prod".into(),
        pipeline_id: "stub__1__ci".into(),
        run_number: 1,
        started_at: started,
        concluded_at: concluded,
        status: "success".into(),
        outcome_recorded: flags.0,
        timing_recorded: flags.1,
        attempt: flags.2,
    }
}

#[test]
fn segments_sum_to_span() {
    let build = run(at(9, 50), Some(at(10, 0)), (true, true, true));
    let deploy = run(at(10, 5), Some(at(10, 15)), (true, true, true));
    let parts = segments(Some(&build), Some(&deploy));
    let kinds: Vec<SegmentKind> = parts.iter().map(|part| part.kind).collect();
    assert_eq!(
        kinds,
        [SegmentKind::Build, SegmentKind::Wait, SegmentKind::Deploy]
    );
    let total: i64 = parts.iter().map(|part| part.seconds).sum();
    assert_eq!(total, (at(10, 15) - at(9, 50)).num_seconds());

    let deployed_only = run(at(10, 15), Some(at(10, 15)), (false, false, false));
    let parts = segments(Some(&build), Some(&deployed_only));
    let kinds: Vec<SegmentKind> = parts.iter().map(|part| part.kind).collect();
    assert_eq!(kinds, [SegmentKind::Build, SegmentKind::BuildToDeployed]);
    let total: i64 = parts.iter().map(|part| part.seconds).sum();
    assert_eq!(total, (at(10, 15) - at(9, 50)).num_seconds());
}

struct Seeded {
    _harness: Harness,
    service: Arc<CorrelationService>,
    application: i64,
}

async fn seeded() -> Seeded {
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
        event_interval: crate::application::services::correlation_service::EVENT_INTERVAL,
    });
    let provider: i64 = sqlx::query_scalar(
        "INSERT INTO providers (name, provider_type, token_encrypted) \
         VALUES ('ci', 'stub', 'secret') RETURNING id",
    )
    .fetch_one(&harness.pool)
    .await
    .unwrap();
    let build = format!("stub__{provider}__build");
    let deploy = format!("stub__{provider}__api-prod");
    let sha = "a".repeat(40);
    let digest = format!("sha256:{}", "d".repeat(64));
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
    for (pipeline, stage) in [(&build, "build"), (&deploy, "deploy")] {
        conn.upsert_application_member(&ApplicationMemberRow {
            application_id: application,
            pipeline_id: pipeline.clone(),
            stage: stage.into(),
            source: "exact".into(),
            edge_id: None,
        })
        .await
        .unwrap();
    }
    conn.replace_application_targets(
        application,
        &[ApplicationTargetRow {
            application_id: application,
            target_key: "cluster|prod".into(),
            name: "prod".into(),
            group_key: "prod".into(),
            lifetime: "standing".into(),
        }],
    )
    .await
    .unwrap();
    let evidence = |pipeline: &str, number: i64, start, end| RunEvidenceRow {
        pipeline_id: pipeline.to_string(),
        run_number: number,
        outcome_recorded: true,
        timing_recorded: true,
        attempt: true,
        started_at: start,
        concluded_at: Some(end),
        status: "success".into(),
    };
    for row in [
        evidence(&build, 1, at(9, 50), at(10, 0)),
        evidence(&deploy, 5, at(10, 5), at(10, 15)),
        evidence(&deploy, 6, at(11, 0), at(11, 10)),
    ] {
        conn.upsert_run_evidence(&row).await.unwrap();
    }
    let source = |subject: &str, number: i64| {
        (
            subject.to_string(),
            number,
            vec![SourceObservationRow {
                provider_id: provider,
                subject_id: subject.to_string(),
                run_number: number,
                ordinal: 0,
                repo: REPO.into(),
                path_scopes_json: "[]".into(),
                commit_sha: Some(sha.clone()),
            }],
        )
    };
    for (subject, number, rows) in [source(&build, 1), source(&deploy, 6)] {
        conn.replace_source_observations(&subject, number, &rows)
            .await
            .unwrap();
    }
    conn.replace_target_observations(
        &deploy,
        PIPELINE_LEVEL,
        &[TargetObservationRow {
            provider_id: provider,
            subject_id: deploy.clone(),
            run_number: PIPELINE_LEVEL,
            ordinal: 0,
            cluster: "cluster".into(),
            namespace: Some("prod".into()),
            name: "prod".into(),
            lifetime: "standing".into(),
        }],
    )
    .await
    .unwrap();
    conn.replace_artifact_observations(
        &deploy,
        5,
        "provider",
        &[ArtifactObservationRow {
            provider_id: provider,
            subject_id: deploy.clone(),
            run_number: 5,
            relation: "consumed".into(),
            ordinal: 0,
            repository: "registry.test/shop/api".into(),
            digest: Some(digest.clone()),
            tag: None,
            target_ordinal: None,
            origin: "provider".into(),
        }],
    )
    .await
    .unwrap();
    conn.upsert_edge(&CorrelationEdgeRow {
        id: 0,
        from_kind: ARTIFACT.into(),
        from_id: digest,
        to_kind: COMMIT.into(),
        to_id: commit_node_id(REPO, &sha),
        relation: BUILT_FROM.into(),
        confidence: "exact".into(),
        rule: ATTESTATION_COMMIT.into(),
        evidence_json: "{}".into(),
        first_seen: at(10, 0),
        last_seen: at(10, 0),
    })
    .await
    .unwrap();
    Seeded {
        _harness: harness,
        service,
        application,
    }
}

#[tokio::test]
async fn rebuild_traces_a_commit_through_build_and_both_deploy_cases() {
    let seeded = seeded().await;
    seeded
        .service
        .rebuild_changes(seeded.application)
        .await
        .unwrap();
    let changes = seeded
        .service
        .all_changes(seeded.application)
        .await
        .unwrap();
    assert_eq!(changes.len(), 1);
    let change = &changes[0];
    assert_eq!(change.commit_sha, "a".repeat(40));
    assert_eq!(change.authored_at, None);
    assert_eq!(change.builds.len(), 1);
    let runs: Vec<i64> = change.deploys.iter().map(|run| run.run_number).collect();
    assert_eq!(runs, [5, 6]);
    assert!(change.deploys.iter().all(|run| run.group_key == "prod"));
    assert_eq!(change.first_deploy("prod").unwrap().run_number, 5);

    seeded
        .service
        .rebuild_changes(seeded.application)
        .await
        .unwrap();
    let again = seeded
        .service
        .all_changes(seeded.application)
        .await
        .unwrap();
    assert_eq!(again, changes);
}
