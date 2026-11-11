use std::collections::HashMap;

use futures::{
    stream,
    StreamExt,
    TryStreamExt,
};
use pipedash_plugin_api::pagination::paginate;
use pipedash_plugin_api::{
    AvailablePipeline,
    PaginatedResponse,
    PaginationParams,
    PluginResult,
};

use super::{
    CoolifyPlugin,
    CONCURRENCY,
};
use crate::{
    mapper,
    types,
};

impl CoolifyPlugin {
    pub(super) async fn placements(&self) -> PluginResult<HashMap<i64, mapper::Placement>> {
        let client = self.client()?;
        let projects = client.projects().await?;
        let details: Vec<(types::Project, types::ProjectDetail)> = stream::iter(projects)
            .map(|project| async move {
                client
                    .project(&project.uuid)
                    .await
                    .map(|detail| (project, detail))
            })
            .buffered(CONCURRENCY)
            .try_collect()
            .await?;

        Ok(details
            .into_iter()
            .flat_map(|(project, detail)| {
                detail.environments.into_iter().map(move |environment| {
                    (
                        environment.id,
                        mapper::Placement {
                            project: project.name.clone(),
                            environment: environment.name,
                        },
                    )
                })
            })
            .collect())
    }

    pub(super) async fn fetch_available_pipelines_filtered_impl(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        let (applications, placements) =
            futures::try_join!(client.applications(), self.placements())?;
        let search = search.map(|term| term.to_lowercase());
        let matching: Vec<AvailablePipeline> = applications
            .iter()
            .map(|app| {
                mapper::map_available_pipeline(
                    app,
                    app.environment_id.and_then(|id| placements.get(&id)),
                )
            })
            .filter(|app| org.is_none() || app.organization == org)
            .filter(|app| {
                search.as_ref().is_none_or(|term| {
                    app.name.to_lowercase().contains(term)
                        || app
                            .description
                            .as_ref()
                            .is_some_and(|text| text.to_lowercase().contains(term))
                })
            })
            .collect();

        paginate(matching, params.unwrap_or_default())
    }
}
