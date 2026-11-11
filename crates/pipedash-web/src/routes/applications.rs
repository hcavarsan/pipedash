use axum::extract::{
    Path,
    Query,
    State,
};
use axum::http::StatusCode;
use axum::routing::{
    delete,
    get,
    post,
};
use axum::{
    Json,
    Router,
};
use chrono::{
    DateTime,
    Utc,
};
use pipedash_core::domain::application::dto::{
    ApplicationDetail,
    ApplicationList,
    ApplicationListQuery,
    ApplicationRef,
    ApplicationSort,
    ChangesPage,
    CommitLocations,
    DeployChanges,
    Suggestion,
};
use pipedash_core::domain::application::metrics::{
    ApplicationMetrics,
    ApplicationMetricsQuery,
};
use pipedash_core::domain::application::overrides::{
    ApplicationEdit,
    CreateApplicationInput,
};
use pipedash_core::domain::{
    AggregationPeriod,
    PipelineStatus,
    SortDirection,
};
use serde::de::value::StrDeserializer;
use serde::de::{
    Deserializer,
    Error,
};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

fn status_list<'de, D>(deserializer: D) -> Result<Vec<PipelineStatus>, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)?
        .split(',')
        .filter(|status| !status.is_empty())
        .map(|status| PipelineStatus::deserialize(StrDeserializer::<D::Error>::new(status)))
        .collect()
}

fn id_list<'de, D>(deserializer: D) -> Result<Vec<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)?
        .split(',')
        .filter(|id| !id.is_empty())
        .map(|id| id.parse().map_err(D::Error::custom))
        .collect()
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct ListQuery {
    search: Option<String>,
    #[serde(deserialize_with = "status_list")]
    health: Vec<PipelineStatus>,
    behind_only: bool,
    #[serde(deserialize_with = "id_list")]
    provider_ids: Vec<i64>,
    with_suggestions: bool,
    include_hidden: bool,
    sort: ApplicationSort,
    direction: SortDirection,
}

impl From<ListQuery> for ApplicationListQuery {
    fn from(query: ListQuery) -> Self {
        Self {
            search: query.search,
            health: query.health,
            behind_only: query.behind_only,
            provider_ids: query.provider_ids,
            with_suggestions: query.with_suggestions,
            include_hidden: query.include_hidden,
            sort: query.sort,
            direction: query.direction,
        }
    }
}

#[derive(Deserialize)]
pub struct ChangesQuery {
    cursor: Option<String>,
    limit: usize,
}

#[derive(Deserialize)]
pub struct MetricsQuery {
    group_key: Option<String>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    period: AggregationPeriod,
}

#[derive(Deserialize)]
pub struct AcceptRequest {
    ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct RunQuery {
    pipeline_id: String,
    run_number: i64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_applications).post(create_application))
        .route("/suggestions", get(suggestions))
        .route("/suggestions/accept", post(accept_suggestions))
        .route("/commit-locations", get(commit_locations))
        .route("/deploy-changes", get(deploy_changes))
        .route("/by-pipeline/{pipeline_id}", get(pipeline_applications))
        .route("/{id}", get(get_application).delete(delete_application))
        .route("/{id}/changes", get(changes))
        .route("/{id}/metrics", get(metrics))
        .route("/{id}/edits", post(edit_application))
        .route("/{id}/overrides/{override_id}", delete(revert_override))
}

async fn list_applications(
    State(state): State<AppState>, Query(query): Query<ListQuery>,
) -> ApiResult<Json<ApplicationList>> {
    Ok(Json(
        state
            .correlation_service()
            .await?
            .list(query.into())
            .await?,
    ))
}

async fn get_application(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<ApplicationDetail>> {
    Ok(Json(state.correlation_service().await?.detail(id).await?))
}

async fn changes(
    State(state): State<AppState>, Path(id): Path<i64>, Query(query): Query<ChangesQuery>,
) -> ApiResult<Json<ChangesPage>> {
    Ok(Json(
        state
            .correlation_service()
            .await?
            .changes(id, query.cursor, query.limit)
            .await?,
    ))
}

async fn metrics(
    State(state): State<AppState>, Path(id): Path<i64>, Query(query): Query<MetricsQuery>,
) -> ApiResult<Json<ApplicationMetrics>> {
    let query = ApplicationMetricsQuery {
        application_id: id,
        group_key: query.group_key,
        from: query.from,
        to: query.to,
        period: query.period,
    };
    Ok(Json(
        state.correlation_service().await?.metrics(query).await?,
    ))
}

async fn create_application(
    State(state): State<AppState>, Json(input): Json<CreateApplicationInput>,
) -> ApiResult<(StatusCode, Json<ApplicationDetail>)> {
    let detail = state
        .correlation_service()
        .await?
        .create_application(input, None)
        .await?;
    Ok((StatusCode::CREATED, Json(detail)))
}

async fn edit_application(
    State(state): State<AppState>, Path(id): Path<i64>, Json(edit): Json<ApplicationEdit>,
) -> ApiResult<Json<ApplicationDetail>> {
    Ok(Json(
        state
            .correlation_service()
            .await?
            .apply_edit(id, edit, None)
            .await?,
    ))
}

async fn delete_application(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<()> {
    Ok(state
        .correlation_service()
        .await?
        .delete_application(id)
        .await?)
}

async fn revert_override(
    State(state): State<AppState>, Path((id, override_id)): Path<(i64, i64)>,
) -> ApiResult<Json<ApplicationDetail>> {
    Ok(Json(
        state
            .correlation_service()
            .await?
            .revert_override(id, override_id)
            .await?,
    ))
}

async fn suggestions(State(state): State<AppState>) -> ApiResult<Json<Vec<Suggestion>>> {
    Ok(Json(
        state.correlation_service().await?.suggestions().await?,
    ))
}

async fn accept_suggestions(
    State(state): State<AppState>, Json(request): Json<AcceptRequest>,
) -> ApiResult<()> {
    Ok(state
        .correlation_service()
        .await?
        .accept_suggestions(request.ids, None)
        .await?)
}

async fn pipeline_applications(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
) -> ApiResult<Json<Vec<ApplicationRef>>> {
    Ok(Json(
        state
            .correlation_service()
            .await?
            .pipeline_applications(&pipeline_id)
            .await?,
    ))
}

async fn commit_locations(
    State(state): State<AppState>, Query(query): Query<RunQuery>,
) -> ApiResult<Json<CommitLocations>> {
    Ok(Json(
        state
            .correlation_service()
            .await?
            .commit_locations(&query.pipeline_id, query.run_number)
            .await?,
    ))
}

async fn deploy_changes(
    State(state): State<AppState>, Query(query): Query<RunQuery>,
) -> ApiResult<Json<DeployChanges>> {
    Ok(Json(
        state
            .correlation_service()
            .await?
            .deploy_changes(&query.pipeline_id, query.run_number)
            .await?,
    ))
}

#[cfg(test)]
mod tests;
