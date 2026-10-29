use axum::extract::{
    Path,
    Query,
    State,
};
use axum::http::StatusCode;
use axum::routing::{
    get,
    post,
    put,
};
use axum::{
    Json,
    Router,
};
use pipedash_core::domain::{
    CreateViewInput,
    LegacyImportResult,
    LegacyMarks,
    PinKind,
    PinnedItem,
    PipelineView,
    PipelineViewPage,
    PipelineViewQuery,
    ViewMember,
    ViewPatch,
    ViewRef,
};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct OrderRequest {
    order: Vec<ViewRef>,
}

#[derive(Deserialize)]
pub struct HiddenRequest {
    pipeline_id: String,
    hidden: bool,
}

#[derive(Deserialize)]
pub struct PinRequest {
    kind: PinKind,
    #[serde(rename = "ref")]
    reference: String,
    pinned: bool,
}

#[derive(Deserialize)]
pub struct MembershipRequest {
    pipeline_id: String,
    member: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_views).post(create_view))
        .route("/page", get(page))
        .route("/order", put(reorder_views))
        .route("/hidden", put(set_hidden))
        .route("/pins", get(list_pins).put(set_pin))
        .route("/legacy-import", post(import_legacy_marks))
        .route("/{id}", put(update_view).delete(delete_view))
        .route("/{id}/members", get(list_members))
        .route("/{id}/membership", put(set_membership))
}

async fn list_views(State(state): State<AppState>) -> ApiResult<Json<Vec<PipelineView>>> {
    Ok(Json(
        state.pipeline_view_service().await?.list_views().await?,
    ))
}

async fn create_view(
    State(state): State<AppState>, Json(input): Json<CreateViewInput>,
) -> ApiResult<(StatusCode, Json<PipelineView>)> {
    let view = state
        .pipeline_view_service()
        .await?
        .create_view(input)
        .await?;
    Ok((StatusCode::CREATED, Json(view)))
}

async fn page(
    State(state): State<AppState>, Query(query): Query<PipelineViewQuery>,
) -> ApiResult<Json<PipelineViewPage>> {
    Ok(Json(
        state.pipeline_view_service().await?.page(query).await?,
    ))
}

async fn list_pins(State(state): State<AppState>) -> ApiResult<Json<Vec<PinnedItem>>> {
    Ok(Json(
        state.pipeline_view_service().await?.list_pins().await?,
    ))
}

async fn set_pin(State(state): State<AppState>, Json(req): Json<PinRequest>) -> ApiResult<()> {
    Ok(state
        .pipeline_view_service()
        .await?
        .set_pin(req.kind, &req.reference, req.pinned)
        .await?)
}

async fn reorder_views(
    State(state): State<AppState>, Json(req): Json<OrderRequest>,
) -> ApiResult<()> {
    Ok(state
        .pipeline_view_service()
        .await?
        .reorder_views(req.order)
        .await?)
}

async fn set_hidden(
    State(state): State<AppState>, Json(req): Json<HiddenRequest>,
) -> ApiResult<()> {
    Ok(state
        .pipeline_view_service()
        .await?
        .set_hidden(&req.pipeline_id, req.hidden)
        .await?)
}

async fn import_legacy_marks(
    State(state): State<AppState>, Json(marks): Json<LegacyMarks>,
) -> ApiResult<Json<LegacyImportResult>> {
    Ok(Json(
        state
            .pipeline_view_service()
            .await?
            .import_legacy_marks(marks)
            .await?,
    ))
}

async fn update_view(
    State(state): State<AppState>, Path(id): Path<ViewRef>, Json(patch): Json<ViewPatch>,
) -> ApiResult<Json<PipelineView>> {
    Ok(Json(
        state
            .pipeline_view_service()
            .await?
            .update_view(id, patch)
            .await?,
    ))
}

async fn delete_view(State(state): State<AppState>, Path(id): Path<ViewRef>) -> ApiResult<()> {
    Ok(state.pipeline_view_service().await?.delete_view(id).await?)
}

async fn list_members(
    State(state): State<AppState>, Path(id): Path<ViewRef>,
) -> ApiResult<Json<Vec<ViewMember>>> {
    Ok(Json(
        state
            .pipeline_view_service()
            .await?
            .list_members(id)
            .await?,
    ))
}

