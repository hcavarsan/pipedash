use axum::{
    body::Body,
    extract::{
        Path,
        Query,
        State,
    },
    http::header::{
        CACHE_CONTROL,
        CONTENT_DISPOSITION,
        CONTENT_TYPE,
        REFERRER_POLICY,
    },
    response::{
        IntoResponse,
        Response,
    },
    routing::{
        get,
        post,
    },
    Json,
    Router,
};
use pipedash_core::domain::{
    LogLinesPage,
    LogLinesQuery,
    LogSearchQuery,
    LogSearchResult,
    RunLogDocument,
    RunLogTree,
};
use serde::{
    Deserialize,
    Serialize,
};
use tower_http::compression::CompressionLayer;

use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

const MAX_FILE_NAME_BYTES: usize = 200;

#[derive(Debug, Deserialize)]
struct OpenQuery {
    #[serde(default)]
    retry: bool,
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    revision: i64,
    session: String,
}

#[derive(Debug, Deserialize)]
struct TicketRequest {
    revision: i64,
    file_name: String,
}

#[derive(Debug, Serialize)]
struct TicketResponse {
    ticket: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/{id}/runs/{run_number}/logs", get(get_run_log_tree))
        .route("/{id}/runs/{run_number}/logs/{node_id}", get(open_run_log))
        .route(
            "/{id}/runs/{run_number}/logs/{node_id}/lines",
            get(read_run_log_lines),
        )
        .route(
            "/{id}/runs/{run_number}/logs/{node_id}/search",
            post(search_run_log),
        )
        .route(
            "/{id}/runs/{run_number}/logs/{node_id}/download-ticket",
            post(issue_download_ticket),
        )
        .layer(CompressionLayer::new())
}

async fn get_run_log_tree(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<Json<RunLogTree>> {
    let service = state.log_service().await?;
    Ok(Json(service.run_log_tree(&pipeline_id, run_number).await?))
}

async fn open_run_log(
    State(state): State<AppState>,
    Path((pipeline_id, run_number, node_id)): Path<(String, i64, String)>,
    Query(query): Query<OpenQuery>,
) -> ApiResult<Json<RunLogDocument>> {
    let service = state.log_service().await?;
    Ok(Json(
        service
            .open_document(&pipeline_id, run_number, &node_id, query.retry)
            .await?,
    ))
}

async fn read_run_log_lines(
    State(state): State<AppState>,
    Path((pipeline_id, run_number, node_id)): Path<(String, i64, String)>,
    Query(query): Query<LogLinesQuery>,
) -> ApiResult<Json<LogLinesPage>> {
    let service = state.log_service().await?;
    Ok(Json(
        service
            .read_lines(&pipeline_id, run_number, &node_id, query)
            .await?,
    ))
}

async fn search_run_log(
    State(state): State<AppState>,
    Path((pipeline_id, run_number, node_id)): Path<(String, i64, String)>,
    Query(params): Query<SearchParams>, Json(query): Json<LogSearchQuery>,
) -> ApiResult<Json<LogSearchResult>> {
    let service = state.log_service().await?;
    Ok(Json(
        service
            .search(
                &pipeline_id,
                run_number,
                &node_id,
                params.revision,
                &params.session,
                &query,
            )
            .await?,
    ))
}

async fn issue_download_ticket(
    State(state): State<AppState>,
    Path((pipeline_id, run_number, node_id)): Path<(String, i64, String)>,
    Json(request): Json<TicketRequest>,
) -> ApiResult<Json<TicketResponse>> {
    let service = state.log_service().await?;
    let export = service
        .prepare_export(&pipeline_id, run_number, &node_id, request.revision)
        .await?;
    let file_name = sanitize_file_name(&request.file_name, run_number);
    let ticket = state.download_tickets.issue(file_name, export)?;
    Ok(Json(TicketResponse { ticket }))
}

pub async fn download(
    State(state): State<AppState>, Path(ticket): Path<String>,
) -> ApiResult<Response> {
    let (file_name, export) = state
        .download_tickets
        .take(&ticket)
        .ok_or_else(|| AppError::not_found("This download is no longer available"))?;
    let stream = export.into_stream()?;
    Ok((
        [
            (CONTENT_TYPE, "text/plain; charset=utf-8".to_owned()),
            (
                CONTENT_DISPOSITION,
                format!("attachment; filename=\"{file_name}\""),
            ),
            (CACHE_CONTROL, "no-store".to_owned()),
            (REFERRER_POLICY, "no-referrer".to_owned()),
        ],
        Body::from_stream(stream),
    )
        .into_response())
}

fn sanitize_file_name(raw: &str, run_number: i64) -> String {
    let mut name: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    name.truncate(MAX_FILE_NAME_BYTES);
    if name.ends_with(".log") {
        name
    } else {
        format!("run-{run_number}.log")
    }
}

#[cfg(test)]
mod tests;
