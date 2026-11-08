use std::collections::HashMap;

use pipedash_plugin_api::config::selected_items;
use pipedash_plugin_api::support::join_buffered;
use pipedash_plugin_api::utils::collect_partial;
use pipedash_plugin_api::*;

use super::BitbucketPlugin;
use crate::{
    mapper,
    types,
};

impl BitbucketPlugin {
    pub(super) async fn fetch_all_repositories(&self) -> PluginResult<Vec<types::Repository>> {
        let client = self.client()?;
        let mut all_repos = Vec::new();
        let mut page = 1;

        const MAX_PAGES: usize = 50;

        loop {
            let params = PaginationParams {
                page,
                page_size: 100,
            };
            let response = client.list_all_repositories(&params).await?;

            if response.items.is_empty() {
                break;
            }

            all_repos.extend(response.items);

            if !response.has_more || page >= MAX_PAGES {
                break;
            }
            page += 1;
        }

        if let Some(selected) = selected_items(&self.config) {
            Ok(all_repos
                .into_iter()
                .filter(|r| selected.contains(&r.full_name))
                .collect())
        } else {
            Ok(all_repos)
        }
    }

    pub(super) async fn fetch_organizations_impl(&self) -> PluginResult<Vec<Organization>> {
        let client = self.client()?;
        let workspaces = client.list_workspaces().await?;

        Ok(workspaces
            .into_iter()
            .map(|w| Organization {
                id: w.uuid,
                name: w.name,
                description: Some(w.slug),
            })
            .collect())
    }

    pub(super) async fn ids_with_ci_impl(&self, ids: Vec<String>) -> PluginResult<Vec<String>> {
        let client = self.client()?;
        let repos: HashMap<String, (String, String)> = self
            .fetch_all_repositories()
            .await?
            .into_iter()
            .map(|repo| (repo.full_name, (repo.workspace.slug, repo.slug)))
            .collect();
        Ok(retain_with_ci(ids, |id| {
            let repo = repos.get(&id).cloned();
            async move {
                let Some((workspace, slug)) = repo else {
                    return Ok(true);
                };
                if !client
                    .list_pipelines(&workspace, &slug, 1)
                    .await?
                    .is_empty()
                {
                    return Ok(true);
                }
                client.has_pipelines_file(&workspace, &slug).await
            }
        })
        .await)
    }

    pub(super) async fn fetch_available_pipelines_impl(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        let params = params.unwrap_or_default();

        let response = client.list_all_repositories(&params).await?;

        let available = response
            .items
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        Ok(PaginatedResponse::new(
            available,
            response.page,
            response.page_size,
            response.total_count,
        ))
    }

    pub(super) async fn fetch_available_pipelines_filtered_impl(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        let params = params.unwrap_or_default();

        let response = if let Some(workspace) = &org {
            client.list_repositories(workspace, &params).await?
        } else {
            client.list_all_repositories(&params).await?
        };

        let mut available: Vec<AvailablePipeline> = response
            .items
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        if let Some(search_term) = search {
            let search_lower = search_term.to_lowercase();
            available.retain(|p| {
                p.name.to_lowercase().contains(&search_lower)
                    || p.id.to_lowercase().contains(&search_lower)
            });
        }

        Ok(PaginatedResponse::new(
            available,
            params.page,
            params.page_size,
            response.total_count,
        ))
    }

    pub(super) async fn fetch_pipelines_impl(&self) -> PluginResult<Vec<Pipeline>> {
        let repos = self.fetch_all_repositories().await?;

        let requests: Vec<_> = repos
            .iter()
            .map(|repo| self.repository_pipeline(repo))
            .collect();
        let results: Vec<_> = join_buffered(requests, 8).await;

        Ok(collect_partial(
            results,
            |error| tracing::warn!(%error, item = "repository", "Skipping failed Bitbucket item"),
        )?
        .into_iter()
        .flatten()
        .collect())
    }

    async fn repository_pipeline(
        &self, repo: &types::Repository,
    ) -> PluginResult<Option<Pipeline>> {
        let client = self.client()?;
        let pipelines = client
            .list_pipelines(&repo.workspace.slug, &repo.slug, 1)
            .await?;
        let latest = pipelines.first();
        if latest.is_none()
            && !client
                .has_pipelines_file(&repo.workspace.slug, &repo.slug)
                .await
                .unwrap_or(true)
        {
            return Ok(None);
        }
        Ok(Some(mapper::map_pipeline(repo, latest)))
    }
}
