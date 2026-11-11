use pipedash_plugin_api::{
    CommitSha,
    Correlation,
    Observed,
    RepoId,
    RunEvidence,
    SourceObservation,
};

use crate::fixtures::{
    correlation_fixtures,
    job_json,
    BASE_SHA,
    EXTRA_SHA,
    PULL_SHA,
};
use crate::mapper::runs as map_runs;
use crate::types::ProwJob;

fn source(url: &str, commit: Option<&str>) -> SourceObservation {
    SourceObservation {
        repo: RepoId::parse(url).unwrap(),
        path_scopes: Vec::new(),
        commit: commit.and_then(CommitSha::parse),
    }
}

#[test]
fn refs_and_extra_refs_become_sources_and_runs_carry_their_commits() {
    let (pipeline, runs) = correlation_fixtures().remove(0);

    assert_eq!(
        pipeline.correlation,
        Correlation {
            sources: vec![
                source("https://github.com/shop/api", None),
                source("https://github.com/shop/config", None),
            ],
            ..Correlation::default()
        }
    );
    assert_eq!(
        runs[0].correlation,
        Correlation {
            sources: vec![
                source("https://github.com/shop/api", Some(PULL_SHA)),
                source("https://github.com/shop/config", Some(EXTRA_SHA)),
            ],
            ..Correlation::default()
        }
    );
    assert_eq!(runs[0].evidence, RunEvidence::default());
}

#[test]
fn refs_without_pulls_use_the_base_sha() {
    let mut json = job_json();
    json["spec"]["refs"]["pulls"] = serde_json::json!([]);
    let job: ProwJob = serde_json::from_value(json).unwrap();
    let deck = reqwest::Url::parse("https://prow.test").unwrap();

    let run = map_runs(&[job], "pull-shop-test", "id", &deck).remove(0);

    assert_eq!(
        run.correlation.sources[0],
        source("https://github.com/shop/api", Some(BASE_SHA))
    );
}

#[test]
fn job_without_start_time_has_no_timing_evidence() {
    let mut json = job_json();
    json["status"]["startTime"] = serde_json::Value::Null;
    json["metadata"]["creationTimestamp"] = serde_json::json!("2026-01-01T09:00:00Z");
    let job: ProwJob = serde_json::from_value(json).unwrap();
    let deck = reqwest::Url::parse("https://prow.test").unwrap();

    let run = map_runs(&[job], "pull-shop-test", "id", &deck).remove(0);

    assert_eq!(run.evidence.timing, Observed::Absent);
    assert_eq!(run.duration_seconds, None);
}
