use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{
    Duration,
    Utc,
};

use super::super::super::vocab::{
    ORIGIN_LOG,
    REL_PRODUCED,
};
use super::super::tests::{
    digest,
    sha,
    Fixture,
};
use super::{
    record,
    run_hash,
    Lines,
};
use crate::application::test_support::{
    pipeline,
    stub_run,
    StubProvider,
};
use crate::domain::{
    LogAvailability,
    LogNode,
    LogNodeKind,
    PipelineStatus,
};
use crate::infrastructure::database::RunEvidenceRow;
use crate::infrastructure::PipedashConfig;

const RUN: i64 = 7;
const SOURCE: &str = "git.test/shop/api";

fn job() -> LogNode {
    LogNode {
        id: "1".to_string(),
        parent_id: None,
        kind: LogNodeKind::Job,
        name: "build".to_string(),
        status: PipelineStatus::Success,
        started_at: Some(Utc::now()),
        finished_at: Some(Utc::now()),
        log: Some(LogAvailability::Ready),
        web_url: None,
    }
}

fn wanted(entries: &[(String, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(digest, repository)| (digest.clone(), repository.to_string()))
        .collect()
}

fn lines_of(chunks: &[&str]) -> Vec<String> {
    let mut lines = Lines::default();
    let mut found = Vec::new();
    for chunk in chunks {
        lines.push(chunk.as_bytes(), |line| found.push(line.to_string()));
    }
    lines.finish(|line| found.push(line.to_string()));
    found
}

#[test]
fn lines_are_rejoined_across_chunks() {
    assert_eq!(
        lines_of(&["one\ntw", "o\r\nthree"]),
        vec!["one", "two", "three"]
    );
}

#[test]
fn an_endless_line_is_dropped_instead_of_buffered() {
    let long = "x".repeat(70 * 1024);
    assert_eq!(lines_of(&[long.as_str(), "\nok\n"]), vec!["", "ok"]);
}

#[test]
fn run_hash_follows_the_run_and_the_wanted_digests() {
    let run = RunEvidenceRow {
        pipeline_id: "p".into(),
        run_number: 1,
        outcome_recorded: true,
        timing_recorded: true,
        attempt: true,
        started_at: Utc::now(),
        concluded_at: None,
        status: "success".into(),
    };
    let one = wanted(&[(digest('a'), "r")]);
    let two = wanted(&[(digest('a'), "r"), (digest('b'), "r")]);
    assert_eq!(run_hash(&run, &one), run_hash(&run, &one));
    assert_ne!(run_hash(&run, &one), run_hash(&run, &two));
    let failed = RunEvidenceRow {
        status: "failed".into(),
        ..run.clone()
    };
    assert_ne!(run_hash(&run, &one), run_hash(&failed, &one));
}

#[test]
fn only_wanted_pushes_are_recorded() {
    let pushed = digest('b');
    let other = digest('c');
    let wanted = wanted(&[(pushed.clone(), "registry.test/shop/api")]);
    let mut found = BTreeMap::new();
    record(
        &format!("latest: digest: {other} size: 10"),
        &wanted,
        &mut found,
    );
    assert!(found.is_empty());
    record(
        &format!("pushing manifest for registry.test/mirror/api@{pushed}"),
        &wanted,
        &mut found,
    );
    assert_eq!(
        found.get(&pushed).map(String::as_str),
        Some("registry.test/mirror/api")
    );
}

async fn build_run(f: &Fixture, log: &str) -> (i64, String, Arc<StubProvider>) {
    let (provider, stub) = f
        .stub(|stub| {
            let pipeline_id = format!("stub__{}__build", stub.provider_id);
            stub.with_logs(vec![job()], &[log])
                .with_runs(vec![stub_run(&pipeline_id, RUN, 60)])
        })
        .await;
    f.h.repository
        .update_pipelines_cache(
            provider,
            &[pipeline(provider, "build", PipelineStatus::Success)],
        )
        .await
        .unwrap();
    let pipeline_id = format!("stub__{provider}__build");
    f.source(provider, &pipeline_id, RUN, SOURCE, Some(&sha('1')))
        .await;
    f.concluded_run(&pipeline_id, RUN, Utc::now() - Duration::hours(1))
        .await;
    (provider, pipeline_id, stub)
}

async fn consume_labelled(
    f: &Fixture, provider: i64, name: &str, image: &str, repository: &str,
) -> String {
    let subject = format!("stub__{provider}__{name}");
    f.consume(provider, &subject, image, repository, Utc::now())
        .await;
    f.artifact(image, repository, Utc::now(), Some(SOURCE))
        .await;
    subject
}

#[tokio::test]
async fn base_image_digest_in_log_is_not_produced() {
    let f = Fixture::new().await;
    let base = digest('a');
    let built = digest('b');
    let log = format!(
        "Step 1/4 : FROM registry.test/base/img@{base}\nDigest: {base}\nStatus: Downloaded newer image for registry.test/base/img@{base}\nlatest: digest: {built} size: 1234\n"
    );
    let (provider, pipeline_id, stub) = build_run(&f, &log).await;
    let api = consume_labelled(&f, provider, "api", &built, "registry.test/shop/api").await;
    let image = consume_labelled(&f, provider, "image", &base, "registry.test/base/img").await;

    let touched = f.service.scan_logs().await.unwrap();

    let mut conn = f.service.store.conn();
    let rows = conn
        .list_artifact_observations_by_subject(&pipeline_id)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].relation, REL_PRODUCED);
    assert_eq!(rows[0].origin, ORIGIN_LOG);
    assert_eq!(rows[0].digest.as_deref(), Some(built.as_str()));
    assert_eq!(rows[0].repository, "registry.test/shop/api");
    assert!(touched.contains(&pipeline_id));
    assert!(touched.contains(&api));
    assert!(!touched.contains(&image));
    assert!(conn
        .get_log_scan(&pipeline_id, RUN)
        .await
        .unwrap()
        .is_some());
    assert_eq!(stub.log_read_count(), 1);
    assert!(f.service.scan_logs().await.unwrap().is_empty());
    assert_eq!(stub.log_read_count(), 1);
}

#[tokio::test]
async fn a_run_without_a_matching_source_is_not_scanned() {
    let f = Fixture::new().await;
    let built = digest('b');
    let log = format!("latest: digest: {built} size: 1234\n");
    let (provider, _, stub) = build_run(&f, &log).await;
    let subject = format!("stub__{provider}__api");
    f.consume(
        provider,
        &subject,
        &built,
        "registry.test/shop/api",
        Utc::now(),
    )
    .await;
    f.artifact(
        &built,
        "registry.test/shop/api",
        Utc::now(),
        Some("git.test/other/repo"),
    )
    .await;

    assert!(f.service.scan_logs().await.unwrap().is_empty());
    assert_eq!(stub.log_read_count(), 0);
}

#[tokio::test]
async fn log_evidence_off_scans_nothing() {
    let mut config = PipedashConfig::default();
    config.correlation.log_evidence = false;
    let f = Fixture::with_config(config).await;
    let built = digest('b');
    let log = format!("latest: digest: {built} size: 1234\n");
    let (provider, pipeline_id, stub) = build_run(&f, &log).await;
    consume_labelled(&f, provider, "api", &built, "registry.test/shop/api").await;

    assert!(f.service.scan_logs().await.unwrap().is_empty());

    assert_eq!(stub.log_node_fetch_count(), 0);
    assert!(f
        .service
        .store
        .conn()
        .get_log_scan(&pipeline_id, RUN)
        .await
        .unwrap()
        .is_none());
}
