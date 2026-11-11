use std::collections::BTreeMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    ChangedFile,
    ClusterId,
    CommitComparison,
    CommitInfo,
    CommitSha,
    Correlation,
    PathScope,
    PluginError,
    PluginResult,
    RepoId,
    SourceObservation,
    TargetLifetime,
    TargetObservation,
};

use crate::types;

pub(crate) mod paths;
pub(crate) mod provenance;

pub(crate) fn pipeline_correlation(
    repo: Option<RepoId>, path_scopes: Vec<PathScope>,
) -> Correlation {
    Correlation {
        sources: repo
            .map(|repo| SourceObservation {
                repo,
                path_scopes,
                commit: None,
            })
            .into_iter()
            .collect(),
        ..Correlation::default()
    }
}

pub(crate) fn deployment_targets(
    cluster: &ClusterId, from: DateTime<Utc>, to: DateTime<Utc>, deployments: &[types::Deployment],
) -> Vec<TargetObservation> {
    let mut environments: BTreeMap<&str, bool> = BTreeMap::new();
    for deployment in deployments
        .iter()
        .filter(|d| !d.environment.is_empty() && (from..=to).contains(&d.created_at))
    {
        *environments.entry(&deployment.environment).or_default() |=
            deployment.transient_environment.unwrap_or(false);
    }
    environments
        .into_iter()
        .map(|(name, transient)| TargetObservation {
            cluster: cluster.clone(),
            namespace: None,
            name: name.to_string(),
            lifetime: if transient {
                TargetLifetime::ShortLived
            } else {
                TargetLifetime::Standing
            },
        })
        .collect()
}

fn commit_sha(sha: &str) -> PluginResult<CommitSha> {
    CommitSha::parse(sha).ok_or_else(|| PluginError::ApiError(format!("Invalid commit SHA: {sha}")))
}

pub(crate) fn commit_info(commit: types::CommitResponse) -> PluginResult<CommitInfo> {
    let date = |stamp: Option<types::Stamp>, role: &str| {
        stamp.and_then(|stamp| stamp.date).ok_or_else(|| {
            PluginError::ApiError(format!("Commit {} has no {role} date", commit.sha))
        })
    };
    Ok(CommitInfo {
        sha: commit_sha(&commit.sha)?,
        authored_at: date(commit.commit.author, "author")?,
        committed_at: date(commit.commit.committer, "committer")?,
        message: commit.commit.message,
        parents: commit
            .parents
            .iter()
            .filter_map(|parent| CommitSha::parse(&parent.sha))
            .collect(),
    })
}

pub(crate) fn commit_comparison(comparison: types::Comparison) -> CommitComparison {
    CommitComparison {
        ahead: comparison.ahead_by,
        behind: comparison.behind_by,
        commits: comparison
            .commits
            .iter()
            .filter_map(|commit| CommitSha::parse(&commit.sha))
            .collect(),
    }
}

pub(crate) fn changed_files(commit: types::CommitResponse) -> Vec<ChangedFile> {
    commit
        .files
        .unwrap_or_default()
        .into_iter()
        .map(|file| ChangedFile {
            path: file.filename,
            patch: file.patch,
        })
        .collect()
}

#[cfg(test)]
mod tests;
