use chrono::{
    DateTime,
    TimeZone,
    Utc,
};

use super::{
    ApplicationMemberRow,
    ApplicationOverrideRow,
    ApplicationRow,
    ApplicationTargetRow,
    ArtifactObservationRow,
    ArtifactRow,
    ChangeRow,
    ChangeStageRow,
    CorrelationConn,
    CorrelationEdgeRow,
    CorrelationRepository,
    LogScanRow,
    ObservationWindowRow,
    ObservedTransitionRow,
    RunEvidenceRow,
    SourceObservationRow,
    SubjectIdentityRow,
    TargetDirectionRow,
    TargetObservationRow,
    PIPELINE_LEVEL,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::database::bind::{
    execute_bound,
    fetch_all_as,
    BindValue,
};
use crate::infrastructure::database::{
    init_database,
    DatabasePool,
};

const TABLES: [&str; 17] = [
    "change_stages",
    "changes",
    "target_directions",
    "application_targets",
    "application_overrides",
    "application_members",
    "applications",
    "correlation_edges",
    "source_observations",
    "target_observations",
    "artifact_observations",
    "subject_identities",
    "run_evidence",
    "artifacts",
    "observed_transitions",
    "observation_windows",
    "log_scans",
];

fn at(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 10, hour, 0, 0).unwrap()
}

async fn insert_provider(pool: &DatabasePool, name: &str) -> i64 {
    let sql = format!(
        "INSERT INTO providers (name, provider_type, token_encrypted) VALUES ({}) RETURNING id",
        pool.placeholders(3)
    );
    let values = [
        BindValue::Text(name),
        BindValue::Text("demo"),
        BindValue::Text("secret"),
    ];
    let rows: Vec<(i64,)> = fetch_all_as(pool, &sql, &values).await.unwrap();
    rows[0].0
}

async fn clear(pool: &DatabasePool) {
    for table in TABLES {
        execute_bound(pool, &format!("DELETE FROM {table}"), &[])
            .await
            .unwrap();
    }
}

fn source(
    provider: i64, subject: &str, run: i64, ordinal: i64, repo: &str,
) -> SourceObservationRow {
    SourceObservationRow {
        provider_id: provider,
        subject_id: subject.to_string(),
        run_number: run,
        ordinal,
        repo: repo.to_string(),
        path_scopes_json: "[\"apps/api\"]".to_string(),
        commit_sha: (ordinal == 0).then(|| "a".repeat(40)),
    }
}

fn target(provider: i64, subject: &str, ordinal: i64, cluster: &str) -> TargetObservationRow {
    TargetObservationRow {
        provider_id: provider,
        subject_id: subject.to_string(),
        run_number: PIPELINE_LEVEL,
        ordinal,
        cluster: cluster.to_string(),
        namespace: (ordinal == 0).then(|| "shop".to_string()),
        name: format!("target-{ordinal}"),
        lifetime: if ordinal == 0 {
            "standing"
        } else {
            "short_lived"
        }
        .to_string(),
    }
}

fn artifact_obs(
    provider: i64, relation: &str, ordinal: i64, digest: &str, origin: &str,
) -> ArtifactObservationRow {
    ArtifactObservationRow {
        provider_id: provider,
        subject_id: "pipe-1".to_string(),
        run_number: 5,
        relation: relation.to_string(),
        ordinal,
        repository: "images.test/shop/api".to_string(),
        digest: Some(digest.to_string()),
        tag: Some("v1".to_string()),
        target_ordinal: (relation == "consumed").then_some(0),
        origin: origin.to_string(),
    }
}

