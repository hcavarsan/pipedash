pub mod demo;
mod download_tickets;
pub mod error;
pub mod pairing;
mod pairing_codes;
pub mod push;
mod routes;
pub mod state;
#[cfg(feature = "embedded-frontend")]
mod static_files;
#[cfg(test)]
mod test_support;
pub mod ws;

use std::future::Future;
use std::sync::Arc;

use axum::{
    extract::{
        Request,
        State,
    },
    http::{
        header::{
            AUTHORIZATION,
            HOST,
            ORIGIN,
            UPGRADE,
        },
        Method,
        StatusCode,
    },
    middleware::Next,
    response::{
        IntoResponse,
        Response,
    },
    routing::get,
    Router,
};
use subtle::ConstantTimeEq;

use crate::error::{
    ApiError,
    AppError,
};
use crate::state::{
    AppState,
    CurrentDevice,
};

pub fn env_flag(value: &str) -> bool {
    value == "true" || value == "1"
}

fn token_matches(provided: &str, expected: &str) -> bool {
    provided.as_bytes().ct_eq(expected.as_bytes()).into()
}

fn bearer_token(auth_header: Option<&str>) -> Option<&str> {
    auth_header.and_then(|h| h.strip_prefix("Bearer "))
}

fn bearer_matches(auth_header: Option<&str>, token: &str) -> bool {
    bearer_token(auth_header).is_some_and(|t| token_matches(t, token))
}

fn is_websocket_upgrade(req: &Request) -> bool {
    req.headers()
        .get(UPGRADE)
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"))
}

fn is_cross_origin_request(req: &Request) -> bool {
    let guarded = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS)
        || is_websocket_upgrade(req);
    if !guarded {
        return false;
    }

    let headers = req.headers();
    if let Some(site) = headers.get("sec-fetch-site") {
        return !matches!(site.as_bytes(), b"same-origin" | b"none");
    }

    let Some(origin) = headers.get(ORIGIN) else {
        return false;
    };
    let Some((origin_scheme, origin_host)) = origin.to_str().ok().and_then(|o| o.split_once("://"))
    else {
        return true;
    };
    let Some(host) = headers
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .or_else(|| req.uri().authority().map(|a| a.as_str()))
    else {
        return true;
    };
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|p| p.to_str().ok())
        .and_then(|p| p.split(',').next())
        .map_or("http", str::trim);
    let default_port = if scheme.eq_ignore_ascii_case("https") {
        ":443"
    } else {
        ":80"
    };
    let host = host.strip_suffix(default_port).unwrap_or(host);

    !(origin_scheme.eq_ignore_ascii_case(scheme) && origin_host.eq_ignore_ascii_case(host))
}

pub async fn reject_cross_origin_requests(
    req: Request, next: Next,
) -> Result<Response, StatusCode> {
    if is_cross_origin_request(&req) {
        tracing::warn!(
            method = %req.method(),
            path = req.uri().path(),
            origin = ?req.headers().get(ORIGIN),
            "Rejected cross-origin request. Set PIPEDASH_CORS_ALLOW_ALL=true to allow other origins"
        );
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(req).await)
}

async fn auth_middleware(
    State(state): State<AppState>, mut req: Request, next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();

    if !path.starts_with("/api/v1/")
        || path.starts_with("/api/v1/health")
        || path.starts_with("/api/v1/setup")
        || path == "/api/v1/vault/status"
        || path == "/api/v1/ws"
        || path == "/api/v1/plugins"
        || (req.method() == Method::GET && path.starts_with("/api/v1/downloads/"))
    {
        return Ok(next.run(req).await);
    }

    if state.desktop_host.is_some() && state.inner.read().await.core.is_none() {
        return Ok(host_locked());
    }

    let auth_header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    if state
        .api_token()
        .is_some_and(|token| bearer_matches(auth_header, &token))
    {
        return Ok(next.run(req).await);
    }

    if let Some(token) = bearer_token(auth_header) {
        match state.paired_device(token).await {
            Ok(Some(device)) => {
                req.extensions_mut().insert(CurrentDevice(device.id));
                return Ok(next.run(req).await);
            }
            Ok(None) => {}
            Err(_) => return Ok(server_not_ready()),
        }
    }

    if state.requires_auth() {
        Err(StatusCode::UNAUTHORIZED)
    } else {
        Ok(next.run(req).await)
    }
}

