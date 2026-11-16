use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};

use super::*;
use crate::domain::PipelineStatus;

#[tokio::test]
async fn test_request_deduplication() {
    let dedup = RequestDeduplicator::<i32, ()>::new();
    let call_count = Arc::new(AtomicUsize::new(0));

    let request_id = hash_request(1, Generation::next(), "test");

    let dedup1 = dedup.clone();
    let count1 = call_count.clone();
    let handle1 = tokio::spawn(async move {
        dedup1
            .deduplicate(request_id, || async {
                count1.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                Ok::<_, ()>(42)
            })
            .await
    });

    let dedup2 = dedup.clone();
    let count2 = call_count.clone();
    let handle2 = tokio::spawn(async move {
        dedup2
            .deduplicate(request_id, || async {
                count2.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                Ok::<_, ()>(42)
            })
            .await
    });

    let (r1, r2) = tokio::join!(handle1, handle2);
    assert_eq!(r1.unwrap().unwrap(), 42);
    assert_eq!(r2.unwrap().unwrap(), 42);

    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn failed_request_does_not_block_the_next_one() {
    let dedup = RequestDeduplicator::<i32, &str>::new();
    let request_id = hash_request(1, Generation::next(), "fetch_runs_x_100");

    let first = dedup
        .deduplicate(request_id, || async { Err::<i32, &str>("provider error") })
        .await;
    assert_eq!(first, Err("provider error"));

    let second = tokio::time::timeout(
        tokio::time::Duration::from_secs(2),
        dedup.deduplicate(request_id, || async { Ok::<_, &str>(7) }),
    )
    .await;
    assert_eq!(second, Ok(Ok(7)));
}

#[tokio::test]
async fn dropped_request_does_not_block_the_next_one() {
    let dedup = RequestDeduplicator::<i32, &str>::new();
    let request_id = hash_request(1, Generation::next(), "fetch_runs_x_100");

    let abandoned = tokio::time::timeout(
        tokio::time::Duration::from_millis(20),
        dedup.deduplicate(request_id, || async {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            Ok::<_, &str>(1)
        }),
    )
    .await;
    assert!(abandoned.is_err());

    let next = tokio::time::timeout(
        tokio::time::Duration::from_secs(2),
        dedup.deduplicate(request_id, || async { Ok::<_, &str>(2) }),
    )
    .await;
    assert_eq!(next, Ok(Ok(2)));
}

#[tokio::test]
async fn failure_reaches_every_waiter_and_runs_once() {
    let dedup = RequestDeduplicator::<i32, &str>::new();
    let request_id = hash_request(1, Generation::next(), "fetch_runs_x_100");
    let calls = Arc::new(AtomicUsize::new(0));

    let leader = dedup.clone();
    let leader_calls = calls.clone();
    let first = tokio::spawn(async move {
        leader
            .deduplicate(request_id, || async move {
                leader_calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                Err::<i32, &str>("provider error")
            })
            .await
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let waiter_calls = calls.clone();
    let waiter = tokio::time::timeout(
        tokio::time::Duration::from_secs(2),
        dedup.deduplicate(request_id, || async move {
            waiter_calls.fetch_add(1, Ordering::SeqCst);
            Ok::<_, &str>(3)
        }),
    )
    .await;

    assert_eq!(first.await.unwrap(), Err("provider error"));
    assert_eq!(waiter, Ok(Err("provider error")));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_hash_request() {
    let generation = Generation::next();
    let h1 = hash_request(1, generation, "pipelines");
    let h2 = hash_request(1, generation, "pipelines");
    let h3 = hash_request(2, generation, "pipelines");
    let h4 = hash_request(1, Generation::next(), "pipelines");

    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
    assert_ne!(h1, h4);
}

#[tokio::test]
async fn different_generations_are_not_shared() {
    let dedup = RequestDeduplicator::<i32, &str>::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let old = hash_request(1, Generation::next(), "fetch_runs_x_100");
    let new = hash_request(1, Generation::next(), "fetch_runs_x_100");
    let run = |request_id, value| {
        let dedup = dedup.clone();
        let calls = calls.clone();
        async move {
            dedup
                .deduplicate(request_id, || async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    Ok::<_, &str>(value)
                })
                .await
        }
    };

    let (first, second) = tokio::join!(run(old, 1), run(new, 2));

    assert_eq!(first, Ok(1));
    assert_eq!(second, Ok(2));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

fn run(status: PipelineStatus) -> PipelineRun {
    PipelineRun {
        id: "1".into(),
        pipeline_id: "p".into(),
        run_number: 1,
        status,
        started_at: "2024-01-01T00:00:00Z".parse().unwrap(),
        concluded_at: None,
        duration_seconds: Some(60),
        logs_url: String::new(),
        commit_sha: None,
        commit_message: None,
        branch: Some("main".into()),
        actor: None,
        inputs: None,
        metadata: Default::default(),
        actions: Default::default(),
        display: Default::default(),
        pace: Default::default(),
        correlation: Default::default(),
        evidence: Default::default(),
    }
}

#[test]
fn test_hash_pipeline_run() {
    let h1 = hash_pipeline_run(&run(PipelineStatus::Success));
    let h2 = hash_pipeline_run(&run(PipelineStatus::Success));
    let h3 = hash_pipeline_run(&run(PipelineStatus::Failed));

    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
}
