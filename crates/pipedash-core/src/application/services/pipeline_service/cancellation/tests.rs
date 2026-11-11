use std::sync::Arc;
use std::time::Duration;

use pipedash_plugin_api::AUTH_MODE_KEY;

use super::SCAN_CONCURRENCY;
use crate::application::services::presenter::READ_ONLY_PROVIDER;
use crate::application::test_support::{
    pipeline,
    stub_run,
    Gate,
    Harness,
    StubProvider,
};
use crate::domain::{
    Pipeline,
    PipelineRun,
    PipelineStatus,
    RunTarget,
};

fn run_as(pipeline_id: &str, number: i64, status: PipelineStatus) -> PipelineRun {
    let mut run = stub_run(pipeline_id, number, 60);
    run.status = status;
    run
}

async fn cache(h: &Harness, pipeline: &Pipeline, runs: Vec<PipelineRun>) {
    h.repository
        .update_pipelines_cache(pipeline.provider_id, std::slice::from_ref(pipeline))
        .await
        .unwrap();
    h.repository
        .cache_run_history(&pipeline.id, &runs)
        .await
        .unwrap();
}

#[tokio::test]
async fn cancellation_targets_should_keep_only_unfinished_runs_and_skip_pipelines_with_none() {
    let h = Harness::new().await;
    let (provider_id, _) = h.add_stub(StubProvider::new).await;
    let active = pipeline(provider_id, "a", PipelineStatus::Running);
    let idle = pipeline(provider_id, "b", PipelineStatus::Running);
    cache(
        &h,
        &active,
        vec![
            run_as(&active.id, 2, PipelineStatus::Running),
            run_as(&active.id, 1, PipelineStatus::Success),
            run_as(&active.id, 3, PipelineStatus::Pending),
        ],
    )
    .await;
    cache(&h, &idle, vec![run_as(&idle.id, 9, PipelineStatus::Failed)]).await;
    h.repository
        .update_pipelines_cache(provider_id, &[active.clone(), idle.clone()])
        .await
        .unwrap();

    let targets = h
        .pipeline_service
        .cancellation_targets(vec![active.id.clone(), idle.id.clone(), active.id.clone()])
        .await
        .unwrap();

    assert_eq!(targets.groups.len(), 1);
    assert_eq!(targets.groups[0].pipeline.id, active.id);
    assert_eq!(targets.groups[0].provider_name, "Stub");
    let numbers: Vec<i64> = targets.groups[0]
        .runs
        .iter()
        .map(|run| run.run_number)
        .collect();
    assert_eq!(numbers, [3, 2]);
    assert_eq!(targets.run_count, 2);
    assert!(targets.skipped.is_empty());
    assert!(targets.failed_pipeline_ids.is_empty());
}

#[tokio::test]
async fn cancellation_targets_should_skip_providers_that_cannot_cancel() {
    let h = Harness::new().await;
    let mut config = h.config("public", 30);
    config
        .config
        .insert(AUTH_MODE_KEY.to_string(), "none".to_string());
    let provider_id = h.repository.add_provider(&config).await.unwrap();
    let running = pipeline(provider_id, "build", PipelineStatus::Running);
    let finished = pipeline(provider_id, "lint", PipelineStatus::Success);
    cache(
        &h,
        &running,
        vec![run_as(&running.id, 1, PipelineStatus::Running)],
    )
    .await;
    h.repository
        .update_pipelines_cache(provider_id, &[running.clone(), finished.clone()])
        .await
        .unwrap();

    let targets = h
        .pipeline_service
        .cancellation_targets(vec![running.id.clone(), finished.id.clone()])
        .await
        .unwrap();

    assert!(targets.groups.is_empty());
    assert_eq!(targets.run_count, 0);
    assert_eq!(targets.skipped.len(), 1);
    assert_eq!(targets.skipped[0].pipeline_id, running.id);
    assert_eq!(targets.skipped[0].reason, READ_ONLY_PROVIDER);
}

#[tokio::test]
async fn cancellation_targets_should_find_old_runs_beyond_the_first_pages() {
    let h = Harness::new().await;
    let (provider_id, _) = h.add_stub(StubProvider::new).await;
    let deep = pipeline(provider_id, "deep", PipelineStatus::Success);
    let busy = pipeline(provider_id, "busy", PipelineStatus::Running);
    cache(
        &h,
        &deep,
        (1..=120)
            .map(|number| {
                let status = if number == 5 {
                    PipelineStatus::Running
                } else {
                    PipelineStatus::Success
                };
                run_as(&deep.id, number, status)
            })
            .collect(),
    )
    .await;
    h.repository
        .update_pipelines_cache(provider_id, &[deep.clone(), busy.clone()])
        .await
        .unwrap();
    h.repository
        .cache_run_history(
            &busy.id,
            &(1..=150)
                .map(|number| run_as(&busy.id, number, PipelineStatus::Running))
                .collect::<Vec<_>>(),
        )
        .await
        .unwrap();

    let targets = h
        .pipeline_service
        .cancellation_targets(vec![deep.id.clone(), busy.id.clone()])
        .await
        .unwrap();

    assert_eq!(targets.groups.len(), 2);
    assert_eq!(targets.groups[0].runs.len(), 1);
    assert_eq!(targets.groups[0].runs[0].run_number, 5);
    assert_eq!(targets.groups[1].runs.len(), 150);
    assert_eq!(targets.run_count, 151);
}

