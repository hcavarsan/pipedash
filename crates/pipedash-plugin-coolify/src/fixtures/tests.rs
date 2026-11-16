use pipedash_plugin_api::RunEvidence;

use super::*;

#[test]
fn fixtures_run_the_real_mappers_over_a_git_and_an_image_application() {
    let fixtures = correlation_fixtures();

    assert_eq!(fixtures.len(), 2);
    for (pipeline, runs) in &fixtures {
        assert!(!pipeline.correlation.is_empty());
        assert_eq!(runs.len(), 2);
        for run in runs {
            assert_eq!(run.evidence, RunEvidence::default());
        }
    }
    let (git, runs) = &fixtures[0];
    assert_eq!(git.correlation.sources.len(), 1);
    assert_eq!(runs[0].correlation.sources.len(), 1);
    assert!(runs[0].correlation.sources[0].commit.is_some());
    assert!(runs[1].correlation.sources[0].commit.is_none());
    assert_eq!(fixtures[1].0.correlation.consumed.len(), 1);
}