async fn observation_tables(db: &mut CorrelationConn, provider: i64) {
    let sources = vec![
        source(provider, "pipe-1", PIPELINE_LEVEL, 0, "git.test/shop/api"),
        source(
            provider,
            "pipe-1",
            PIPELINE_LEVEL,
            1,
            "git.test/shop/config",
        ),
    ];
    db.replace_source_observations("pipe-1", PIPELINE_LEVEL, &sources)
        .await
        .unwrap();
    assert_eq!(
        db.list_source_observations_by_subject("pipe-1")
            .await
            .unwrap(),
        sources
    );
    assert_eq!(db.list_source_observations().await.unwrap().len(), 2);
    assert_eq!(
        db.list_source_observations_by_provider(provider)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        db.list_source_observations_by_repo("git.test/shop/api")
            .await
            .unwrap(),
        vec![sources[0].clone()]
    );
    let commit = "a".repeat(40);
    assert_eq!(
        db.list_source_observations_by_repo_commit("git.test/shop/api", &commit)
            .await
            .unwrap(),
        vec![sources[0].clone()]
    );
    db.replace_source_observations("pipe-1", PIPELINE_LEVEL, &sources[..1])
        .await
        .unwrap();
    assert_eq!(db.list_source_observations().await.unwrap().len(), 1);
    assert_eq!(
        db.delete_source_observations_by_subject("pipe-1")
            .await
            .unwrap(),
        1
    );
    assert!(db.list_source_observations().await.unwrap().is_empty());

    let targets = vec![
        target(provider, "pipe-1", 0, "uid:1"),
        target(provider, "pipe-1", 1, "uid:2"),
    ];
    db.replace_target_observations("pipe-1", PIPELINE_LEVEL, &targets)
        .await
        .unwrap();
    assert_eq!(
        db.list_target_observations_by_subject("pipe-1")
            .await
            .unwrap(),
        targets
    );
    assert_eq!(
        db.list_target_observations_by_cluster("uid:2")
            .await
            .unwrap(),
        vec![targets[1].clone()]
    );
    assert_eq!(
        db.list_target_observations_by_provider(provider)
            .await
            .unwrap(),
        targets
    );
    assert_eq!(db.list_target_observations().await.unwrap(), targets);
    assert_eq!(
        db.delete_target_observations_by_subject("pipe-1")
            .await
            .unwrap(),
        2
    );

    let produced = artifact_obs(provider, "produced", 0, "sha256:aa", "provider");
    let consumed = artifact_obs(provider, "consumed", 0, "sha256:aa", "provider");
    db.replace_artifact_observations(
        "pipe-1",
        5,
        "provider",
        &[produced.clone(), consumed.clone()],
    )
    .await
    .unwrap();
    let logged = artifact_obs(provider, "produced", 0, "sha256:bb", "ignored");
    db.replace_artifact_observations("pipe-1", 5, "log", &[logged])
        .await
        .unwrap();
    db.replace_artifact_observations("pipe-1", 5, "provider", &[produced.clone(), consumed])
        .await
        .unwrap();
    let stored = db
        .list_artifact_observations_by_subject("pipe-1")
        .await
        .unwrap();
    let shape: Vec<_> = stored
        .iter()
        .map(|row| (row.relation.as_str(), row.ordinal, row.origin.as_str()))
        .collect();
    assert_eq!(
        shape,
        vec![
            ("consumed", 0, "provider"),
            ("produced", 1, "log"),
            ("produced", 2, "provider"),
        ]
    );
    assert_eq!(stored[2].target_ordinal, None);
    assert_eq!(stored[0].target_ordinal, Some(0));
    assert_eq!(
        db.list_artifact_observations_by_digest("sha256:bb")
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        db.list_artifact_observations_by_repository("images.test/shop/api")
            .await
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        db.list_artifact_observations_by_provider(provider)
            .await
            .unwrap()
            .len(),
        3
    );
    assert_eq!(db.list_artifact_observations().await.unwrap().len(), 3);
    assert_eq!(
        db.delete_artifact_observations_by_subject("pipe-1")
            .await
            .unwrap(),
        3
    );

    let identity = SubjectIdentityRow {
        subject_id: "pipe-1".to_string(),
        provider_id: provider,
        unit_id: Some("1:ns/api".to_string()),
        unit_parent: None,
        unit_depends_on_json: "[]".to_string(),
        app_ref_authority: Some("auth".to_string()),
        app_ref_scope: Some("scope".to_string()),
        app_ref_id: Some("api".to_string()),
        app_label: None,
    };
    db.upsert_subject_identity(&identity).await.unwrap();
    let changed = SubjectIdentityRow {
        app_label: Some("shop".to_string()),
        ..identity.clone()
    };
    db.upsert_subject_identity(&changed).await.unwrap();
    assert_eq!(
        db.get_subject_identity("pipe-1").await.unwrap(),
        Some(changed.clone())
    );
    assert_eq!(
        db.list_subject_identities().await.unwrap(),
        vec![changed.clone()]
    );
    assert_eq!(
        db.list_subject_identities_by_provider(provider)
            .await
            .unwrap(),
        vec![changed]
    );
    assert!(db.delete_subject_identity("pipe-1").await.unwrap());
    assert!(!db.delete_subject_identity("pipe-1").await.unwrap());
}

