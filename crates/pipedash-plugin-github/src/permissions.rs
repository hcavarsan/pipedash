use std::collections::HashSet;
use std::time::Duration;

use chrono::Utc;
use octocrab::Octocrab;
use pipedash_plugin_api::{
    Permission,
    PermissionCheck,
    PermissionStatus,
    PluginError,
    PluginResult,
};
use secrecy::{
    ExposeSecret,
    SecretString,
};
use tracing::{
    debug,
    warn,
};

use crate::config;

type ScopeSet = HashSet<String>;

pub(crate) struct PermissionChecker {
    octocrab: Octocrab,
    token: SecretString,
}

impl PermissionChecker {
    const API_TIMEOUT_SECS: u64 = 10;

    const SCOPE_HIERARCHIES: &'static [(&'static str, &'static [&'static str])] = &[
        ("admin:org", &["admin:org"]),
        ("write:org", &["write:org", "admin:org"]),
        ("read:org", &["read:org", "write:org", "admin:org"]),
        ("admin:repo_hook", &["admin:repo_hook"]),
        ("write:repo_hook", &["write:repo_hook", "admin:repo_hook"]),
        (
            "read:repo_hook",
            &["read:repo_hook", "write:repo_hook", "admin:repo_hook"],
        ),
        ("admin:public_key", &["admin:public_key"]),
        (
            "write:public_key",
            &["write:public_key", "admin:public_key"],
        ),
        (
            "read:public_key",
            &["read:public_key", "write:public_key", "admin:public_key"],
        ),
        ("admin:gpg_key", &["admin:gpg_key"]),
        ("write:gpg_key", &["write:gpg_key", "admin:gpg_key"]),
        (
            "read:gpg_key",
            &["read:gpg_key", "write:gpg_key", "admin:gpg_key"],
        ),
        ("repo", &["repo", "public_repo"]),
        ("public_repo", &["public_repo", "repo"]),
        ("workflow", &["workflow"]),
    ];

    pub fn new(octocrab: Octocrab, token: SecretString) -> Self {
        Self { octocrab, token }
    }

    fn api_timeout() -> Duration {
        Duration::from_secs(Self::API_TIMEOUT_SECS)
    }

    fn build_http_client() -> PluginResult<reqwest::Client> {
        reqwest::Client::builder()
            .timeout(Self::api_timeout())
            .user_agent("pipedash")
            .build()
            .map_err(|e| PluginError::Internal(format!("Failed to build HTTP client: {e}")))
    }

    fn get_classic_pat_permissions() -> Vec<Permission> {
        vec![
            Permission {
                name: "repo".to_string(),
                description: "Repository access - 'repo' scope for private repositories, or 'public_repo' scope for public repositories only".to_string(),
                required: true,
            },
            Permission {
                name: "workflow".to_string(),
                description: "Trigger workflow dispatches and cancel running workflows".to_string(),
                required: false,
            },
            Permission {
                name: "read:org".to_string(),
                description: "List organizations and their repositories. Without this, only your personal repositories will be available.".to_string(),
                required: false,
            },
        ]
    }

    fn get_fine_grained_permissions() -> Vec<Permission> {
        vec![
            Permission {
                name: "Repository Metadata".to_string(),
                description: "Read repository metadata (name, description, visibility)".to_string(),
                required: true,
            },
            Permission {
                name: "Actions (Read)".to_string(),
                description: "View workflow runs, logs, and workflow definitions".to_string(),
                required: true,
            },
            Permission {
                name: "Actions (Write)".to_string(),
                description: "Trigger workflow dispatches and cancel running workflows".to_string(),
                required: false,
            },
            Permission {
                name: "Organization Members".to_string(),
                description: "List organizations for filtering repositories".to_string(),
                required: false,
            },
        ]
    }

    fn get_acceptable_scopes(minimal_permission: &str) -> Vec<String> {
        Self::SCOPE_HIERARCHIES
            .iter()
            .find(|(perm, _)| *perm == minimal_permission)
            .map(|(_, scopes)| scopes.iter().map(|s| s.to_string()).collect())
            .unwrap_or_else(|| vec![minimal_permission.to_string()])
    }

    fn has_permission(minimal_permission: &str, granted_scopes: &ScopeSet) -> bool {
        let acceptable_scopes = Self::get_acceptable_scopes(minimal_permission);
        acceptable_scopes
            .iter()
            .any(|scope| granted_scopes.contains(scope))
    }

    fn build_classic_pat_status(granted_scopes: &ScopeSet) -> PermissionStatus {
        let required_perms = Self::get_classic_pat_permissions();
        let permissions: Vec<PermissionCheck> = required_perms
            .iter()
            .map(|perm| {
                let granted = Self::has_permission(&perm.name, granted_scopes);
                debug!(
                    "Permission '{}': {}",
                    perm.name,
                    if granted { "granted" } else { "denied" }
                );
                PermissionCheck {
                    permission: perm.clone(),
                    granted,
                }
            })
            .collect();

        let all_granted = permissions
            .iter()
            .filter(|p| p.permission.required)
            .all(|p| p.granted);

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("token_type".to_string(), "classic_pat".to_string());

        PermissionStatus {
            permissions,
            all_granted,
            checked_at: Utc::now(),
            metadata,
        }
    }

    fn build_fine_grained_status(capabilities: &ScopeSet) -> PermissionStatus {
        let required_perms = Self::get_fine_grained_permissions();
        let permissions: Vec<PermissionCheck> = required_perms
            .iter()
            .map(|perm| {
                let granted = capabilities.contains(&perm.name);
                debug!(
                    "Permission '{}': {}",
                    perm.name,
                    if granted { "granted" } else { "denied" }
                );
                PermissionCheck {
                    permission: perm.clone(),
                    granted,
                }
            })
            .collect();

        let all_granted = permissions
            .iter()
            .filter(|p| p.permission.required)
            .all(|p| p.granted);

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("token_type".to_string(), "fine_grained".to_string());

        PermissionStatus {
            permissions,
            all_granted,
            checked_at: Utc::now(),
            metadata,
        }
    }

    pub async fn check_token_permissions(&self) -> PluginResult<PermissionStatus> {
        debug!("Starting permission check");
        match self.check_classic_token_scopes().await {
            Ok(status) => {
                debug!("Classic token scopes detected");
                Ok(status)
            }
            Err(e) => {
                warn!("Classic token check failed, trying fine-grained: {}", e);
                self.check_fine_grained_permissions().await
            }
        }
    }

    async fn check_classic_token_scopes(&self) -> PluginResult<PermissionStatus> {
        let client = Self::build_http_client()?;
        let response = client
            .get("https://api.github.com/user")
            .header(
                "Authorization",
                format!("token {}", self.token.expose_secret()),
            )
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to check token: {e}")))?;

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

        let granted_scopes: ScopeSet = scopes_header
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        debug!("Classic token scopes: {:?}", granted_scopes);
        Ok(Self::build_classic_pat_status(&granted_scopes))
    }

    async fn check_fine_grained_permissions(&self) -> PluginResult<PermissionStatus> {
        debug!("Checking fine-grained token permissions via capability tests");

        let capabilities = self.test_fine_grained_capabilities().await;
        debug!("Fine-grained token capabilities: {:?}", capabilities);

        Ok(Self::build_fine_grained_status(&capabilities))
    }

    async fn test_fine_grained_capabilities(&self) -> ScopeSet {
        let mut capabilities = Vec::new();

        debug!("Testing repository metadata access");
        if self.test_repository_access().await {
            debug!("Repository metadata: granted");
            capabilities.push("Repository Metadata".to_string());
            capabilities.push("Actions (Read)".to_string());
        } else {
            debug!("Repository metadata: denied");
        }

        debug!("Testing workflow write access");
        if self.test_workflow_access().await {
            debug!("Actions write: granted");
            capabilities.push("Actions (Write)".to_string());
        } else {
            debug!("Actions write: denied");
        }

        debug!("Testing org access");
        if self.test_org_access().await {
            debug!("Organization members: granted");
            capabilities.push("Organization Members".to_string());
        } else {
            debug!("Organization members: denied");
        }

        capabilities.into_iter().collect()
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

    async fn test_workflow_access(&self) -> bool {
        let repos = match tokio::time::timeout(
            Self::api_timeout(),
            self.octocrab
                .current()
                .list_repos_for_authenticated_user()
                .per_page(1)
                .send(),
        )
        .await
        {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                debug!("Workflow test - repo fetch failed: {}", e);
                return false;
            }
            Err(_) => {
                debug!("Workflow test - repo fetch timed out");
                return false;
            }
        };

        if let Some(repo) = repos.items.first() {
            if let Some(full_name) = &repo.full_name {
                if let Some((owner, repo_name)) = config::parse_repo(full_name) {
                    return match tokio::time::timeout(
                        Self::api_timeout(),
                        self.octocrab
                            .workflows(&owner, &repo_name)
                            .list()
                            .per_page(1)
                            .send(),
                    )
                    .await
                    {
                        Ok(Ok(_)) => true,
                        Ok(Err(e)) => {
                            debug!("Workflow list failed: {}", e);
                            false
                        }
                        Err(_) => {
                            debug!("Workflow list timed out");
                            false
                        }
                    };
                }
            }
        }

        false
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