fn is_host_managed(path: &str) -> bool {
    [
        "/api/v1/setup",
        "/api/v1/storage",
        "/api/v1/vault",
        "/api/v1/push",
    ]
    .iter()
    .any(|prefix| path.starts_with(prefix))
        || path == "/api/v1/factory-reset"
}

fn host_error(status: StatusCode, code: &str, message: &str) -> Response {
    AppError::new(status, ApiError::new(code, message)).into_response()
}

const HOST_LOCKED_MESSAGE: &str =
    "Pipedash is locked on the computer that shares it. Unlock it there.";
const SERVER_NOT_READY_MESSAGE: &str = "Pipedash isn't ready yet.";

fn host_locked() -> Response {
    host_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "HOST_LOCKED",
        HOST_LOCKED_MESSAGE,
    )
}

fn server_not_ready() -> Response {
    host_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "SERVER_NOT_READY",
        SERVER_NOT_READY_MESSAGE,
    )
}

async fn hosted_guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let device = req.extensions().get::<CurrentDevice>().is_some();
    if state.desktop_host.is_none() && !device {
        return next.run(req).await;
    }

    let path = req.uri().path();
    let always_open = path.starts_with("/api/v1/health")
        || path == "/api/v1/vault/status"
        || path == "/api/v1/setup/status"
        || path == "/api/v1/ws";
    if !always_open {
        if is_host_managed(path) {
            let message = if state.desktop_host.is_some() {
                "Change this on the computer that shares Pipedash."
            } else {
                "Change this on the server that runs Pipedash."
            };
            return host_error(StatusCode::CONFLICT, "HOST_MANAGED", message);
        }
        if state.desktop_host.is_some() && state.inner.read().await.core.is_none() {
            return host_locked();
        }
    }

    next.run(req).await
}

fn routes() -> Router<AppState> {
    Router::new()
        .nest("/api/v1", routes::api_router())
        .route("/api/v1/ws", get(ws::ws_handler))
}

fn guarded(router: Router<AppState>, state: AppState) -> Router {
    let router = if demo::enabled() {
        router.layer(axum::middleware::from_fn(demo::reject_changes))
    } else {
        router
    };

    router
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            hosted_guard,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state)
}

#[cfg(feature = "embedded-frontend")]
fn with_frontend(router: Router<AppState>, base_path: Option<String>) -> Router<AppState> {
    match base_path {
        Some(base_path) => router.fallback(get(move |uri| {
            let base_path = base_path.clone();
            async move { static_files::serve_static(uri, &base_path) }
        })),
        None => router.fallback(get(|| async { StatusCode::NOT_FOUND })),
    }
}

#[cfg(not(feature = "embedded-frontend"))]
fn with_frontend(router: Router<AppState>, _base_path: Option<String>) -> Router<AppState> {
    router.fallback(get(|| async { StatusCode::NOT_FOUND }))
}

pub fn app(state: AppState, frontend_base_path: Option<String>) -> Router {
    guarded(with_frontend(routes(), frontend_base_path), state)
}

pub fn host_router(state: AppState) -> Router {
    guarded(
        routes().fallback(get(|| async { StatusCode::NOT_FOUND })),
        state,
    )
}

pub async fn serve_host(
    listener: tokio::net::TcpListener, state: AppState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let closing = Arc::clone(&state.closing);
    let router = host_router(state).layer(axum::middleware::from_fn(reject_cross_origin_requests));
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown.await;
            closing.send_replace(true);
        })
        .await
}

#[cfg(test)]
mod tests;