async fn evidence_tables(db: &mut CorrelationConn, provider: i64) {
    let evidence = RunEvidenceRow {
        pipeline_id: "pipe-1".to_string(),
        run_number: 5,
        outcome_recorded: true,
        timing_recorded: false,
        attempt: true,
        started_at: at(8),
        concluded_at: Some(at(9)),
        status: "success".to_string(),
    };
    db.upsert_run_evidence(&evidence).await.unwrap();
    let running = RunEvidenceRow {
        run_number: 6,
        concluded_at: None,
        status: "running".to_string(),
        ..evidence.clone()
    };
    db.upsert_run_evidence(&running).await.unwrap();
    let revised = RunEvidenceRow {
        timing_recorded: true,
        ..evidence.clone()
    };
    db.upsert_run_evidence(&revised).await.unwrap();
    assert_eq!(
        db.get_run_evidence("pipe-1", 5).await.unwrap(),
        Some(revised.clone())
    );
    assert_eq!(
        db.list_run_evidence_by_pipeline("pipe-1").await.unwrap(),
        vec![revised.clone(), running.clone()]
    );
    assert_eq!(db.list_run_evidence().await.unwrap().len(), 2);
    assert_eq!(
        db.list_run_evidence_concluded_between(at(8), at(10))
            .await
            .unwrap(),
        vec![revised]
    );
    assert_eq!(
        db.delete_run_evidence_by_pipeline("pipe-1").await.unwrap(),
        2
    );

    let first = ObservedTransitionRow {
        id: 0,
        pipeline_id: "pipe-1".to_string(),
        provider_id: provider,
        at: at(11),
        status_before: None,
        status_after: "success".to_string(),
        health_before: None,
        health_after: Some("Healthy".to_string()),
        revision: Some("rev".to_string()),
        digests_json: "[\"sha256:aa\"]".to_string(),
    };
    let second = ObservedTransitionRow {
        at: at(10),
        status_before: Some("success".to_string()),
        status_after: "failed".to_string(),
        ..first.clone()
    };
    let first_id = db.insert_observed_transition(&first).await.unwrap();
    let second_id = db.insert_observed_transition(&second).await.unwrap();
    let stored = db
        .list_observed_transitions_by_pipeline("pipe-1")
        .await
        .unwrap();
    assert_eq!(
        stored,
        vec![
            ObservedTransitionRow {
                id: second_id,
                ..second
            },
            ObservedTransitionRow {
                id: first_id,
                ..first
            },
        ]
    );
    assert_eq!(
        db.list_observed_transitions_by_provider(provider)
            .await
            .unwrap(),
        stored
    );
    assert_eq!(db.list_observed_transitions().await.unwrap(), stored);
    assert_eq!(
        db.delete_observed_transitions_by_pipeline("pipe-1")
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        db.delete_observed_transitions_by_provider(provider)
            .await
            .unwrap(),
        0
    );

    let window = ObservationWindowRow {
        provider_id: provider,
        started_at: at(1),
        ended_at: at(2),
    };
    db.upsert_observation_window(&window).await.unwrap();
    let extended = ObservationWindowRow {
        ended_at: at(3),
        ..window
    };
    db.upsert_observation_window(&extended).await.unwrap();
    assert_eq!(
        db.list_observation_windows().await.unwrap(),
        vec![extended.clone()]
    );
    assert_eq!(
        db.list_observation_windows_by_provider(provider)
            .await
            .unwrap(),
        vec![extended]
    );
    assert_eq!(
        db.delete_observation_windows_by_provider(provider)
            .await
            .unwrap(),
        1
    );

    let scan = LogScanRow {
        pipeline_id: "pipe-1".to_string(),
        run_number: 5,
        run_hash: "h1".to_string(),
        scanned_at: at(4),
    };
    db.upsert_log_scan(&scan).await.unwrap();
    let rescan = LogScanRow {
        run_hash: "h2".to_string(),
        ..scan
    };
    db.upsert_log_scan(&rescan).await.unwrap();
    assert_eq!(
        db.get_log_scan("pipe-1", 5).await.unwrap(),
        Some(rescan.clone())
    );
    assert_eq!(
        db.list_log_scans_by_pipeline("pipe-1").await.unwrap(),
        vec![rescan.clone()]
    );
    assert_eq!(db.list_log_scans().await.unwrap(), vec![rescan]);
    assert_eq!(db.delete_log_scans_by_pipeline("pipe-1").await.unwrap(), 1);
}

