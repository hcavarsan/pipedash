use std::collections::HashMap;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use pipedash_plugin_api::{
    CommitSha,
    RepoId,
    SourceObservation,
};
use reqwest::Url;
use serde::Deserialize;

const SLSA_PROVENANCE: &str = "https://slsa.dev/provenance/";
const GIT_COMMIT: &str = "gitCommit";

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Invocation {
    pub owner: String,
    pub repo: String,
    pub run_id: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Attested {
    pub source: SourceObservation,
    pub invocation: Option<Invocation>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Statement {
    predicate_type: String,
    predicate: Predicate,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Predicate {
    build_definition: BuildDefinition,
    #[serde(default)]
    run_details: Option<RunDetails>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildDefinition {
    external_parameters: ExternalParameters,
    #[serde(default)]
    resolved_dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct ExternalParameters {
    #[serde(default)]
    workflow: Option<Workflow>,
}

#[derive(Deserialize)]
struct Workflow {
    repository: String,
}

#[derive(Deserialize)]
struct Dependency {
    #[serde(default)]
    digest: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RunDetails {
    #[serde(default)]
    metadata: Option<Metadata>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    #[serde(default)]
    invocation_id: Option<String>,
}

fn invocation(id: &str) -> Option<Invocation> {
    let url = Url::parse(id).ok()?;
    let mut segments = url.path_segments()?;
    let (owner, repo) = (segments.next()?, segments.next()?);
    if (segments.next()?, segments.next()?) != ("actions", "runs") {
        return None;
    }
    Some(Invocation {
        owner: owner.to_string(),
        repo: repo.to_string(),
        run_id: segments.next()?.parse().ok()?,
    })
}

pub(crate) fn decode(payload: &str) -> Option<Attested> {
    let statement: Statement =
        serde_json::from_slice(&STANDARD.decode(payload.trim()).ok()?).ok()?;
    if !statement.predicate_type.starts_with(SLSA_PROVENANCE) {
        return None;
    }
    let Predicate {
        build_definition,
        run_details,
    } = statement.predicate;
    let repo = RepoId::parse(&build_definition.external_parameters.workflow?.repository)?;
    let commit = build_definition
        .resolved_dependencies
        .iter()
        .find_map(|dependency| dependency.digest.get(GIT_COMMIT))
        .and_then(|commit| CommitSha::parse(commit));

    Some(Attested {
        source: SourceObservation {
            repo,
            path_scopes: Vec::new(),
            commit,
        },
        invocation: run_details
            .and_then(|details| details.metadata)
            .and_then(|metadata| metadata.invocation_id)
            .and_then(|id| invocation(&id)),
    })
}

#[cfg(test)]
mod tests;
