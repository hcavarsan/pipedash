use axum::extract::{
    Path,
    State,
};
use axum::http::StatusCode;
use axum::routing::{
    delete,
    get,
    post,
};
use axum::{
    Extension,
    Json,
    Router,
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
use crate::pairing::{
    self,
    DeviceResponse,
    PairingResponse,
};
use crate::state::{
    AppState,
    CurrentDevice,
};

#[derive(Debug, Serialize)]
pub struct ListDevicesResponse {
    pub devices: Vec<DeviceResponse>,
}

#[derive(Deserialize)]
pub struct CreatePairingRequest {
    pub server_url: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_devices))
        .route("/pairing", post(create_pairing))
        .route("/{id}", delete(revoke_device))
}

async fn list_devices(
    State(state): State<AppState>, current: Option<Extension<CurrentDevice>>,
) -> ApiResult<Json<ListDevicesResponse>> {
    let devices = state.core().await?.devices().list().await?;
    let current = current.as_ref().map(|Extension(device)| device.0.as_str());

    Ok(Json(ListDevicesResponse {
        devices: devices
            .into_iter()
            .map(|device| pairing::device_response(device, current))
            .collect(),
    }))
}

async fn create_pairing(
    State(state): State<AppState>, Json(req): Json<CreatePairingRequest>,
) -> ApiResult<Json<PairingResponse>> {
    pairing::create_pairing(&state, &req.server_url).map(Json)
}

async fn revoke_device(
    State(state): State<AppState>, Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    if state.core().await?.revoke_device(&id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::new(
            StatusCode::NOT_FOUND,
            ApiError::new("DEVICE_NOT_FOUND", "This device is no longer paired."),
        ))
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{
        header::CONTENT_TYPE,
        Method,
        Request,
    };
    use serde_json::{
        json,
        Value,
    };
    use tower::ServiceExt;

    use super::*;
    use crate::test_support::hosted;
    use crate::ws::WebSocketEventBus;

    async fn call(
        app: &Router, method: Method, uri: &str, body: Option<Value>,
    ) -> (StatusCode, Value) {
        let request = Request::builder().method(method).uri(uri);
        let request = match body {
            Some(body) => request
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
    }

    fn pairing_app() -> Router {
        let state = AppState::setup_mode(std::sync::Arc::new(WebSocketEventBus::new()));
        Router::new().nest("/devices", router()).with_state(state)
    }

    #[tokio::test]
    async fn pairing_for_a_loopback_address_returns_a_code_link_and_qr() {
        let app = pairing_app();

        let (status, body) = call(
            &app,
            Method::POST,
            "/devices/pairing",
            Some(json!({ "server_url": "http://localhost:8080/" })),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["loopback"], true);
        assert_eq!(body["server_url"], "http://localhost:8080");
        let code = body["code"].as_str().unwrap();
        assert_eq!(code.len(), 9);
        assert_eq!(code.as_bytes()[4], b'-');
        assert!(body["link"]
            .as_str()
            .unwrap()
            .starts_with("pipedash://pair?url="));
        assert!(body["qr_svg"].as_str().unwrap().contains("<svg"));
        assert!(body["expires_at"].is_string());
    }

    #[tokio::test]
    async fn pairing_for_an_invalid_address_is_unprocessable() {
        let app = pairing_app();

        let (status, body) = call(
            &app,
            Method::POST,
            "/devices/pairing",
            Some(json!({ "server_url": "ftp://host" })),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["code"], "INVALID_SERVER_URL");
    }

    #[tokio::test]
    async fn a_device_is_listed_then_revoked_once() {
        let host = hosted("devices-route").await;
        let core = host.state.core().await.unwrap();
        let (device, _) = core.pair_device("Pixel", "android").await.unwrap();
        let app = Router::new()
            .nest("/devices", router())
            .with_state(host.state.clone());

        let (status, body) = call(&app, Method::GET, "/devices", None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["devices"][0]["id"], device.id);
        assert_eq!(body["devices"][0]["name"], "Pixel");
        assert_eq!(body["devices"][0]["current"], false);

        let uri = format!("/devices/{}", device.id);
        assert_eq!(
            call(&app, Method::DELETE, &uri, None).await.0,
            StatusCode::NO_CONTENT
        );
        let (status, body) = call(&app, Method::DELETE, &uri, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["code"], "DEVICE_NOT_FOUND");
    }
}