async fn artifact_and_edge_tables(db: &mut CorrelationConn) -> i64 {
    let artifact = ArtifactRow {
        digest: "sha256:aa".to_string(),
        repository: "images.test/shop/api".to_string(),
        first_seen: at(10),
        label_source: None,
        label_revision: None,
        registry_checked_at: None,
        log_scanned_at: None,
    };
    db.upsert_artifact(&artifact).await.unwrap();
    db.upsert_artifact(&ArtifactRow {
        first_seen: at(12),
        ..artifact.clone()
    })
    .await
    .unwrap();
    assert_eq!(
        db.get_artifact("sha256:aa").await.unwrap(),
        Some(artifact.clone())
    );
    db.upsert_artifact(&ArtifactRow {
        first_seen: at(9),
        ..artifact.clone()
    })
    .await
    .unwrap();
    assert_eq!(
        db.get_artifact("sha256:aa")
            .await
            .unwrap()
            .unwrap()
            .first_seen,
        at(9)
    );
    assert!(db
        .mark_artifact_registry_checked("sha256:aa", Some("git.test/shop/api"), None, at(13))
        .await
        .unwrap());
    assert!(db
        .mark_artifact_log_scanned("sha256:aa", at(14))
        .await
        .unwrap());
    assert!(!db
        .mark_artifact_log_scanned("sha256:zz", at(14))
        .await
        .unwrap());
    let marked = db.get_artifact("sha256:aa").await.unwrap().unwrap();
    assert_eq!(marked.label_source.as_deref(), Some("git.test/shop/api"));
    assert_eq!(marked.registry_checked_at, Some(at(13)));
    assert_eq!(marked.log_scanned_at, Some(at(14)));
    assert_eq!(db.list_artifacts().await.unwrap(), vec![marked.clone()]);
    assert_eq!(
        db.list_artifacts_by_repository("images.test/shop/api")
            .await
            .unwrap(),
        vec![marked]
    );

    let edge = CorrelationEdgeRow {
        id: 0,
        from_kind: "run".to_string(),
        from_id: "pipe-1#5".to_string(),
        to_kind: "artifact".to_string(),
        to_id: "sha256:aa".to_string(),
        relation: "produced".to_string(),
        confidence: "exact".to_string(),
        rule: "provider_produced".to_string(),
        evidence_json: "{}".to_string(),
        first_seen: at(10),
        last_seen: at(10),
    };
    let id = db.upsert_edge(&edge).await.unwrap();
    let again = CorrelationEdgeRow {
        first_seen: at(11),
        last_seen: at(11),
        evidence_json: "{\"n\":2}".to_string(),
        ..edge.clone()
    };
    assert_eq!(db.upsert_edge(&again).await.unwrap(), id);
    let expected = CorrelationEdgeRow {
        id,
        first_seen: at(10),
        ..again
    };
    assert_eq!(db.get_edge(id).await.unwrap(), Some(expected.clone()));
    assert_eq!(db.list_edges().await.unwrap(), vec![expected.clone()]);
    assert_eq!(
        db.list_edges_from("run", "pipe-1#5").await.unwrap(),
        vec![expected.clone()]
    );
    assert_eq!(
        db.list_edges_to("artifact", "sha256:aa").await.unwrap(),
        vec![expected.clone()]
    );
    assert_eq!(
        db.list_edges_by_rule("provider_produced").await.unwrap(),
        vec![expected]
    );
    let other = db
        .upsert_edge(&CorrelationEdgeRow {
            to_id: "sha256:bb".to_string(),
            ..edge
        })
        .await
        .unwrap();
    assert_eq!(
        db.delete_edges_to("artifact", "sha256:bb").await.unwrap(),
        1
    );
    assert!(!db.delete_edge(other).await.unwrap());
    assert_eq!(db.delete_edges_from("run", "none").await.unwrap(), 0);
    id
}

