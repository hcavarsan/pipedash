use std::collections::HashMap;

use pipedash_plugin_api::pagination::paginate;
use pipedash_plugin_api::{
    AvailablePipeline,
    Organization,
    PaginatedResponse,
    PaginationParams,
    PluginResult,
};
use tracing::debug;

use super::{
    ArgocdPlugin,
    DEFAULT_PAGE_SIZE,
};
use crate::config;
use crate::mapper::catalog::map_available_pipeline;

fn page_of(
    pipelines: Vec<AvailablePipeline>, params: Option<PaginationParams>,
) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
    let Some(params) = params else {
        let total = pipelines.len();
        return Ok(PaginatedResponse::new(
            pipelines,
            1,
            DEFAULT_PAGE_SIZE,
            total,
        ));
    };
    let defaults = PaginationParams::default();
    paginate(
        pipelines,
        PaginationParams {
            page: params.page.max(defaults.page),
            page_size: if params.page_size == 0 {
                defaults.page_size
            } else {
                params.page_size
            },
        },
    )
}

impl ArgocdPlugin {
    pub(super) async fn fetch_available_pipelines_impl(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        debug!("Fetching available pipelines");
        let client = self.client()?;

        let mut apps = client.list_applications(None).await?;
        debug!(
            total_apps = apps.len(),
            "Retrieved applications from ArgoCD"
        );

        if let Some(orgs_filter) = &self.organizations_filter {
            debug!(?orgs_filter, "Filtering applications by organizations");
            apps.retain(|app| {
                let git_org = config::extract_git_org(&app.spec.primary_source().repo_url);
                orgs_filter.contains(&git_org)
            });
            debug!(
                filtered_apps = apps.len(),
                "Applications after organization filtering"
            );
        }

        let available_pipelines: Vec<AvailablePipeline> =
            apps.iter().map(map_available_pipeline).collect();
        let response = page_of(available_pipelines, params)?;

        debug!(
            pipeline_count = response.items.len(),
            total = response.total_count,
            page = response.page,
            page_size = response.page_size,
            "Mapped and paginated available pipelines"
        );

        Ok(response)
    }

    pub(super) async fn fetch_organizations_impl(&self) -> PluginResult<Vec<Organization>> {
        debug!("Fetching Git organizations from applications");
        let client = self.client()?;

        let apps = client.list_applications(None).await?;

        debug!(
            app_count = apps.len(),
            "Extracting organizations from applications"
        );

        let mut orgs_map: HashMap<String, Organization> = HashMap::new();

        for app in apps {
            let git_org = config::extract_git_org(&app.spec.primary_source().repo_url);

            if git_org == "unknown" || git_org.is_empty() {
                continue;
            }

            if !orgs_map.contains_key(&git_org) {
                orgs_map.insert(
                    git_org.clone(),
                    Organization {
                        id: git_org.clone(),
                        name: git_org.clone(),
                        description: Some(format!("Git Organization: {}", git_org)),
                    },
                );
            }
        }

        let organizations: Vec<Organization> = orgs_map.into_values().collect();
        debug!(
            org_count = organizations.len(),
            org_names = ?organizations.iter().map(|o| &o.name).collect::<Vec<_>>(),
            "Extracted unique Git organizations"
        );

        Ok(organizations)
    }

    pub(super) async fn fetch_available_pipelines_filtered_impl(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        debug!(?org, ?search, "Fetching filtered available pipelines");

        let client = self.client()?;

        let apps = client.list_applications(None).await?;

        debug!(
            total_apps = apps.len(),
            "Retrieved applications for filtering"
        );

        let filtered_apps: Vec<_> = apps
            .into_iter()
            .filter(|app| {
                let org_match = org.as_ref().is_none_or(|o| {
                    let git_org = config::extract_git_org(&app.spec.primary_source().repo_url);
                    git_org == *o
                });

                let search_match = search.as_ref().is_none_or(|s| {
                    app.metadata.name.to_lowercase().contains(&s.to_lowercase())
                        || app
                            .spec
                            .primary_source()
                            .repo_url
                            .to_lowercase()
                            .contains(&s.to_lowercase())
                });

                org_match && search_match
            })
            .collect();

        debug!(
            filtered_apps = filtered_apps.len(),
            "Applications after applying filters"
        );

        let available_pipelines: Vec<AvailablePipeline> =
            filtered_apps.iter().map(map_available_pipeline).collect();
        let response = page_of(available_pipelines, params)?;

        debug!(
            pipeline_count = response.items.len(),
            total = response.total_count,
            page = response.page,
            page_size = response.page_size,
            "Mapped and paginated filtered pipelines"
        );

        Ok(response)
    }
}
