use pipedash_plugin_api::{
    Correlation,
    Digest,
    Observed,
};

use super::*;

fn correlations() -> Vec<(Correlation, Vec<Correlation>)> {
    all()
        .into_iter()
        .map(|(pipeline, runs)| {
            (
                pipeline.correlation,
                runs.into_iter().map(|run| run.correlation).collect(),
            )
        })
        .collect()
}

#[test]
fn fixtures_cover_both_kinds_with_runs_and_correlation() {
    let fixtures = all();

    assert_eq!(fixtures.len(), 2);
    for (pipeline, runs) in &fixtures {
        assert!(!pipeline.correlation.is_empty(), "{}", pipeline.id);
        assert!(!runs.is_empty(), "{}", pipeline.id);
    }
}

#[test]
fn fixture_correlation_satisfies_the_contract_invariants() {
    for (pipeline, runs) in all() {
        let correlation = &pipeline.correlation;
        assert!(correlation.consumed.iter().all(|consumed| consumed
            .target
            .is_none_or(|target| target < correlation.targets.len())));
        for run in &runs {
            if run.evidence.timing == Observed::Absent {
                assert_eq!(run.duration_seconds, None, "{}", run.id);
            }
            if run.evidence.outcome == Observed::Absent {
                assert!(!run.evidence.attempt, "{}", run.id);
            }
        }
    }
}

#[test]
fn fixtures_are_stable_across_calls() {
    assert_eq!(correlations(), correlations());
}

#[test]
fn the_kustomization_fixture_binds_the_digest_of_its_pods_to_its_only_target() {
    let fixtures = all();
    let correlation = &fixtures[0].0.correlation;

    assert_eq!(correlation.targets.len(), 1);
    assert_eq!(correlation.consumed.len(), 1);
    assert_eq!(correlation.consumed[0].target, Some(0));
    assert_eq!(
        correlation.consumed[0]
            .artifact
            .digest
            .as_ref()
            .map(Digest::as_str),
        Some(DIGEST)
    );
    assert_eq!(fixtures[0].1[0].correlation.sources.len(), 1);
}