fn application(key: &str, name: &str) -> ApplicationRow {
    ApplicationRow {
        id: 0,
        identity_kind: "unit".to_string(),
        identity_key: key.to_string(),
        name: name.to_string(),
        hidden: false,
        archived_at: None,
        created_at: at(1),
    }
}

async fn application_tables(db: &mut CorrelationConn, edge_id: i64) -> i64 {
    let app = application("1:ns/api", "api");
    let id = db.upsert_application(&app).await.unwrap();
    let renamed = ApplicationRow {
        name: "api-service".to_string(),
        hidden: true,
        created_at: at(2),
        ..app.clone()
    };
    assert_eq!(db.upsert_application(&renamed).await.unwrap(), id);
    let expected = ApplicationRow {
        id,
        created_at: at(1),
        ..renamed
    };
    assert_eq!(
        db.get_application(id).await.unwrap(),
        Some(expected.clone())
    );
    assert_eq!(
        db.get_application_by_identity("unit", "1:ns/api")
            .await
            .unwrap(),
        Some(expected.clone())
    );
    assert!(db.set_application_archived(id, Some(at(5))).await.unwrap());
    let archived = db.get_application(id).await.unwrap().unwrap();
    assert_eq!(archived.archived_at, Some(at(5)));
    assert!(db.set_application_archived(id, None).await.unwrap());
    let sibling = db
        .upsert_application(&application("1:ns/web", "web"))
        .await
        .unwrap();
    assert_eq!(db.list_applications().await.unwrap().len(), 2);

    let members = vec![
        ApplicationMemberRow {
            application_id: id,
            pipeline_id: "pipe-1".to_string(),
            stage: "deploy".to_string(),
            source: "exact".to_string(),
            edge_id: Some(edge_id),
        },
        ApplicationMemberRow {
            application_id: id,
            pipeline_id: "pipe-2".to_string(),
            stage: "build".to_string(),
            source: "config".to_string(),
            edge_id: None,
        },
    ];
    db.replace_application_members(id, &members).await.unwrap();
    assert_eq!(
        db.list_application_members_by_application(id)
            .await
            .unwrap(),
        members
    );
    assert_eq!(
        db.list_application_members_by_pipeline("pipe-2")
            .await
            .unwrap(),
        vec![members[1].clone()]
    );
    db.upsert_application_member(&ApplicationMemberRow {
        application_id: sibling,
        ..members[1].clone()
    })
    .await
    .unwrap();
    assert_eq!(db.list_application_members().await.unwrap().len(), 3);
    assert!(db
        .delete_application_member(sibling, "pipe-2")
        .await
        .unwrap());
    assert!(!db
        .delete_application_member(sibling, "pipe-2")
        .await
        .unwrap());

    let rename = ApplicationOverrideRow {
        id: 0,
        application_id: Some(id),
        kind: "rename".to_string(),
        subject: None,
        target: None,
        value_json: "{\"name\":\"api-service\"}".to_string(),
        author: Some("dev".to_string()),
        created_at: at(6),
    };
    let reject = ApplicationOverrideRow {
        application_id: None,
        kind: "unlink".to_string(),
        subject: Some("suggestion".to_string()),
        target: Some("pipe-9".to_string()),
        author: None,
        ..rename.clone()
    };
    let rename_id = db.insert_override(&rename).await.unwrap();
    let reject_id = db.insert_override(&reject).await.unwrap();
    assert_eq!(
        db.get_override(rename_id).await.unwrap(),
        Some(ApplicationOverrideRow {
            id: rename_id,
            ..rename
        })
    );
    assert_eq!(db.list_overrides().await.unwrap().len(), 2);
    assert_eq!(db.list_overrides_by_application(id).await.unwrap().len(), 1);
    assert!(db
        .set_override_application(reject_id, Some(sibling))
        .await
        .unwrap());
    assert_eq!(
        db.list_overrides_by_application(sibling)
            .await
            .unwrap()
            .len(),
        1
    );
    let temp = ApplicationOverrideRow {
        kind: "hide".to_string(),
        subject: None,
        target: None,
        ..reject
    };
    let temp_id = db.insert_override(&temp).await.unwrap();
    assert!(db.delete_override(temp_id).await.unwrap());
    assert!(!db.delete_override(temp_id).await.unwrap());

    let targets = vec![
        ApplicationTargetRow {
            application_id: id,
            target_key: "uid:1|shop".to_string(),
            name: "staging".to_string(),
            group_key: "uid:1|shop".to_string(),
            lifetime: "standing".to_string(),
        },
        ApplicationTargetRow {
            application_id: id,
            target_key: "uid:2|".to_string(),
            name: "preview".to_string(),
            group_key: "short_lived".to_string(),
            lifetime: "short_lived".to_string(),
        },
    ];
    db.replace_application_targets(id, &targets).await.unwrap();
    assert_eq!(
        db.list_application_targets_by_application(id)
            .await
            .unwrap(),
        targets
    );
    assert_eq!(db.list_application_targets().await.unwrap(), targets);
    let directions = vec![TargetDirectionRow {
        application_id: id,
        from_group: "uid:1|shop".to_string(),
        to_group: "uid:3|shop".to_string(),
        source: "provider".to_string(),
    }];
    db.replace_target_directions(id, &directions).await.unwrap();
    assert_eq!(
        db.list_target_directions_by_application(id).await.unwrap(),
        directions
    );
    assert_eq!(db.list_target_directions().await.unwrap(), directions);
    id
}

