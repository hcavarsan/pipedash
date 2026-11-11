use super::*;

const COMMIT: &str = "faef2ddd81b0736748413a7c646cd0bfc26c00a0";

fn statement() -> serde_json::Value {
    serde_json::json!({
        "_type": "https://in-toto.io/Statement/v1",
        "subject": [{"name": "ghcr.io/cli/cli", "digest": {"sha256": "8aad"}}],
        "predicateType": "https://slsa.dev/provenance/v1",
        "predicate": {
            "buildDefinition": {
                "buildType": "https://slsa-framework.github.io/github-actions-buildtypes/workflow/v1",
                "externalParameters": {"workflow": {
                    "ref": "refs/heads/trunk",
                    "repository": "https://github.com/cli/cli",
                    "path": ".github/workflows/deployment.yml"
                }},
                "resolvedDependencies": [{
                    "uri": "git+https://github.com/cli/cli@refs/heads/trunk",
                    "digest": {"gitCommit": COMMIT}
                }]
            },
            "runDetails": {
                "builder": {"id": "https://github.com/actions/runner/github-hosted"},
                "metadata": {
                    "invocationId": "https://github.com/cli/cli/actions/runs/9289075752/attempts/1"
                }
            }
        }
    })
}

fn encode(statement: &serde_json::Value) -> String {
    STANDARD.encode(statement.to_string())
}

#[test]
fn slsa_provenance_yields_the_source_commit_and_the_run() {
    assert_eq!(
        decode(&encode(&statement())),
        Some(Attested {
            source: SourceObservation {
                repo: RepoId::parse("https://github.com/cli/cli").unwrap(),
                path_scopes: Vec::new(),
                commit: CommitSha::parse(COMMIT),
            },
            invocation: Some(Invocation {
                owner: "cli".to_string(),
                repo: "cli".to_string(),
                run_id: 9289075752,
            }),
        })
    );
}

#[test]
fn missing_commit_and_invocation_still_attest_the_repository() {
    let mut statement = statement();
    statement["predicate"]["buildDefinition"]["resolvedDependencies"] = serde_json::json!([]);
    statement["predicate"]["runDetails"]["metadata"]["invocationId"] =
        serde_json::json!("https://github.com/cli/cli/actions/workflows/ci.yml");

    let attested = decode(&encode(&statement)).unwrap();

    assert_eq!(attested.source.commit, None);
    assert_eq!(attested.invocation, None);
}

#[test]
fn other_predicates_and_malformed_payloads_are_ignored() {
    let mut sbom = statement();
    sbom["predicateType"] = serde_json::json!("https://spdx.dev/Document/v2.3");
    let mut no_workflow = statement();
    no_workflow["predicate"]["buildDefinition"]["externalParameters"] = serde_json::json!({});
    let mut bad_repository = statement();
    bad_repository["predicate"]["buildDefinition"]["externalParameters"]["workflow"]
        ["repository"] = serde_json::json!("not a repository");

    for payload in [
        encode(&sbom),
        encode(&no_workflow),
        encode(&bad_repository),
        STANDARD.encode("not json"),
        "%%%".to_string(),
    ] {
        assert_eq!(decode(&payload), None, "{payload}");
    }
}
