use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

use super::AzureDevOpsClient;
use crate::types::{
    GitChanges,
    GitCommit,
    GitCommitDiffs,
    GitItem,
    ListResponse,
};

const CHANGES_PAGE: usize = 1000;
const CHANGES_PAGES: usize = 20;
const COMMITS_LIMIT: u32 = 1000;

impl AzureDevOpsClient {
    pub async fn get_commit(
        &self, project: &str, repository: &str, commit_id: &str,
    ) -> PluginResult<GitCommit> {
        let path = format!("_apis/git/repositories/{repository}/commits/{commit_id}");
        let (commit, _) = self.get_json(Some(project), &path, &[]).await?;
        Ok(commit)
    }

    pub async fn branch_head(
        &self, project: &str, repository: &str, branch: &str,
    ) -> PluginResult<Option<String>> {
        let path = format!("_apis/git/repositories/{repository}/commits");
        let query = [
            ("searchCriteria.itemVersion.version", branch.to_string()),
            (
                "searchCriteria.itemVersion.versionType",
                "branch".to_string(),
            ),
            ("searchCriteria.$top", "1".to_string()),
        ];
        let (commits, _): (ListResponse<GitCommit>, _) =
            self.get_json(Some(project), &path, &query).await?;
        Ok(commits
            .value
            .into_iter()
            .next()
            .map(|commit| commit.commit_id))
    }

    pub async fn diff_commits(
        &self, project: &str, repository: &str, base: &str, head: &str,
    ) -> PluginResult<GitCommitDiffs> {
        let path = format!("_apis/git/repositories/{repository}/diffs/commits");
        let query = [
            ("baseVersion", base.to_string()),
            ("baseVersionType", "commit".to_string()),
            ("targetVersion", head.to_string()),
            ("targetVersionType", "commit".to_string()),
            ("$top", "1".to_string()),
        ];
        let (diffs, _) = self.get_json(Some(project), &path, &query).await?;
        Ok(diffs)
    }

    pub async fn commits_between(
        &self, project: &str, repository: &str, base: &str, head: &str, count: u32,
    ) -> PluginResult<Vec<GitCommit>> {
        let path = format!("_apis/git/repositories/{repository}/commits");
        let query = [
            ("searchCriteria.compareVersion.version", base.to_string()),
            (
                "searchCriteria.compareVersion.versionType",
                "commit".to_string(),
            ),
            ("searchCriteria.itemVersion.version", head.to_string()),
            (
                "searchCriteria.itemVersion.versionType",
                "commit".to_string(),
            ),
            ("searchCriteria.$top", count.min(COMMITS_LIMIT).to_string()),
        ];
        let (commits, _): (ListResponse<GitCommit>, _) =
            self.get_json(Some(project), &path, &query).await?;
        Ok(commits.value)
    }

    pub async fn commit_changes(
        &self, project: &str, repository: &str, commit_id: &str,
    ) -> PluginResult<Vec<GitItem>> {
        let path = format!("_apis/git/repositories/{repository}/commits/{commit_id}/changes");
        let mut items = Vec::new();
        for page in 0..CHANGES_PAGES {
            let query = [
                ("$top", CHANGES_PAGE.to_string()),
                ("$skip", (page * CHANGES_PAGE).to_string()),
            ];
            let (changes, _): (GitChanges, _) = self.get_json(Some(project), &path, &query).await?;
            let full = changes.changes.len() >= CHANGES_PAGE;
            items.extend(changes.changes.into_iter().filter_map(|change| change.item));
            if !full {
                return Ok(items);
            }
        }
        Err(PluginError::ApiError(format!(
            "Azure DevOps commit {commit_id} still has more changes after {CHANGES_PAGES} pages"
        )))
    }
}