fn stage(change_id: i64, pipeline: &str, run: i64) -> ChangeStageRow {
    ChangeStageRow {
        change_id,
        stage: "deploy".to_string(),
        group_key: "uid:1|shop".to_string(),
        pipeline_id: pipeline.to_string(),
        run_number: run,
        started_at: at(8),
        concluded_at: Some(at(9)),
        status: "success".to_string(),
        outcome_recorded: true,
        timing_recorded: true,
        attempt: true,
    }
}

async fn change_tables(db: &mut CorrelationConn, application_id: i64) {
    let commit = "c".repeat(40);
    let change = ChangeRow {
        id: 0,
        application_id,
        repo: "git.test/shop/api".to_string(),
        commit_sha: commit.clone(),
        authored_at: None,
    };
    let first = db.upsert_change(&change).await.unwrap();
    let authored = ChangeRow {
        authored_at: Some(at(7)),
        ..change.clone()
    };
    assert_eq!(db.upsert_change(&authored).await.unwrap(), first);
    assert_eq!(db.upsert_change(&change).await.unwrap(), first);
    let expected = ChangeRow {
        id: first,
        ..authored
    };
    assert_eq!(db.get_change(first).await.unwrap(), Some(expected.clone()));
    assert_eq!(
        db.get_change_by_commit(application_id, "git.test/shop/api", &commit)
            .await
            .unwrap(),
        Some(expected.clone())
    );
    let later = db
        .upsert_change(&ChangeRow {
            commit_sha: "d".repeat(40),
            authored_at: Some(at(8)),
            ..change.clone()
        })
        .await
        .unwrap();
    let unknown = db
        .upsert_change(&ChangeRow {
            commit_sha: "e".repeat(40),
            ..change
        })
        .await
        .unwrap();
    let ids = |rows: Vec<ChangeRow>| rows.into_iter().map(|row| row.id).collect::<Vec<_>>();
    assert_eq!(
        ids(db.list_changes().await.unwrap()),
        vec![first, later, unknown]
    );
    assert_eq!(
        ids(db
            .list_changes_by_application(application_id)
            .await
            .unwrap()),
        vec![first, later, unknown]
    );
    assert_eq!(
        ids(db.list_changes_page(application_id, 10, 0).await.unwrap()),
        vec![later, first, unknown]
    );
    assert_eq!(
        ids(db.list_changes_page(application_id, 1, 1).await.unwrap()),
        vec![first]
    );
    assert_eq!(
        ids(db.list_changes_without_authored_at(5).await.unwrap()),
        vec![unknown]
    );
    assert_eq!(
        ids(db
            .list_changes_by_commit("git.test/shop/api", &commit)
            .await
            .unwrap()),
        vec![first]
    );

    let stages = vec![stage(first, "pipe-1", 5), stage(first, "pipe-2", 7)];
    db.replace_change_stages(first, &stages).await.unwrap();
    assert_eq!(
        db.list_change_stages_by_change(first).await.unwrap(),
        stages
    );
    assert_eq!(
        db.list_change_stages_by_application(application_id)
            .await
            .unwrap(),
        stages
    );
    assert_eq!(db.list_change_stages().await.unwrap(), stages);
    assert_eq!(
        db.list_change_stages_by_pipeline("pipe-2").await.unwrap(),
        vec![stages[1].clone()]
    );
    db.replace_change_stages(first, &stages[..1]).await.unwrap();
    assert_eq!(db.list_change_stages().await.unwrap().len(), 1);
    assert!(db.delete_change(unknown).await.unwrap());
    assert_eq!(
        db.delete_changes_by_application(application_id + 100)
            .await
            .unwrap(),
        0
    );
}

