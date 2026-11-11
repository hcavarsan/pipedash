use std::collections::HashMap;

use pipedash_plugin_api::{
    AvailablePipeline,
    Organization,
    PaginatedResponse,
    PaginationParams,
    PluginError,
    PluginResult,
};

use super::{
    map_octocrab_error,
    GitHubClient,
};
use crate::config;

fn available_pipeline(repo: &octocrab::models::Repository) -> PluginResult<AvailablePipeline> {
    let full_name = repo
        .full_name
        .as_ref()
        .ok_or_else(|| PluginError::ApiError("Repository missing full_name".to_string()))?;

    let (owner, repo_name) = match config::parse_repo(full_name) {
        Some((o, r)) => (Some(o), Some(r)),
        None => (None, None),
    };

    Ok(AvailablePipeline {
        id: full_name.clone(),
        name: full_name.clone(),
        description: repo.description.clone(),
        organization: owner,
        repository: repo_name,
    })
}

fn matches_search(pipeline: &AvailablePipeline, search_lower: &str) -> bool {
    pipeline.name.to_lowercase().contains(search_lower)
        || pipeline.id.to_lowercase().contains(search_lower)
        || pipeline
            .description
            .as_ref()
            .is_some_and(|d| d.to_lowercase().contains(search_lower))
}

impl GitHubClient {
    async fn detect_organizations_from_repos(&self) -> PluginResult<Vec<Organization>> {
        tracing::debug!("Detecting organizations from accessible repositories");

        let repos = self
            .octocrab
            .current()
            .list_repos_for_authenticated_user()
            .per_page(100)
            .send()
            .await
            .map_err(|e| map_octocrab_error("Failed to fetch accessible repositories", e))?;

        let mut org_map: HashMap<String, Organization> = HashMap::new();

        for owner in repos.items.into_iter().filter_map(|repo| repo.owner) {
            org_map
                .entry(owner.login.clone())
                .or_insert_with(|| Organization {
                    id: owner.login.clone(),
                    name: owner.login,
                    description: None,
                });
        }

        let organizations: Vec<_> = org_map.into_values().collect();
        tracing::debug!(
            "Detected {} organizations from repositories",
            organizations.len()
        );

        Ok(organizations)
    }

    async fn organizations_or_repositories(
        &self, user_login: &str,
    ) -> PluginResult<Vec<Organization>> {
        let memberships = self
            .octocrab
            .current()
            .list_org_memberships_for_authenticated_user()
            .per_page(100)
            .send()
            .await;

        match memberships {
            Ok(organizations) => {
                tracing::debug!("Successfully fetched organization memberships");
                return Ok(organizations
                    .items
                    .into_iter()
                    .map(|org| Organization {
                        id: org.organization.login.clone(),
                        name: org.organization.login.clone(),
                        description: org.organization.description.clone(),
                    })
                    .collect());
            }
            Err(e) => tracing::warn!(
                "Cannot fetch organization memberships ({}). Falling back to repository-based detection.",
                e
            ),
        }

        match self.detect_organizations_from_repos().await {
            Ok(detected) => {
                tracing::debug!("Successfully detected organizations from repositories");
                Ok(detected
                    .into_iter()
                    .filter(|org| org.id != user_login)
                    .collect())
            }
            Err(detect_err) => {
                tracing::warn!(
                    "Failed to detect organizations from repositories: {}. \
                    Only personal account will be available.",
                    detect_err
                );
                Ok(Vec::new())
            }
        }
    }

    pub async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        self.retry_policy
            .retry(|| async {
                let user = self
                    .octocrab
                    .current()
                    .user()
                    .await
                    .map_err(|e| map_octocrab_error("Failed to fetch current user", e))?;

                let mut orgs = vec![Organization {
                    id: user.login.clone(),
                    name: user.login.clone(),
                    description: user.name.clone(),
                }];
                orgs.extend(self.organizations_or_repositories(&user.login).await?);

                Ok(orgs)
            })
            .await
    }

    pub async fn fetch_all_repositories(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        self.fetch_available_pipelines_filtered(None, None, params)
            .await
    }

    async fn user_repositories(
        &self, context: &str, per_page: u8, page: u8,
    ) -> PluginResult<octocrab::Page<octocrab::models::Repository>> {
        self.retry_policy
            .retry(|| async {
                self.octocrab
                    .current()
                    .list_repos_for_authenticated_user()
                    .per_page(per_page)
                    .page(page)
                    .send()
                    .await
                    .map_err(|e| map_octocrab_error(context, e))
            })
            .await
    }

    async fn organization_repositories(
        &self, org_name: &str, per_page: u8, page: u8,
    ) -> PluginResult<(octocrab::Page<octocrab::models::Repository>, Option<String>)> {
        let user = self
            .retry_policy
            .retry(|| async {
                self.octocrab
                    .current()
                    .user()
                    .await
                    .map_err(|e| map_octocrab_error("Failed to fetch current user", e))
            })
            .await?;

        if org_name == user.login {
            let repos = self
                .user_repositories("Failed to fetch user repositories", per_page, page)
                .await?;
            return Ok((repos, Some(user.login)));
        }

        let repos = self
            .retry_policy
            .retry(|| async {
                self.octocrab
                    .orgs(org_name)
                    .list_repos()
                    .per_page(per_page)
                    .page(page)
                    .send()
                    .await
                    .map_err(|e| map_octocrab_error("Failed to fetch org repositories", e))
            })
            .await?;
        Ok((repos, None))
    }

    pub async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let params = params.unwrap_or_default();
        let github_page = params.page.min(255) as u8;
        let per_page = params.page_size.min(100) as u8;

        let (repos, personal_login) = match org {
            Some(org_name) => {
                self.organization_repositories(&org_name, per_page, github_page)
                    .await?
            }
            None => (
                self.user_repositories("Failed to fetch repositories", per_page, github_page)
                    .await?,
                None,
            ),
        };

        let mut all_repos = Vec::new();
        for repo in &repos.items {
            let pipeline = available_pipeline(repo)?;
            if personal_login
                .as_deref()
                .is_some_and(|login| pipeline.organization.as_deref() != Some(login))
            {
                continue;
            }
            all_repos.push(pipeline);
        }

        if let Some(search_term) = search {
            let search_lower = search_term.to_lowercase();
            all_repos.retain(|p| matches_search(p, &search_lower));
        }

        let total_count = repos
            .total_count
            .map(|t| t as usize)
            .unwrap_or(all_repos.len());
        Ok(PaginatedResponse::new(
            all_repos,
            params.page,
            params.page_size,
            total_count,
        ))
    }
}
