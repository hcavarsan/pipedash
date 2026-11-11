use pipedash_plugin_api::Observed;

use super::correlation_fixtures;

#[test]
fn fixtures_run_the_real_mappers_and_hold_the_evidence_invariants() {
    let fixtures = correlation_fixtures();

    assert_eq!(fixtures.len(), 1);
    let (pipeline, runs) = &fixtures[0];
    assert!(!pipeline.correlation.is_empty());
    assert_eq!(pipeline.correlation.sources.len(), 2);
    assert_eq!(pipeline.correlation.consumed.len(), 1);
    assert_eq!(runs.len(), 3);
    assert!(runs.iter().any(|run| !run.correlation.is_empty()));
    for run in runs {
        let target_count = run.correlation.targets.len();
        assert!(run
            .correlation
            .consumed
            .iter()
            .all(|consumed| consumed.target.is_none_or(|index| index < target_count)));
        if run.evidence.timing == Observed::Absent {
            assert_eq!(run.duration_seconds, None);
        }
        if run.evidence.outcome == Observed::Absent {
            assert!(!run.evidence.attempt);
        }
    }
}
