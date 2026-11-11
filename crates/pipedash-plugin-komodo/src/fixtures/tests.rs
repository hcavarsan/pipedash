use pipedash_plugin_api::{
    Observed,
    RunEvidence,
};

use super::*;

#[test]
fn fixtures_run_the_real_mappers_over_a_stack_with_a_detailed_and_a_listed_update() {
    let fixtures = correlation_fixtures();

    assert_eq!(fixtures.len(), 1);
    let (pipeline, runs) = &fixtures[0];
    assert!(!pipeline.correlation.is_empty());
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].evidence, RunEvidence::default());
    assert_eq!(runs[1].evidence.timing, Observed::Absent);
    assert_eq!(runs[1].duration_seconds, None);
    assert!(runs[0].correlation.sources[0].commit.is_some());
    assert!(runs[1].correlation.sources[0].commit.is_none());
}
