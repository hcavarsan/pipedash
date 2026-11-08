use pipedash_plugin_api::support::retain_matching;
use pipedash_plugin_api::{
    AvailablePipeline,
    PaginatedResponse,
    PaginationParams,
    PluginResult,
};

use super::BuildkiteClient;

const PAGE_SIZE: usize = 100;

async fn fetch_org_pipelines_into(
    client: &BuildkiteClient, org_slug: &str, all: &mut Vec<AvailablePipeline>,
) -> PluginResult<()> {
    let mut page = 1;
    loop {
        let pipelines = client
            .fetch_org_pipelines(org_slug.to_string(), page, PAGE_SIZE)
            .await?;
        if pipelines.is_empty() {
            return Ok(());
        }
        let is_last_page = pipelines.len() < PAGE_SIZE;
        all.extend(pipelines);
        if is_last_page {
            return Ok(());
        }
        page += 1;
    }
}

async fn fetch_all_orgs_pipelines(
    client: &BuildkiteClient,
) -> PluginResult<Vec<AvailablePipeline>> {
    let organizations = client.fetch_organizations().await?;
    let mut all = Vec::new();

    for org in organizations {
        match fetch_org_pipelines_into(client, &org.slug, &mut all).await {
            Ok(()) => {}
            Err(e) if e.is_transient() => return Err(e),
            Err(e) => {
                tracing::warn!(org = %org.slug, error = %e, "Failed to fetch Buildkite pipelines for org");
            }
        }
    }
    Ok(all)
}

fn page_of(
    all: Vec<AvailablePipeline>, params: PaginationParams,
) -> PaginatedResponse<AvailablePipeline> {
    let total_count = all.len();
    let start = params.calculate_offset().unwrap_or(0).min(total_count);
    let end = (start + params.page_size).min(total_count);
    PaginatedResponse::new(
        all[start..end].to_vec(),
        params.page,
        params.page_size,
        total_count,
    )
}

pub(crate) async fn fetch_all_available_pipelines(
    client: &BuildkiteClient, params: Option<PaginationParams>,
) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
    let params = params.unwrap_or_default();
    let all = fetch_all_orgs_pipelines(client).await?;
    Ok(page_of(all, params))
}

pub(crate) async fn fetch_available_pipelines_filtered(
    client: &BuildkiteClient, org: Option<String>, search: Option<String>,
    params: Option<PaginationParams>,
) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
    let params = params.unwrap_or_default();

    let mut all_pipelines = match org {
        Some(org_slug) => {
            let mut all = Vec::new();
            fetch_org_pipelines_into(client, &org_slug, &mut all).await?;
            all
        }
        None => fetch_all_orgs_pipelines(client).await?,
    };

    if let Some(search_term) = search {
        retain_matching(&mut all_pipelines, &search_term);
    }

    Ok(page_of(all_pipelines, params))
}
