use std::future::Future;

use axum::extract::{
    Path,
    Query,
    State,
};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{
    Json,
    Router,
};
use pipedash_core::domain::{
    DeviceAlert,
    DomainResult,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::{
    ApiError,
    ApiResult,
    AppError,
};
use crate::state::AppState;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

#[derive(Deserialize)]
struct AlertsQuery {
    after: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct AlertView {
    #[serde(flatten)]
    alert: DeviceAlert,
    tone: &'static str,
}

impl From<DeviceAlert> for AlertView {
    fn from(alert: DeviceAlert) -> Self {
        let tone = alert.kind.tone();

        Self { alert, tone }
    }
}

#[derive(Debug, Serialize)]
struct AlertsResponse {
    alerts: Vec<AlertView>,
    latest_seq: i64,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_alerts))
        .route("/{id}", get(get_alert))
}

async fn read_page(
    latest_seq: impl Future<Output = DomainResult<i64>>,
    page: impl Future<Output = DomainResult<Vec<DeviceAlert>>>,
) -> DomainResult<AlertsResponse> {
    let latest_seq = latest_seq.await?;
    let alerts = page.await?;

    Ok(AlertsResponse {
        alerts: alerts.into_iter().map(AlertView::from).collect(),
        latest_seq,
    })
}

async fn list_alerts(
    State(state): State<AppState>, Query(query): Query<AlertsQuery>,
) -> ApiResult<Json<AlertsResponse>> {
    let alerts = state.core().await?.device_alerts();
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let after = query.after.unwrap_or(0);

    Ok(Json(
        read_page(alerts.latest_seq(), alerts.after(after, limit)).await?,
    ))
}

async fn get_alert(
    State(state): State<AppState>, Path(id): Path<String>,
) -> ApiResult<Json<AlertView>> {
    state
        .core()
        .await?
        .device_alerts()
        .get(&id)
        .await?
        .map(|alert| Json(alert.into()))
        .ok_or_else(|| {
            AppError::new(
                StatusCode::NOT_FOUND,
                ApiError::new("ALERT_NOT_FOUND", "This alert is no longer available."),
            )
        })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{
        header::AUTHORIZATION,
        Request,
    };
    use pipedash_core::domain::{
        AlertKind,
        NewDeviceAlert,
    };
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::test_support::{
        hosted,
        Hosted,
    };

    async fn device_app(host: &Hosted) -> (Router, String) {
        let (_, token) = host
            .state
            .core()
            .await
            .unwrap()
            .pair_device("Pixel", "ios")
            .await
            .unwrap();

        (crate::app(host.state.clone(), None), token)
    }

    async fn get_json(app: &Router, uri: &str, token: &str) -> (StatusCode, Value) {
        let request = Request::get(uri)
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    fn new_alert(kind: AlertKind, key: &str) -> NewDeviceAlert {
        NewDeviceAlert {
            key: key.into(),
            kind,
            title: format!("{key} title"),
            body: format!("{key} body"),
            pipeline_id: Some(key.into()),
            provider_id: Some(7),
        }
    }

    #[tokio::test]
    async fn alerts_are_read_after_a_cursor_without_test_rows() {
        let host = hosted("alerts-after").await;
        let alerts = host.state.core().await.unwrap().device_alerts();
        let first = alerts
            .insert(&new_alert(AlertKind::Failed, "pipeline:a"))
            .await
            .unwrap();
        alerts
            .insert(&new_alert(AlertKind::Test, "test:x"))
            .await
            .unwrap();
        let last = alerts
            .insert(&new_alert(AlertKind::Recovered, "pipeline:b"))
            .await
            .unwrap();
        let (app, token) = device_app(&host).await;

        let (status, body) = get_json(&app, "/api/v1/alerts", &token).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["alerts"].as_array().unwrap().len(), 2);
        assert_eq!(body["alerts"][0]["id"], first.id);
        assert_eq!(body["alerts"][0]["kind"], "failed");
        assert_eq!(body["alerts"][0]["tone"], "danger");
        assert_eq!(body["alerts"][1]["tone"], "success");
        assert_eq!(body["alerts"][0]["key"], "pipeline:a");
        assert_eq!(body["alerts"][0]["pipeline_id"], "pipeline:a");
        assert_eq!(body["alerts"][0]["provider_id"], 7);
        assert!(body["alerts"][0]["created_at"].is_string());
        assert_eq!(body["latest_seq"], last.seq);

        let uri = format!("/api/v1/alerts?after={}", first.seq);
        let (_, body) = get_json(&app, &uri, &token).await;
        let listed = body["alerts"].as_array().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0]["id"], last.id);

        let (_, body) = get_json(&app, "/api/v1/alerts?limit=0", &token).await;
        assert_eq!(body["alerts"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn one_alert_is_fetched_by_id_and_a_missing_one_is_not_found() {
        let host = hosted("alerts-get").await;
        let alerts = host.state.core().await.unwrap().device_alerts();
        let test = alerts
            .insert(&new_alert(AlertKind::Test, "test:x"))
            .await
            .unwrap();
        let (app, token) = device_app(&host).await;

        let (status, body) = get_json(&app, &format!("/api/v1/alerts/{}", test.id), &token).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["title"], "test:x title");
        assert_eq!(body["kind"], "test");
        assert_eq!(body["tone"], "success");

        let (status, body) = get_json(&app, "/api/v1/alerts/missing", &token).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "ALERT_NOT_FOUND");
        assert_eq!(body["message"], "This alert is no longer available.");
    }
}

#[cfg(test)]
mod page_order_tests {
    use pipedash_core::domain::{
        AlertKind,
        NewDeviceAlert,
    };

    use super::*;
    use crate::test_support::hosted;

    fn new_alert(key: &str) -> NewDeviceAlert {
        NewDeviceAlert {
            key: key.into(),
            kind: AlertKind::Failed,
            title: key.into(),
            body: key.into(),
            pipeline_id: None,
            provider_id: None,
        }
    }

    #[tokio::test]
    async fn an_alert_recorded_during_the_read_is_never_hidden_behind_latest_seq() {
        let host = hosted("alerts-read-order").await;
        let alerts = host.state.core().await.unwrap().device_alerts();
        let first = alerts.insert(&new_alert("pipeline:a")).await.unwrap();

        let page = read_page(
            async {
                let latest = alerts.latest_seq().await;
                alerts.insert(&new_alert("pipeline:b")).await.unwrap();
                latest
            },
            alerts.after(0, 100),
        )
        .await
        .unwrap();

        assert_eq!(page.latest_seq, first.seq);
        assert!(page.alerts.iter().any(|view| view.alert.seq == first.seq));
        let listed = page.alerts.iter().map(|view| view.alert.seq).max().unwrap();
        assert!(page.latest_seq <= listed);
    }
}
