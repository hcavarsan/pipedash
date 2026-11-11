#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use pipedash_plugin_api::{
    CommitSha,
    Correlation,
    Observed,
    Pipeline,
    PipelineRun,
    Plugin,
    PluginError,
    PluginMetadata,
    RepoId,
};

type Fixtures = Vec<(Pipeline, Vec<PipelineRun>)>;

fn each_plugin(mut check: impl FnMut(&PluginMetadata, &dyn Plugin)) {
    let registry = pipedash_plugins::registry();
    for metadata in registry.metadata() {
        let plugin = registry.create(&metadata.provider_type).unwrap();
        check(metadata, plugin.as_ref());
    }
}

fn fixtures() -> Vec<(&'static str, Fixtures)> {
    vec![
        ("argocd", pipedash_plugin_argocd::correlation_fixtures()),
        (
            "azure-devops",
            pipedash_plugin_azure_devops::correlation_fixtures(),
        ),
        (
            "bitbucket",
            pipedash_plugin_bitbucket::correlation_fixtures(),
        ),
        (
            "buildkite",
            pipedash_plugin_buildkite::correlation_fixtures(),
        ),
        ("circleci", pipedash_plugin_circleci::correlation_fixtures()),
        ("coolify", pipedash_plugin_coolify::correlation_fixtures()),
        ("fluxcd", pipedash_plugin_fluxcd::correlation_fixtures()),
        ("gitea", pipedash_plugin_gitea::correlation_fixtures()),
        ("github", pipedash_plugin_github::correlation_fixtures()),
        ("gitlab", pipedash_plugin_gitlab::correlation_fixtures()),
        ("jenkins", pipedash_plugin_jenkins::correlation_fixtures()),
        ("komodo", pipedash_plugin_komodo::correlation_fixtures()),
        (
            "ociregistry",
            pipedash_plugin_ociregistry::correlation_fixtures(),
        ),
        ("prow", pipedash_plugin_prow::correlation_fixtures()),
        ("tekton", pipedash_plugin_tekton::correlation_fixtures()),
        (
            "woodpecker",
            pipedash_plugin_woodpecker::correlation_fixtures(),
        ),
    ]
}

fn correlations(fixtures: &Fixtures) -> Vec<&Correlation> {
    fixtures
        .iter()
        .flat_map(|(pipeline, runs)| {
            std::iter::once(&pipeline.correlation).chain(runs.iter().map(|run| &run.correlation))
        })
        .collect()
}

#[test]
fn every_registered_plugin_has_fixtures() {
    let covered: Vec<&str> = fixtures().into_iter().map(|(id, _)| id).collect();
    each_plugin(|metadata, _| {
        assert!(
            covered.contains(&metadata.provider_type.as_str()),
            "{} has no correlation fixtures",
            metadata.provider_type
        );
    });
}

#[test]
fn source_hosts_is_empty_without_config() {
    each_plugin(|metadata, plugin| {
        assert!(
            plugin.source_hosts(&HashMap::new()).is_empty(),
            "{}",
            metadata.provider_type
        );
    });
}

#[tokio::test]
async fn source_hosting_plugins_implement_the_commit_hooks() {
    let registry = pipedash_plugins::registry();
    let config = HashMap::from([("base_url".to_string(), "https://example.test".to_string())]);
    let repo = RepoId::parse("https://example.test/acme/shop").unwrap();
    let sha = CommitSha::parse(&"a".repeat(40)).unwrap();
    for metadata in registry.metadata() {
        let plugin = registry.create(&metadata.provider_type).unwrap();
        if plugin.source_hosts(&config).is_empty() {
            continue;
        }
        let unsupported = [
            matches!(
                plugin.resolve_commit(&repo, sha.as_str()).await,
                Err(PluginError::NotSupported(_))
            ),
            matches!(
                plugin.compare_commits(&repo, &sha, &sha).await,
                Err(PluginError::NotSupported(_))
            ),
            matches!(
                plugin.commit_changes(&repo, &sha).await,
                Err(PluginError::NotSupported(_))
            ),
        ];
        assert_eq!(unsupported, [false; 3], "{}", metadata.provider_type);
    }
}

#[test]
fn consumed_artifacts_point_at_an_existing_target() {
    for (id, fixtures) in fixtures() {
        for correlation in correlations(&fixtures) {
            for consumed in &correlation.consumed {
                assert!(
                    consumed
                        .target
                        .is_none_or(|index| index < correlation.targets.len()),
                    "{id}"
                );
            }
        }
    }
}

#[test]
fn run_evidence_never_contradicts_the_run() {
    for (id, fixtures) in fixtures() {
        for run in fixtures.iter().flat_map(|(_, runs)| runs) {
            if run.evidence.timing == Observed::Absent {
                assert_eq!(run.duration_seconds, None, "{id} run {}", run.run_number);
            }
            if run.evidence.outcome == Observed::Absent {
                assert!(!run.evidence.attempt, "{id} run {}", run.run_number);
            }
        }
    }
}

#[test]
fn observation_order_is_stable() {
    for ((id, first), (_, second)) in fixtures().into_iter().zip(fixtures()) {
        assert_eq!(correlations(&first), correlations(&second), "{id}");
    }
}