async fn set_membership(
    State(state): State<AppState>, Path(id): Path<ViewRef>, Json(req): Json<MembershipRequest>,
) -> ApiResult<()> {
    Ok(state
        .pipeline_view_service()
        .await?
        .set_membership(id, &req.pipeline_id, req.member)
        .await?)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{
        Method,
        Request,
    };
    use pipedash_core::domain::{
        Pipeline,
        PipelineStatus,
    };
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

    struct Harness {
        app: Router,
        pipeline_ids: Vec<String>,
        data_dir: PathBuf,
        _env: tokio::sync::MutexGuard<'static, ()>,
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.data_dir);
        }
    }

    fn pipeline(provider_id: i64, name: &str) -> Pipeline {
        Pipeline {
            id: format!("jenkins__{provider_id}__{name}"),
            provider_id,
            provider_type: "jenkins".into(),
            name: name.into(),
            status: PipelineStatus::Success,
            last_run: None,
            last_updated: chrono::Utc::now(),
            repository: format!("acme/{name}"),
            branch: None,
            workflow_file: None,
            resource_state: None,
            metadata: Default::default(),
            overdue_run: None,
            typical_duration_seconds: None,
        }
    }

    async fn harness() -> Harness {
        let env = CONFIG_ENV.lock().await;
        let data_dir = std::env::temp_dir().join(format!("pipedash-views-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_dir);
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(data_dir.join("config.toml"), "").unwrap();
        std::env::set_var("PIPEDASH_CONFIG_PATH", data_dir.join("config.toml"));

        let mut config = PipedashConfig::default();
        config.storage.backend = StorageBackendType::Sqlite;
        config.storage.data_dir = data_dir.display().to_string();
        config.storage.vault_password = Some("vault-password".into());
        let storage_manager = StorageManager::from_config(config, false).await.unwrap();
        let provider_id = storage_manager
            .config_backend()
            .create_provider(&ProviderConfig {
                id: None,
                name: "jenkins".into(),
                provider_type: "jenkins".into(),
                token: String::new(),
                config: [
                    ("server_url".to_string(), "http://127.0.0.1:9".to_string()),
                    ("username".to_string(), "admin".to_string()),
                ]
                .into(),
                refresh_interval: 30,
                version: None,
            })
            .await
            .unwrap();

        let ws_event_bus = Arc::new(WebSocketEventBus::new());
        let core = CoreContext::with_storage_manager(&storage_manager, ws_event_bus.clone())
            .await
            .unwrap();
        let pipelines = vec![pipeline(provider_id, "api"), pipeline(provider_id, "web")];
        core.provider_service
            .repository()
            .update_pipelines_cache(provider_id, &pipelines)
            .await
            .unwrap();
        let state = AppState::initialized(ws_event_bus, core, storage_manager);
        let app = Router::new()
            .nest("/pipeline-views", router())
            .with_state(state);

        Harness {
            app,
            pipeline_ids: pipelines.into_iter().map(|p| p.id).collect(),
            data_dir,
            _env: env,
        }
    }

    async fn call(
        app: &Router, method: Method, uri: &str, body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(value) => {
                builder = builder.header("content-type", "application/json");
                Body::from(value.to_string())
            }
            None => Body::empty(),
        };
        let response = app
            .clone()
            .oneshot(builder.body(body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    #[tokio::test]
    async fn pipeline_views_create_add_and_view_over_http() {
        let h = harness().await;

        let (status, view) = call(
            &h.app,
            Method::POST,
            "/pipeline-views",
            Some(json!({ "name": "Release" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let id = view["id"].as_str().unwrap().to_string();
        assert!(id.parse::<i64>().is_ok());

        let (status, _) = call(
            &h.app,
            Method::PUT,
            &format!("/pipeline-views/{id}/membership"),
            Some(json!({ "pipeline_id": h.pipeline_ids[0], "member": true })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, page) = call(
            &h.app,
            Method::GET,
            &format!("/pipeline-views/page?view={id}&show_hidden=false"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["total"], 1);

        let (status, child) = call(
            &h.app,
            Method::POST,
            "/pipeline-views",
            Some(json!({ "name": "Child", "parent": id })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let child_id = child["id"].as_str().unwrap().to_string();

        let (status, _) = call(
            &h.app,
            Method::PUT,
            &format!("/pipeline-views/{child_id}/membership"),
            Some(json!({ "pipeline_id": h.pipeline_ids[1], "member": true })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, page) = call(
            &h.app,
            Method::GET,
            &format!("/pipeline-views/page?view={id}&show_hidden=false"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["total"], 2);
        assert_eq!(page["sections"][0]["id"], format!("view:{child_id}"));
        assert_eq!(page["sections"][0]["kind"], "view");
        assert_eq!(page["sections"][1]["kind"], "repository");

        let (status, error) =
            call(&h.app, Method::GET, "/pipeline-views/page?view=999", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(error["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn pipeline_view_page_filters_by_any_of_several_statuses() {
        let h = harness().await;

        for (statuses, total) in [
            ("failed,success", h.pipeline_ids.len()),
            ("success", h.pipeline_ids.len()),
            ("running,pending", 0),
        ] {
            let (status, page) = call(
                &h.app,
                Method::GET,
                &format!("/pipeline-views/page?view=all&statuses={statuses}"),
                None,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{statuses}");
            assert_eq!(page["total"], total, "{statuses}");
        }

        let (status, _) = call(
            &h.app,
            Method::GET,
            "/pipeline-views/page?view=all&statuses=bogus",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn pipeline_view_pins_round_trip_over_http() {
        let h = harness().await;

        let (status, _) = call(
            &h.app,
            Method::PUT,
            "/pipeline-views/pins",
            Some(json!({ "kind": "repository", "ref": "acme/api", "pinned": true })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, pins) = call(&h.app, Method::GET, "/pipeline-views/pins", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            pins,
            json!([{ "kind": "repository", "ref": "acme/api", "name": "api" }])
        );

        let (status, page) = call(&h.app, Method::GET, "/pipeline-views/page?view=all", None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page["pinned_blocks"][0]["ref"], "acme/api");
        assert_eq!(
            page["pinned_blocks"][0]["pipelines"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let (status, error) = call(
            &h.app,
            Method::PUT,
            "/pipeline-views/pins",
            Some(json!({ "kind": "repository", "ref": "acme/missing", "pinned": true })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(error["code"], "NOT_FOUND");

        let (status, _) = call(
            &h.app,
            Method::PUT,
            "/pipeline-views/pins",
            Some(json!({ "kind": "repository", "ref": "acme/api", "pinned": false })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, pins) = call(&h.app, Method::GET, "/pipeline-views/pins", None).await;
        assert_eq!(pins, json!([]));
    }
}
