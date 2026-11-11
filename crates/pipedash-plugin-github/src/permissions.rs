use std::collections::HashSet;
use std::time::Duration;

use octocrab::Octocrab;
use pipedash_plugin_api::config::comma_separated;
use pipedash_plugin_api::http::sensitive_header;
use pipedash_plugin_api::{
    check_response,
    map_send_error,
    PermissionStatus,
    PluginError,
    PluginResult,
};
use reqwest::header::AUTHORIZATION;
use reqwest::Url;
use secrecy::{
    ExposeSecret,
    SecretString,
};
use tracing::{
    debug,
    warn,
};

mod scopes;

type ScopeSet = HashSet<String>;

pub(crate) struct PermissionChecker {
    octocrab: Octocrab,
    token: SecretString,
    http_client: reqwest::Client,
    user_url: Url,
}

impl PermissionChecker {
    const API_TIMEOUT_SECS: u64 = 5; // GitHub API responds in <1s normally

    pub fn new(
        octocrab: Octocrab, http_client: reqwest::Client, token: SecretString, api_url: &str,
    ) -> PluginResult<Self> {
        let user_url = Url::parse(&format!("{}/user", api_url.trim_end_matches('/')))
            .map_err(|e| PluginError::InvalidConfig(format!("Invalid API URL '{api_url}': {e}")))?;
        Ok(Self {
            octocrab,
            token,
            http_client,
            user_url,
        })
    }

    fn api_timeout() -> Duration {
        Duration::from_secs(Self::API_TIMEOUT_SECS)
    }

    pub async fn check_token_permissions(
        &self, repositories: &[String],
    ) -> PluginResult<PermissionStatus> {
        debug!("Starting permission check");
        match self.check_classic_token_scopes().await {
            Ok(status) => {
                debug!("Classic token scopes detected");
                Ok(status)
            }
            Err(e) if e.is_transient() => Err(e),
            Err(e) => {
                warn!("Classic token check failed, trying fine-grained: {}", e);
                self.check_fine_grained_permissions(repositories).await
            }
        }
    }

    async fn check_classic_token_scopes(&self) -> PluginResult<PermissionStatus> {
        let response = self
            .http_client
            .get(self.user_url.clone())
            .header(
                AUTHORIZATION,
                sensitive_header(
                    &format!("token {}", self.token.expose_secret()),
                    "GitHub authorization",
                )?,
            )
            .timeout(Self::api_timeout())
            .send()
            .await
            .map_err(|e| map_send_error(e, &self.user_url))?;
        let response = check_response(response).await?;

        debug!("Classic token check response status: {}", response.status());

        let scopes_header = response
            .headers()
            .get("X-OAuth-Scopes")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                PluginError::ApiError(
                    "Token is not a classic PAT (no X-OAuth-Scopes header)".to_string(),
                )
            })?;

        let granted_scopes: ScopeSet = comma_separated(scopes_header).map(str::to_string).collect();

        debug!("Classic token scopes: {:?}", granted_scopes);
        Ok(Self::build_classic_pat_status(&granted_scopes))
    }

    async fn check_fine_grained_permissions(
        &self, repositories: &[String],
    ) -> PluginResult<PermissionStatus> {
        debug!("Checking fine-grained token permissions via capability tests");

        let mut capabilities = self.test_fine_grained_capabilities().await;
        if self.test_contents_access(repositories).await? {
            capabilities.insert("Contents (Read)".to_string());
        }
        debug!("Fine-grained token capabilities: {:?}", capabilities);

        Ok(Self::build_fine_grained_status(&capabilities))
    }

    async fn test_fine_grained_capabilities(&self) -> ScopeSet {
        let mut capabilities = Vec::new();

        debug!("Testing repository metadata access");
        if self.test_repository_access().await {
            debug!("Repository metadata: granted");
            capabilities.push("Repository Metadata".to_string());
        } else {
            debug!("Repository metadata: denied");
        }

        debug!("Testing org access");
        if self.test_org_access().await {
            debug!("Organization members: granted");
            capabilities.push("Organization members and teams (Read)".to_string());
        } else {
            debug!("Organization members: denied");
        }

        capabilities.into_iter().collect()
    }

    async fn test_contents_access(&self, repositories: &[String]) -> PluginResult<bool> {
        let mut discovered = Vec::new();
        if repositories.is_empty() {
            let available: Vec<serde_json::Value> = tokio::time::timeout(
                Self::api_timeout(),
                self.octocrab.get("/user/repos", Some(&[("per_page", "1")])),
            )
            .await
            .map_err(|_| {
                PluginError::NetworkError(
                    "Repository lookup for Contents (Read) timed out".to_string(),
                )
            })?
            .map_err(|e| {
                crate::client::map_octocrab_error(
                    "Failed to find a repository for the Contents (Read) check",
                    e,
                )
            })?;
            if let Some(name) = available
                .first()
                .and_then(|repo| repo["full_name"].as_str())
            {
                discovered.push(name.to_string());
            }
            if discovered.is_empty() {
                return Ok(false);
            }
        }

        for repository in repositories.iter().chain(discovered.iter()) {
            let (owner, repo) = repository
                .split_once('/')
                .filter(|(owner, repo)| {
                    !owner.is_empty() && !repo.is_empty() && !repo.contains('/')
                })
                .ok_or_else(|| {
                    PluginError::InvalidConfig(format!("Invalid repository: {repository}"))
                })?;
            let response: Result<serde_json::Value, _> = tokio::time::timeout(
                Self::api_timeout(),
                self.octocrab
                    .get(format!("/repos/{owner}/{repo}/contents"), None::<&()>),
            )
            .await
            .map_err(|_| {
                PluginError::NetworkError(format!(
                    "Contents (Read) check for {repository} timed out"
                ))
            })?;
            if let Err(error) = response {
                let error =
                    crate::client::map_octocrab_error("Failed to check Contents (Read)", error);
                if matches!(
                    error,
                    PluginError::PermissionDenied(_) | PluginError::PipelineNotFound(_)
                ) {
                    return Ok(false);
                }
                return Err(error);
            }
        }
        Ok(true)
    }

    async fn test_repository_access(&self) -> bool {
        match tokio::time::timeout(
            Self::api_timeout(),
            self.octocrab
                .current()
                .list_repos_for_authenticated_user()
                .per_page(1)
                .send(),
        )
        .await
        {
            Ok(Ok(_)) => true,
            Ok(Err(e)) => {
                debug!("Repository access test failed: {}", e);
                false
            }
            Err(_) => {
                debug!("Repository access test timed out");
                false
            }
        }
    }

    async fn test_org_access(&self) -> bool {
        match tokio::time::timeout(
            Self::api_timeout(),
            self.octocrab
                .current()
                .list_org_memberships_for_authenticated_user()
                .per_page(1)
                .send(),
        )
        .await
        {
            Ok(Ok(_)) => true,
            Ok(Err(e)) => {
                debug!("Org access test failed: {}", e);
                false
            }
            Err(_) => {
                debug!("Org access test timed out");
                false
            }
        }
    }
}
