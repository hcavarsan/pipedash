use pipedash_plugin_api::{
    CommitSha,
    PluginResult,
};

use super::GiteaClient;
use crate::types::{
    Commit,
    Compare,
};

impl GiteaClient {
    pub async fn get_commit(&self, owner: &str, repo: &str, rev: &str) -> PluginResult<Commit> {
        self.get(
            &format!(
                "/repos/{owner}/{repo}/git/commits/{}",
                urlencoding::encode(rev)
            ),
            &[],
        )
        .await
    }

    pub async fn compare(
        &self, owner: &str, repo: &str, base: &CommitSha, head: &CommitSha,
    ) -> PluginResult<Compare> {
        self.get(
            &format!(
                "/repos/{owner}/{repo}/compare/{}...{}",
                base.as_str(),
                head.as_str()
            ),
            &[
                ("files", "false".to_string()),
                ("verification", "false".to_string()),
            ],
        )
        .await
    }
}