async fn cascades(db: &mut CorrelationConn, application_id: i64) {
    assert!(db.delete_application(application_id).await.unwrap());
    assert!(!db.delete_application(application_id).await.unwrap());
    assert!(db
        .list_application_members_by_application(application_id)
        .await
        .unwrap()
        .is_empty());
    assert!(db.list_application_targets().await.unwrap().is_empty());
    assert!(db.list_target_directions().await.unwrap().is_empty());
    assert!(db
        .list_overrides_by_application(application_id)
        .await
        .unwrap()
        .is_empty());
    assert!(db.list_changes().await.unwrap().is_empty());
    assert!(db.list_change_stages().await.unwrap().is_empty());
    assert_eq!(db.list_edges().await.unwrap().len(), 1);
    let sibling = db
        .get_application_by_identity("unit", "1:ns/web")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        db.list_overrides_by_application(sibling.id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(db.list_overrides().await.unwrap().len(), 1);
}

async fn transactions(repo: &CorrelationRepository) {
    let artifact = ArtifactRow {
        digest: "sha256:tx".to_string(),
        repository: "images.test/shop/tx".to_string(),
        first_seen: at(1),
        label_source: None,
        label_revision: None,
        registry_checked_at: None,
        log_scanned_at: None,
    };
    let rolled_back: DomainResult<()> = {
        let artifact = artifact.clone();
        repo.transaction(move |tx| {
            Box::pin(async move {
                tx.upsert_artifact(&artifact).await?;
                assert!(tx.get_artifact("sha256:tx").await?.is_some());
                Err(DomainError::InternalError("abort".to_string()))
            })
        })
        .await
    };
    assert!(rolled_back.is_err());
    assert_eq!(repo.conn().get_artifact("sha256:tx").await.unwrap(), None);

    let committed = artifact.clone();
    repo.transaction(move |tx| {
        Box::pin(async move {
            tx.upsert_artifact(&committed).await?;
            Ok(())
        })
    })
    .await
    .unwrap();
    assert_eq!(
        repo.conn().get_artifact("sha256:tx").await.unwrap(),
        Some(artifact)
    );
    repo.conn().delete_artifact("sha256:tx").await.unwrap();
}

async fn replace_is_atomic(repo: &CorrelationRepository, provider: i64) {
    let mut db = repo.conn();
    let good = vec![source(
        provider,
        "pipe-9",
        PIPELINE_LEVEL,
        0,
        "git.test/shop/api",
    )];
    db.replace_source_observations("pipe-9", PIPELINE_LEVEL, &good)
        .await
        .unwrap();
    let broken = vec![
        source(provider, "pipe-9", PIPELINE_LEVEL, 0, "git.test/shop/other"),
        source(i64::MAX, "pipe-9", PIPELINE_LEVEL, 1, "git.test/shop/api"),
    ];
    assert!(db
        .replace_source_observations("pipe-9", PIPELINE_LEVEL, &broken)
        .await
        .is_err());
    assert_eq!(
        db.list_source_observations_by_subject("pipe-9")
            .await
            .unwrap(),
        good
    );

    let mut tx = repo.begin().await.unwrap();
    tx.replace_source_observations("pipe-9", PIPELINE_LEVEL, &[])
        .await
        .unwrap();
    assert!(tx
        .list_source_observations_by_subject("pipe-9")
        .await
        .unwrap()
        .is_empty());
    tx.rollback().await.unwrap();
    assert_eq!(
        db.list_source_observations_by_subject("pipe-9")
            .await
            .unwrap(),
        good
    );
    db.delete_source_observations_by_subject("pipe-9")
        .await
        .unwrap();
}

async fn provider_delete_cascades(pool: &DatabasePool, provider: i64) {
    let repo = CorrelationRepository::new(pool.clone());
    let mut db = repo.conn();
    let sources = vec![source(
        provider,
        "pipe-3",
        PIPELINE_LEVEL,
        0,
        "git.test/shop/api",
    )];
    db.replace_source_observations("pipe-3", PIPELINE_LEVEL, &sources)
        .await
        .unwrap();
    db.replace_target_observations(
        "pipe-3",
        PIPELINE_LEVEL,
        &[target(provider, "pipe-3", 0, "uid:9")],
    )
    .await
    .unwrap();
    db.replace_artifact_observations(
        "pipe-3",
        1,
        "provider",
        &[artifact_obs(
            provider,
            "produced",
            0,
            "sha256:cc",
            "provider",
        )],
    )
    .await
    .unwrap();
    db.upsert_subject_identity(&SubjectIdentityRow {
        subject_id: "pipe-3".to_string(),
        provider_id: provider,
        unit_id: None,
        unit_parent: None,
        unit_depends_on_json: "[]".to_string(),
        app_ref_authority: None,
        app_ref_scope: None,
        app_ref_id: None,
        app_label: None,
    })
    .await
    .unwrap();
    let sql = format!("DELETE FROM providers WHERE id = {}", pool.placeholder(1));
    execute_bound(pool, &sql, &[BindValue::Int(provider)])
        .await
        .unwrap();
    assert!(db.list_source_observations().await.unwrap().is_empty());
    assert!(db.list_target_observations().await.unwrap().is_empty());
    assert!(db.list_artifact_observations().await.unwrap().is_empty());
    assert!(db.list_subject_identities().await.unwrap().is_empty());
}

async fn exercise(pool: DatabasePool) {
    clear(&pool).await;
    let suffix = Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let provider = insert_provider(&pool, &format!("correlation-{suffix}")).await;
    let doomed = insert_provider(&pool, &format!("correlation-doomed-{suffix}")).await;
    let repo = CorrelationRepository::new(pool.clone());
    let mut db = repo.conn();

    observation_tables(&mut db, provider).await;
    evidence_tables(&mut db, provider).await;
    let edge_id = artifact_and_edge_tables(&mut db).await;
    let application_id = application_tables(&mut db, edge_id).await;
    change_tables(&mut db, application_id).await;
    cascades(&mut db, application_id).await;
    transactions(&repo).await;
    replace_is_atomic(&repo, provider).await;
    provider_delete_cascades(&pool, doomed).await;

    clear(&pool).await;
    let sql = format!("DELETE FROM providers WHERE id = {}", pool.placeholder(1));
    execute_bound(&pool, &sql, &[BindValue::Int(provider)])
        .await
        .unwrap();
}

#[tokio::test]
async fn sqlite_correlation_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_database(dir.path().join("correlation.db"))
        .await
        .unwrap();

    exercise(DatabasePool::Sqlite(pool)).await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn postgres_correlation_round_trip() {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set to run the PostgreSQL tests");
    let pool = crate::infrastructure::database::init_postgres_database(&url)
        .await
        .unwrap();

    exercise(DatabasePool::Postgres(pool)).await;
}
