use pipedash_plugin_api::http::send;
use pipedash_plugin_api::{
    check_response,
    CommitSha,
    PluginError,
    PluginResult,
};
use reqwest::Method;

use super::BitbucketClient;
use crate::types::{
    Commit,
    CommitHash,
    DiffStat,
    PaginatedResponse as BitbucketPaginatedResponse,
};

const MAX_PAGES: usize = 10;

impl BitbucketClient {
    pub async fn get_commit(
        &self, workspace: &str, repo_slug: &str, rev: &str,
    ) -> PluginResult<Commit> {
        self.get_json(&format!(
            "{}/repositories/{workspace}/{repo_slug}/commit/{}",
            self.api_url,
            urlencoding::encode(rev)
        ))
        .await
    }

    pub async fn commits_between(
        &self, workspace: &str, repo_slug: &str, include: &CommitSha, exclude: &CommitSha,
    ) -> PluginResult<Vec<CommitHash>> {
        self.collect_pages(format!(
            "{}/repositories/{workspace}/{repo_slug}/commits?include={}&exclude={}&pagelen=100",
            self.api_url,
            include.as_str(),
            exclude.as_str()
        ))
        .await
    }

    pub async fn commit_diffstat(
        &self, workspace: &str, repo_slug: &str, sha: &CommitSha,
    ) -> PluginResult<Vec<DiffStat>> {
        self.collect_pages(format!(
            "{}/repositories/{workspace}/{repo_slug}/diffstat/{}?pagelen=500",
            self.api_url,
            sha.as_str()
        ))
        .await
    }

    pub async fn commit_diff(
        &self, workspace: &str, repo_slug: &str, sha: &CommitSha,
    ) -> PluginResult<String> {
        let url = Self::parse_url(&format!(
            "{}/repositories/{workspace}/{repo_slug}/diff/{}",
            self.api_url,
            sha.as_str()
        ))?;
        self.retry_policy
            .retry(|| async {
                let response =
                    check_response(send(&url, self.authorized(Method::GET, &url)).await?).await?;
                response.text().await.map_err(|e| {
                    PluginError::ApiError(format!(
                        "Failed to read Bitbucket diff: {}",
                        e.without_url()
                    ))
                })
            })
            .await
    }

    async fn collect_pages<T: serde::de::DeserializeOwned>(
        &self, first_url: String,
    ) -> PluginResult<Vec<T>> {
        let mut items = Vec::new();
        let mut next_url = Some(first_url);

        for _ in 0..MAX_PAGES {
            let Some(url) = next_url.take() else {
                return Ok(items);
            };
            let page: BitbucketPaginatedResponse<T> = self.get_json(&url).await?;
            items.extend(page.values);
            next_url = page.next;
        }

        match next_url {
            None => Ok(items),
            Some(_) => Err(PluginError::ApiError(format!(
                "Bitbucket result spans more than {MAX_PAGES} pages"
            ))),
        }
    }
}
