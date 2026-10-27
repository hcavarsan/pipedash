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
struct LinesQuery {
    revision: i64,
    start: u32,
    count: u32,
    max_line_units: u32,
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
    Query(query): Query<LinesQuery>,
) -> ApiResult<Json<LogLinesPage>> {
    let service = state.log_service().await?;
    Ok(Json(
        service
            .read_lines(
                &pipeline_id,
                run_number,
                &node_id,
                query.revision,
                query.start,
                query.count,
                query.max_line_units,
            )
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
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use axum::body::Bytes;
    use axum::http::header::{
        ACCEPT_ENCODING,
        AUTHORIZATION,
        CONTENT_ENCODING,
    };
    use axum::http::{
        HeaderMap,
        Method,
        Request,
        StatusCode,
    };
    use pipedash_core::domain::Pipeline;
    use pipedash_core::infrastructure::{
        PipedashConfig,
        StorageBackendType,
        StorageManager,
    };
    use pipedash_core::{
        CoreContext,
        ProviderConfig,
    };
    use serde_json::{
        json,
        Value,
    };
    use tower::ServiceExt;

    use super::*;
    use crate::routes::CONFIG_ENV;
    use crate::ws::WebSocketEventBus;

    const TOKEN: &str = "operator-secret";
    const NODE: &str = "501";
    const LOG: &str = "2026-10-20T10:00:05.0000000Z ##[group]Run make\n\
                       2026-10-20T10:00:05.1000000Z hello from the job\n\
                       2026-10-20T10:00:05.2000000Z ##[endgroup]\n\
                       2026-10-20T10:00:06.0000000Z done\n";

    fn run_json() -> Value {
        let url = "http://stub.invalid/acme/app";
        json!({
            "id": 9001, "workflow_id": 42, "node_id": "n", "name": "CI",
            "head_branch": "main", "head_sha": "abc", "run_number": 7, "event": "push",
            "status": "completed", "conclusion": "success",
            "created_at": "2026-10-20T10:00:00Z", "updated_at": "2026-10-20T10:01:00Z",
            "url": url, "html_url": url, "jobs_url": url, "logs_url": url,
            "check_suite_url": url, "artifacts_url": url, "cancel_url": url,
            "rerun_url": url, "workflow_url": url,
            "head_commit": {
                "id": "abc", "tree_id": "def", "message": "ship it",
                "timestamp": "2026-10-20T09:59:00Z",
                "author": {"name": "Ada", "email": "ada@example.com"},
                "committer": {"name": "Ada", "email": "ada@example.com"}
            },
            "repository": {"id": 1, "name": "app", "url": url}
        })
    }

    fn job_json() -> Value {
        let url = "http://stub.invalid/acme/app";
        json!({
            "id": 501, "run_id": 9001, "workflow_name": "CI", "head_branch": "main",
            "run_url": url, "run_attempt": 1, "node_id": "n", "head_sha": "abc",
            "url": url, "html_url": url, "status": "completed", "conclusion": "success",
            "created_at": "2026-10-20T10:00:00Z", "started_at": "2026-10-20T10:00:04Z",
            "completed_at": "2026-10-20T10:00:07Z", "name": "build",
            "steps": [{
                "name": "Run make", "status": "completed", "conclusion": "success",
                "number": 1, "started_at": "2026-10-20T10:00:05Z",
                "completed_at": "2026-10-20T10:00:06Z"
            }],
            "check_run_url": url, "labels": []
        })
    }

    async fn stub_github() -> String {
        let stub = Router::new()
            .route(
                "/api/v3/repos/acme/app/actions/workflows/42/runs",
                get(|| async { Json(json!({"total_count": 1, "workflow_runs": [run_json()]})) }),
            )
            .route(
                "/api/v3/repos/acme/app/actions/runs/9001/jobs",
                get(|| async { Json(json!({"total_count": 1, "jobs": [job_json()]})) }),
            )
            .route(
                "/api/v3/repos/acme/app/actions/jobs/501/logs",
                get(|| async { LOG }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, stub).await.unwrap() });
        url
    }

    struct Harness {
        app: Router,
        state: AppState,
        pipeline_id: String,
        data_dir: PathBuf,
        _env: tokio::sync::MutexGuard<'static, ()>,
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.data_dir);
        }
    }

    async fn harness(name: &str) -> Harness {
        let env = CONFIG_ENV.lock().await;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let data_dir =
            std::env::temp_dir().join(format!("pipedash-run-logs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_dir);
        std::env::set_var("PIPEDASH_CONFIG_PATH", data_dir.join("config.toml"));
        let mut config = PipedashConfig::default();
        config.storage.backend = StorageBackendType::Sqlite;
        config.storage.data_dir = data_dir.display().to_string();
        config.storage.vault_password = Some("vault-password".into());
        let storage_manager = StorageManager::from_config(config, false).await.unwrap();

        let provider = ProviderConfig {
            id: None,
            name: "github".into(),
            provider_type: "github".into(),
            token: String::new(),
            config: [
                ("base_url".to_string(), stub_github().await),
                ("selected_items".to_string(), "acme/app".to_string()),
            ]
            .into(),
            refresh_interval: 30,
            version: None,
        };
        let provider_id = storage_manager
            .config_backend()
            .create_provider(&provider)
            .await
            .unwrap();
        storage_manager
            .token_store()
            .await
            .store_token(provider_id, "ghp_stub")
            .await
            .unwrap();

        let ws_event_bus = Arc::new(WebSocketEventBus::new());
        let core = CoreContext::with_storage_manager(&storage_manager, ws_event_bus.clone())
            .await
            .unwrap();
        core.provider_service.load_all_providers().await.unwrap();
        let pipeline_id = format!("github__{provider_id}__acme__app__42");
        let pipeline: Pipeline = serde_json::from_value(json!({
            "id": pipeline_id, "provider_id": provider_id, "provider_type": "github",
            "name": "CI", "status": "success", "last_updated": "2026-10-20T10:01:00Z",
            "repository": "acme/app"
        }))
        .unwrap();
        core.provider_service
            .repository()
            .update_pipelines_cache(provider_id, &[pipeline])
            .await
            .unwrap();

        let state = AppState::initialized(ws_event_bus, core, storage_manager);
        state.set_api_token(TOKEN);
        Harness {
            app: crate::app(state.clone(), None),
            state,
            pipeline_id,
            data_dir,
            _env: env,
        }
    }

    fn authed(method: Method, uri: &str, body: Option<Value>) -> Request<Body> {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(AUTHORIZATION, format!("Bearer {TOKEN}"));
        match body {
            Some(body) => request
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap()
    }

    async fn send(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Bytes) {
        let response = app.clone().oneshot(request).await.unwrap();
        let (status, headers) = (response.status(), response.headers().clone());
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        (status, headers, body)
    }

    fn json_of(body: &Bytes) -> Value {
        serde_json::from_slice(body).unwrap_or(Value::Null)
    }

    impl Harness {
        fn logs_uri(&self, suffix: &str) -> String {
            format!(
                "/api/v1/pipelines/{}/runs/7/logs/{NODE}{suffix}",
                self.pipeline_id
            )
        }

        async fn ready_revision(&self) -> i64 {
            let uri = self.logs_uri("?retry=false");
            for _ in 0..200 {
                let (status, _, body) = send(&self.app, authed(Method::GET, &uri, None)).await;
                let document = json_of(&body);
                assert_eq!(status, StatusCode::OK, "{document}");
                if document["state"] == "ready" {
                    return document["revision"].as_i64().unwrap();
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            panic!("the log never became ready");
        }

        async fn issue(&self, revision: i64) -> (StatusCode, Value) {
            let body = json!({ "revision": revision, "file_name": "run 7/logs.log" });
            let uri = self.logs_uri("/download-ticket");
            let (status, _, body) = send(&self.app, authed(Method::POST, &uri, Some(body))).await;

            (status, json_of(&body))
        }

        async fn download(&self, ticket: &str) -> (StatusCode, HeaderMap, Bytes) {
            let request = Request::get(format!("/api/v1/downloads/{ticket}"))
                .body(Body::empty())
                .unwrap();

            send(&self.app, request).await
        }
    }

    #[tokio::test]
    async fn a_ticket_streams_the_rendered_log_once() {
        let h = harness("ticket-once").await;
        let revision = h.ready_revision().await;

        let (status, body) = h.issue(revision).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let ticket = body["ticket"].as_str().unwrap().to_string();
        assert_eq!(ticket.len(), 32);

        let (status, headers, body) = h.download(&ticket).await;
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[CACHE_CONTROL], "no-store");
        assert_eq!(headers[REFERRER_POLICY], "no-referrer");
        assert_eq!(headers[CONTENT_TYPE], "text/plain; charset=utf-8");
        assert_eq!(
            headers[CONTENT_DISPOSITION],
            "attachment; filename=\"run-7-logs.log\""
        );
        assert!(!headers.contains_key(CONTENT_ENCODING));
        assert!(
            text.starts_with("2026-10-20T10:00:05.000Z Run make\n"),
            "{text}"
        );
        assert!(text.contains("hello from the job\n"), "{text}");
        assert!(text.ends_with("done\n"), "{text}");

        let (status, _, _) = h.download(&ticket).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn fabricated_and_expired_tickets_are_not_found() {
        let h = harness("ticket-expiry").await;
        let revision = h.ready_revision().await;
        let (_, body) = h.issue(revision).await;
        let ticket = body["ticket"].as_str().unwrap().to_string();

        let (status, _, _) = h.download(&format!("{:032x}", 7u128)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(61)).await;

        let (status, _, _) = h.download(&ticket).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn the_33rd_outstanding_ticket_is_refused() {
        let h = harness("ticket-limit").await;
        let revision = h.ready_revision().await;

        for _ in 0..32 {
            assert_eq!(h.issue(revision).await.0, StatusCode::OK);
        }
        let (status, body) = h.issue(revision).await;

        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(body["code"], "TOO_MANY_DOWNLOADS");
    }

    #[tokio::test]
    async fn clearing_the_tickets_invalidates_the_outstanding_ones() {
        let h = harness("ticket-clear").await;
        let revision = h.ready_revision().await;
        let (_, body) = h.issue(revision).await;
        let ticket = body["ticket"].as_str().unwrap().to_string();

        h.state.download_tickets.clear();

        let (status, _, _) = h.download(&ticket).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn clearing_the_run_log_cache_invalidates_tickets_and_frees_the_bytes() {
        let h = harness("ticket-cache-clear").await;
        let revision = h.ready_revision().await;
        let (_, body) = h.issue(revision).await;
        let ticket = body["ticket"].as_str().unwrap().to_string();

        let (_, _, stats) = send(&h.app, authed(Method::GET, "/api/v1/cache/stats", None)).await;
        assert!(json_of(&stats)["run_logs_bytes"].as_u64().unwrap() > 0);

        let (status, _, _) = send(
            &h.app,
            authed(Method::DELETE, "/api/v1/cache/run-logs", None),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _, _) = h.download(&ticket).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (_, _, stats) = send(&h.app, authed(Method::GET, "/api/v1/cache/stats", None)).await;
        assert_eq!(json_of(&stats)["run_logs_bytes"], 0);

        let uri = h.logs_uri(&format!(
            "/lines?revision={revision}&start=0&count=10&max_line_units=4096"
        ));
        let (status, _, _) = send(&h.app, authed(Method::GET, &uri, None)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn line_pages_are_gzip_compressed() {
        let h = harness("compression").await;
        let revision = h.ready_revision().await;
        let uri = h.logs_uri(&format!(
            "/lines?revision={revision}&start=0&count=10&max_line_units=4096"
        ));

        let request = Request::get(&uri)
            .header(AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header(ACCEPT_ENCODING, "gzip")
            .body(Body::empty())
            .unwrap();
        let (status, headers, _) = send(&h.app, request).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[CONTENT_ENCODING], "gzip");
    }

    #[tokio::test]
    async fn search_and_lines_use_the_stored_revision() {
        let h = harness("search").await;
        let revision = h.ready_revision().await;

        let uri = h.logs_uri(&format!("/search?revision={revision}&session=s1"));
        let query = json!({ "text": "hello", "regex": false, "case_sensitive": true });
        let (status, _, body) = send(&h.app, authed(Method::POST, &uri, Some(query))).await;
        let result = json_of(&body);
        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["total"], 1);

        let uri = h.logs_uri("/lines?revision=1&start=0&count=10&max_line_units=4096");
        let (status, _, _) = send(&h.app, authed(Method::GET, &uri, None)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn download_names_are_sanitized_and_must_end_in_log() {
        for (raw, expected) in [
            ("pipe-run-7-build.log", "pipe-run-7-build.log"),
            ("../etc/pass wd.log", "..-etc-pass-wd.log"),
            ("na\"me\r\n.log", "na-me--.log"),
            ("notes.txt", "run-7.log"),
            ("", "run-7.log"),
        ] {
            assert_eq!(sanitize_file_name(raw, 7), expected, "{raw:?}");
        }
        assert_eq!(
            sanitize_file_name(&format!("{}.log", "a".repeat(300)), 7),
            "run-7.log"
        );
    }
}
