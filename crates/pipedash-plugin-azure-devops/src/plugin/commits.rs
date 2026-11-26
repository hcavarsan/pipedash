use pipedash_plugin_api::{
    ChangedFile,
    CommitComparison,
    CommitInfo,
    CommitSha,
    PluginError,
    PluginResult,
    RepoId,
};

use super::AzureDevOpsPlugin;
use crate::config;
use crate::types::GitCommit;

fn malformed(commit_id: &str, what: &str) -> PluginError {
    PluginError::ApiError(format!("Azure DevOps commit {commit_id} has no {what}"))
}

fn commit_info(commit: GitCommit) -> PluginResult<CommitInfo> {
    let id = commit.commit_id.as_str();
    Ok(CommitInfo {
        sha: CommitSha::parse(id).ok_or_else(|| malformed(id, "valid id"))?,
        authored_at: commit
            .author
            .and_then(|author| author.date)
            .ok_or_else(|| malformed(id, "author date"))?,
        committed_at: commit
            .committer
            .and_then(|committer| committer.date)
            .ok_or_else(|| malformed(id, "committer date"))?,
        message: commit.comment.unwrap_or_default(),
        parents: commit
            .parents
            .iter()
            .filter_map(|parent| CommitSha::parse(parent))
            .collect(),
    })
}

impl AzureDevOpsPlugin {
    pub(super) async fn resolve_commit_impl(
        &self, repo: &RepoId, rev: &str,
    ) -> PluginResult<CommitInfo> {
        let client = self.client()?;
        let git = config::git_repo(repo, &self.organization)?;
        let commit_id = match CommitSha::parse(rev) {
            Some(sha) => sha.as_str().to_string(),
            None => client
                .branch_head(&git.project, &git.name, rev)
                .await?
                .ok_or_else(|| {
                    PluginError::PipelineNotFound(format!("{rev} is not a branch of {}", git.name))
                })?,
        };
        commit_info(
            client
                .get_commit(&git.project, &git.name, &commit_id)
                .await?,
        )
    }

    pub(super) async fn compare_commits_impl(
        &self, repo: &RepoId, base: &CommitSha, head: &CommitSha,
    ) -> PluginResult<CommitComparison> {
        let client = self.client()?;
        let git = config::git_repo(repo, &self.organization)?;
        let diffs = client
            .diff_commits(&git.project, &git.name, base.as_str(), head.as_str())
            .await?;
        let mut commits = Vec::new();
        if diffs.ahead_count > 0 {
            let listed = client
                .commits_between(
                    &git.project,
                    &git.name,
                    base.as_str(),
                    head.as_str(),
                    diffs.ahead_count,
                )
                .await?;
            commits.extend(
                listed
                    .iter()
                    .rev()
                    .filter_map(|commit| CommitSha::parse(&commit.commit_id)),
            );
        }
        Ok(CommitComparison {
            ahead: diffs.ahead_count,
            behind: diffs.behind_count,
            commits,
        })
    }

    pub(super) async fn commit_changes_impl(
        &self, repo: &RepoId, sha: &CommitSha,
    ) -> PluginResult<Vec<ChangedFile>> {
        let client = self.client()?;
        let git = config::git_repo(repo, &self.organization)?;
        let items = client
            .commit_changes(&git.project, &git.name, sha.as_str())
            .await?;
        Ok(items
            .into_iter()
            .filter(|item| !item.is_folder)
            .filter_map(|item| item.path)
            .map(|path| ChangedFile {
                path: path.trim_start_matches('/').to_string(),
                patch: None,
            })
            .collect())
    }
}