#[tokio::test]
async fn cancellation_targets_should_find_an_active_run_past_the_first_scan_budget() {
    let h = Harness::new().await;
    let (provider_id, _) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__deep");
            StubProvider::new(id).with_runs(
                (1..=1_200)
                    .rev()
                    .map(|number| {
                        let status = if number == 700 {
                            PipelineStatus::Running
                        } else {
                            PipelineStatus::Success
                        };
                        run_as(&pipeline_id, number, status)
                    })
                    .collect(),
            )
        })
        .await;
    let deep = pipeline(provider_id, "deep", PipelineStatus::Running);
    h.repository
        .update_pipelines_cache(provider_id, std::slice::from_ref(&deep))
        .await
        .unwrap();

    let targets = h
        .pipeline_service
        .cancellation_targets(vec![deep.id.clone()])
        .await
        .unwrap();

    assert!(targets.failed_pipeline_ids.is_empty());
    assert_eq!(targets.groups.len(), 1);
    let numbers: Vec<i64> = targets.groups[0]
        .runs
        .iter()
        .map(|run| run.run_number)
        .collect();
    assert_eq!(numbers, [700]);
    h.pipeline_service.settle().await;
}

#[tokio::test]
async fn cancellation_targets_should_report_unreadable_pipelines() {
    let h = Harness::new().await;

    let targets = h
        .pipeline_service
        .cancellation_targets(vec!["missing".to_string()])
        .await
        .unwrap();

    assert_eq!(targets.failed_pipeline_ids, ["missing"]);
    assert!(targets.groups.is_empty());
}

#[tokio::test]
async fn cancel_runs_should_report_every_failure_and_keep_going() {
    let h = Harness::new().await;
    let (provider_id, stub) = h.add_stub(StubProvider::new).await;
    *stub.cancelled.lock().unwrap() = Some(Vec::new());
    let build = pipeline(provider_id, "build", PipelineStatus::Running);
    let runs = vec![
        run_as(&build.id, 13, PipelineStatus::Running),
        run_as(&build.id, 1, PipelineStatus::Running),
    ];
    *stub.runs.lock().unwrap() = runs.clone();
    cache(&h, &build, runs).await;
    let target = |pipeline_id: &str, run_number| RunTarget {
        pipeline_id: pipeline_id.to_string(),
        run_number,
    };

    let batch = h
        .pipeline_service
        .cancel_runs(vec![
            target(&build.id, 13),
            target(&build.id, 1),
            target("missing", 4),
            target(&build.id, 1),
        ])
        .await
        .unwrap();

    assert_eq!(batch.requested_count, 3);
    assert_eq!(batch.failed_count, 2);
    assert!(batch.results[0]
        .error
        .as_deref()
        .unwrap()
        .contains("locked"));
    assert_eq!(batch.results[1].error, None);
    assert_eq!(batch.results[1].pipeline_name, "build");
    assert!(batch.results[2].error.is_some());
    assert_eq!(*stub.cancelled.lock().unwrap(), Some(vec![1]));
}

#[tokio::test]
async fn cancel_runs_should_reject_oversized_batches() {
    let h = Harness::new().await;
    let targets = (0..1001)
        .map(|run_number| RunTarget {
            pipeline_id: "stub__1__build".to_string(),
            run_number,
        })
        .collect();

    let result = h.pipeline_service.cancel_runs(targets).await;

    assert!(matches!(
        result,
        Err(crate::domain::DomainError::InvalidConfig(_))
    ));
}

async fn wait_until(condition: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the condition was never reached");
}

#[tokio::test]
async fn cancel_runs_should_run_in_parallel_within_the_limit_and_keep_input_order() {
    let h = Harness::new().await;
    let gates: Vec<Arc<Gate>> = (0..SCAN_CONCURRENCY + 2).map(|_| Gate::new()).collect();
    let (provider_id, stub) = h
        .add_stub(|id| {
            gates
                .iter()
                .zip(1..)
                .fold(StubProvider::new(id), |stub, (gate, run)| {
                    stub.cancel_gated(run, gate)
                })
        })
        .await;
    *stub.cancelled.lock().unwrap() = Some(Vec::new());
    let build = pipeline(provider_id, "build", PipelineStatus::Running);
    let numbers: Vec<i64> = (1..=gates.len() as i64).collect();
    let runs: Vec<PipelineRun> = numbers
        .iter()
        .map(|number| run_as(&build.id, *number, PipelineStatus::Running))
        .collect();
    *stub.runs.lock().unwrap() = runs.clone();
    cache(&h, &build, runs).await;
    let targets = numbers
        .iter()
        .map(|number| RunTarget {
            pipeline_id: build.id.clone(),
            run_number: *number,
        })
        .collect();
    let service = Arc::clone(&h.pipeline_service);
    let batch = tokio::spawn(async move { service.cancel_runs(targets).await });

    wait_until(|| stub.cancels_in_flight() == SCAN_CONCURRENCY).await;
    gates[SCAN_CONCURRENCY - 1].open();
    wait_until(|| *stub.cancelled.lock().unwrap() == Some(vec![SCAN_CONCURRENCY as i64])).await;
    assert!(!batch.is_finished());
    for gate in &gates {
        gate.open();
    }
    let batch = batch.await.unwrap().unwrap();

    assert!(stub.cancels_peak() > 1);
    assert!(stub.cancels_peak() <= SCAN_CONCURRENCY);
    assert_eq!(batch.failed_count, 0);
    assert_eq!(
        batch
            .results
            .iter()
            .map(|result| result.target.run_number)
            .collect::<Vec<_>>(),
        numbers
    );
}
