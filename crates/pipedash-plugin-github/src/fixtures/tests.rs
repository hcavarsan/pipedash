use pipedash_plugin_api::{
    ClusterId,
    CommitSha,
    Correlation,
    PathScope,
    RepoId,
    RunEvidence,
    SourceObservation,
    TargetLifetime,
    TargetObservation,
};

use super::*;

fn repo() -> RepoId {
    RepoId::from_host_path("github.test", "acme/app").unwrap()
}

#[test]
fn fixtures_run_the_real_mappers_into_exact_correlations() {
    let fixtures = correlation_fixtures();
    assert_eq!(fixtures.len(), 1);
    let (pipeline, runs) = &fixtures[0];

    assert_eq!(pipeline.id, "acme__app__42");
    assert_eq!(
        pipeline.correlation,
        Correlation {
            sources: vec![SourceObservation {
                repo: repo(),
                path_scopes: vec![PathScope::new("Cargo.toml"), PathScope::new("services/api")],
                commit: None,
            }],
            ..Correlation::default()
        }
    );

    let cluster = ClusterId::from_host("github.test/acme/app").unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].pipeline_id, pipeline.id);
    assert_eq!(
        runs[0].correlation,
        Correlation {
            sources: vec![SourceObservation {
                repo: repo(),
                path_scopes: Vec::new(),
                commit: CommitSha::parse("0123456789abcdef0123456789abcdef01234567"),
            }],
            targets: vec![
                TargetObservation {
                    cluster: cluster.clone(),
                    namespace: None,
                    name: "preview-7".to_string(),
                    lifetime: TargetLifetime::ShortLived,
                },
                TargetObservation {
                    cluster,
                    namespace: None,
                    name: "production".to_string(),
                    lifetime: TargetLifetime::Standing,
                },
            ],
            ..Correlation::default()
        }
    );
    assert_eq!(runs[0].evidence, RunEvidence::default());
}

#[test]
fn fixture_correlations_are_stable_across_calls() {
    let first = correlation_fixtures();
    let second = correlation_fixtures();

    assert_eq!(first[0].0.correlation, second[0].0.correlation);
    assert_eq!(first[0].1[0].correlation, second[0].1[0].correlation);
}
