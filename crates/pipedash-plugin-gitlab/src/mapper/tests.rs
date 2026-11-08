use super::*;

fn job(id: i64, name: &str, stage: &str, status: &str, started: bool) -> types::Job {
    types::Job {
        id,
        name: name.to_string(),
        stage: stage.to_string(),
        status: status.to_string(),
        started_at: started.then(|| "2026-10-27T10:00:00Z".parse().unwrap()),
        finished_at: None,
        web_url: None,
    }
}

#[test]
fn every_job_status_word_maps_to_a_status_and_a_log() {
    let cases = [
        (
            "created",
            false,
            PipelineStatus::Pending,
            Some(LogAvailability::Pending),
        ),
        (
            "pending",
            false,
            PipelineStatus::Running,
            Some(LogAvailability::Pending),
        ),
        (
            "preparing",
            false,
            PipelineStatus::Pending,
            Some(LogAvailability::Pending),
        ),
        (
            "waiting_for_resource",
            false,
            PipelineStatus::Pending,
            Some(LogAvailability::Pending),
        ),
        (
            "running",
            true,
            PipelineStatus::Running,
            Some(LogAvailability::Partial),
        ),
        (
            "success",
            true,
            PipelineStatus::Success,
            Some(LogAvailability::Ready),
        ),
        (
            "failed",
            true,
            PipelineStatus::Failed,
            Some(LogAvailability::Ready),
        ),
        ("failed", false, PipelineStatus::Failed, None),
        (
            "canceled",
            true,
            PipelineStatus::Cancelled,
            Some(LogAvailability::Ready),
        ),
        ("canceled", false, PipelineStatus::Cancelled, None),
        (
            "canceling",
            true,
            PipelineStatus::Running,
            Some(LogAvailability::Partial),
        ),
        ("skipped", false, PipelineStatus::Skipped, None),
        (
            "manual",
            false,
            PipelineStatus::Pending,
            Some(LogAvailability::Pending),
        ),
        (
            "scheduled",
            false,
            PipelineStatus::Pending,
            Some(LogAvailability::Pending),
        ),
    ];
    for (word, started, status, log) in cases {
        let nodes = jobs_to_log_nodes(&[job(1, "j", "s", word, started)]);
        assert_eq!(nodes[1].status, status, "{word}");
        assert_eq!(nodes[1].log, log, "{word} started={started}");
    }
}

#[test]
fn a_canceling_pipeline_still_counts_as_running() {
    let pipeline: types::Pipeline = serde_json::from_str(
        r#"{"id":7,"status":"canceling","ref":"main","sha":"abc","web_url":"https://gitlab.example/p/-/pipelines/7","created_at":"2026-10-29T10:00:00Z","updated_at":"2026-10-29T10:05:00Z","started_at":"2026-10-29T10:00:05Z","finished_at":null,"duration":null,"user":null}"#,
    )
    .unwrap();

    let run = map_pipeline_run(&pipeline, 1, None);

    assert_eq!(run.status, PipelineStatus::Running);
}

#[test]
fn stage_status_summarizes_its_jobs() {
    let cases = [
        (vec!["success", "failed", "running"], PipelineStatus::Failed),
        (vec!["success", "running"], PipelineStatus::Running),
        (vec!["success", "canceled"], PipelineStatus::Cancelled),
        (vec!["success", "manual"], PipelineStatus::Pending),
        (vec!["skipped", "skipped"], PipelineStatus::Skipped),
        (vec!["success", "skipped"], PipelineStatus::Success),
    ];
    for (words, expected) in cases {
        let jobs: Vec<_> = words
            .iter()
            .enumerate()
            .map(|(i, word)| job(i as i64 + 1, &format!("j{i}"), "s", word, true))
            .collect();
        assert_eq!(jobs_to_log_nodes(&jobs)[0].status, expected, "{words:?}");
    }
}

#[test]
fn a_retried_stage_keeps_its_place_and_its_latest_job() {
    let jobs = [
        job(30, "test", "test", "success", true),
        job(3, "ship", "deploy", "skipped", false),
        job(2, "test", "test", "failed", true),
        job(1, "build", "build", "success", true),
    ];

    let nodes = jobs_to_log_nodes(&jobs);

    let order: Vec<_> = nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(
        order,
        ["stage:build", "1", "stage:test", "30", "stage:deploy", "3"]
    );
}
