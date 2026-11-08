use chrono::TimeZone;
use serde_json::from_value;

use super::*;

fn app(value: Value) -> Application {
    from_value(value).unwrap()
}

fn deployment(value: Value) -> Deployment {
    let mut base = json!({
        "id": 117, "deployment_uuid": "d117", "created_at": "2026-11-01T10:00:00Z",
        "status": "finished"
    });
    if let (Some(base), Some(extra)) = (base.as_object_mut(), value.as_object()) {
        base.extend(extra.clone());
    }
    from_value(base).unwrap()
}

#[test]
fn app_state_maps_every_state_and_health() {
    let cases = [
        ("running:healthy", PipelineStatus::Success, "Up"),
        ("running:unknown", PipelineStatus::Success, "Up"),
        ("running:unhealthy", PipelineStatus::Degraded, "Unhealthy"),
        ("degraded:unhealthy", PipelineStatus::Degraded, "Degraded"),
        ("restarting:unknown", PipelineStatus::Degraded, "Restarting"),
        ("dead:unhealthy", PipelineStatus::Failed, "Dead"),
        ("dead:unknown", PipelineStatus::Failed, "Dead"),
        ("dead", PipelineStatus::Failed, "Dead"),
        ("starting:unknown", PipelineStatus::Running, "Starting"),
        ("exited:unhealthy", PipelineStatus::Cancelled, "Exited"),
        ("paused:unknown", PipelineStatus::Cancelled, "Paused"),
        ("removing:unknown", PipelineStatus::Pending, "Unknown"),
        ("", PipelineStatus::Pending, "Unknown"),
    ];
    for (status, expected, label) in cases {
        let (mapped, state) = map_app_state(status);
        assert_eq!(
            (mapped, state.label.as_str()),
            (expected, label),
            "{status}"
        );
    }
}

#[test]
fn dead_containers_fail_after_a_finished_deployment() {
    let dead = app(json!({"uuid": "a", "name": "web", "status": "dead:unhealthy"}));
    let pipeline = map_pipeline(&dead, Some(&deployment(json!({}))));

    assert_eq!(pipeline.status, PipelineStatus::Failed);
    assert_eq!(pipeline.resource_state.unwrap().label, "Dead");
    assert_eq!(pipeline.metadata["state"], json!("Dead"));
}

#[test]
fn an_unfinished_latest_deployment_overrides_live_health() {
    let healthy = app(json!({"uuid": "a", "name": "web", "status": "running:healthy"}));
    let unhealthy = app(json!({"uuid": "a", "name": "web", "status": "running:unhealthy"}));
    let exited = app(json!({"uuid": "a", "name": "web", "status": "exited:unhealthy"}));

    let failed = map_pipeline(&healthy, Some(&deployment(json!({"status": "failed"}))));
    assert_eq!(failed.status, PipelineStatus::Failed);
    assert_eq!(failed.resource_state, None);
    assert_eq!(failed.metadata["state"], json!("Up"));

    let finished = map_pipeline(&unhealthy, Some(&deployment(json!({}))));
    assert_eq!(finished.status, PipelineStatus::Degraded);
    assert_eq!(finished.resource_state.unwrap().label, "Unhealthy");

    let never = map_pipeline(&exited, None);
    assert_eq!(never.status, PipelineStatus::Cancelled);
    assert_eq!(never.resource_state.unwrap().label, "Exited");
    assert_eq!(never.last_run, None);

    let running = map_pipeline(
        &healthy,
        Some(&deployment(json!({"status": "in_progress"}))),
    );
    assert_eq!(running.status, PipelineStatus::Running);
    assert_eq!(running.resource_state, None);
    assert_eq!(running.metadata["state"], json!("Up"));
}

#[test]
fn runs_conclude_only_when_terminal_and_skip_symbolic_commits() {
    let finished = deployment(json!({
        "commit": "HEAD", "updated_at": "2026-11-01T10:01:30Z", "finished_at": null
    }));
    let run = map_run("a", "https://c.example", &finished);
    assert_eq!(run.commit_sha, None);
    assert_eq!(
        run.concluded_at,
        Some(Utc.with_ymd_and_hms(2026, 11, 1, 10, 1, 30).unwrap())
    );
    assert_eq!(run.duration_seconds, Some(90));
    assert_eq!(run.logs_url, "https://c.example");

    let clock_skew = deployment(json!({"updated_at": "2026-11-01T09:59:00Z"}));
    assert_eq!(map_run("p", "", &clock_skew).duration_seconds, Some(0));

    let in_progress = deployment(json!({
        "status": "in_progress", "commit": "abc123", "updated_at": "2026-11-01T10:01:30Z",
        "deployment_url": "/project/p/environment/e/application/a/deployment/d117"
    }));
    let run = map_run("a", "https://c.example", &in_progress);
    assert_eq!(run.concluded_at, None);
    assert_eq!(run.duration_seconds, None);
    assert_eq!(run.commit_sha.as_deref(), Some("abc123"));
    assert_eq!(
        run.logs_url,
        "https://c.example/project/p/environment/e/application/a/deployment/d117"
    );
    assert_eq!(run.run_number, 117);
    assert_eq!(run.id, "a__d117");
}

#[test]
fn run_source_follows_coolify_precedence() {
    let cases = [
        (
            json!({"pull_request_id": 4, "is_webhook": true}),
            "Pull request",
        ),
        (json!({"is_webhook": true, "rollback": true}), "Webhook"),
        (json!({"rollback": true, "is_api": true}), "Rollback"),
        (json!({"is_api": true}), "API"),
        (json!({}), "Manual"),
    ];
    for (fields, expected) in cases {
        let run = map_run("p", "", &deployment(fields));
        assert_eq!(run.metadata["source"], json!(expected));
    }
}

#[test]
fn repository_is_the_image_for_docker_image_apps_and_the_git_repo_otherwise() {
    let image = app(json!({
        "uuid": "a", "name": "img", "build_pack": "dockerimage",
        "git_repository": "coollabsio/coolify", "git_branch": "main",
        "docker_registry_image_name": "ghcr.io/acme/img", "docker_registry_image_tag": "1.2"
    }));
    let pipeline = map_pipeline(&image, None);
    assert_eq!(pipeline.repository, "ghcr.io/acme/img:1.2");
    assert_eq!(pipeline.branch, None);

    let git = app(json!({
        "uuid": "b", "name": "web", "build_pack": "nixpacks",
        "git_repository": "acme/web", "git_branch": "main",
        "docker_registry_image_name": null, "fqdn": null
    }));
    let pipeline = map_pipeline(&git, None);
    assert_eq!(pipeline.repository, "acme/web");
    assert_eq!(pipeline.branch.as_deref(), Some("main"));
}
