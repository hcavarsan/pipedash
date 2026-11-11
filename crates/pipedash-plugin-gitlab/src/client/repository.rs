use pipedash_plugin_api::{
    CommitSha,
    PluginResult,
    RepoId,
};

use super::GitLabClient;
use crate::types::{
    Commit,
    CommitRef,
    Compare,
    Deployment,
    Environment,
    FileDiff,
};

const ENVIRONMENT_LIMIT: usize = 500;
const DIFF_FILE_LIMIT: usize = 1000;

impl GitLabClient {
    pub async fn get_commit(&self, repo: &RepoId, rev: &str) -> PluginResult<Commit> {
        self.get_json(format!(
            "{}/repository/commits/{}",
            self.repository_url(repo),
            urlencoding::encode(rev)
        ))
        .await
    }

    pub async fn commits_between(
        &self, repo: &RepoId, from: &CommitSha, to: &CommitSha,
    ) -> PluginResult<Vec<CommitRef>> {
        let compare: Compare = self
            .get_json(format!(
                "{}/repository/compare?from={}&to={}",
                self.repository_url(repo),
                from.as_str(),
                to.as_str()
            ))
            .await?;
        Ok(compare.commits)
    }

    pub async fn commit_diff(&self, repo: &RepoId, sha: &CommitSha) -> PluginResult<Vec<FileDiff>> {
        self.get_paged(
            format!(
                "{}/repository/commits/{}/diff",
                self.repository_url(repo),
                sha.as_str()
            ),
            DIFF_FILE_LIMIT,
        )
        .await
    }

    pub async fn project_deployments(
        &self, project_id: i64, limit: usize,
    ) -> PluginResult<Vec<Deployment>> {
        self.get_paged(
            format!(
                "{}/projects/{}/deployments?order_by=id&sort=desc",
                self.api_url, project_id
            ),
            limit,
        )
        .await
    }

    pub async fn project_environments(&self, project_id: i64) -> PluginResult<Vec<Environment>> {
        self.get_paged(
            format!("{}/projects/{}/environments", self.api_url, project_id),
            ENVIRONMENT_LIMIT,
        )
        .await
    }

    fn repository_url(&self, repo: &RepoId) -> String {
        format!(
            "{}/projects/{}",
            self.api_url,
            urlencoding::encode(repo.path())
        )
    }
}
