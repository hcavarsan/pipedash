use std::collections::HashSet;
use std::net::{
    IpAddr,
    SocketAddr,
};
use std::sync::Arc;
use std::time::{
    Duration,
    Instant,
};

use axum::extract::{
    ConnectInfo,
    DefaultBodyLimit,
    Path,
    State,
};
use axum::http::{
    HeaderMap,
    StatusCode,
};
use axum::response::{
    IntoResponse,
    Response,
};
use axum::routing::{
    get,
    post,
    put,
};
use axum::{
    Json,
    Router,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::{
    SecureRandom,
    SystemRandom,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::apns::{
    unix_now,
    Apns,
    Notification,
    Outcome,
};
use crate::rate_limit::RateLimiter;
use crate::store::{
    subscription_key,
    Environment,
    Store,
    Subscription,
};

const MAX_REQUEST_BYTES: usize = 16 * 1024;
const MIN_TOKEN_HEX_LEN: usize = 64;
const MAX_TOKEN_HEX_LEN: usize = 200;
const MAX_TITLE_BYTES: usize = 512;
const MAX_BODY_BYTES: usize = 2048;
const MAX_THREAD_ID_BYTES: usize = 512;
const MAX_ALERT_ID_BYTES: usize = 64;
const RATE_LIMIT: u32 = 30;
const RATE_WINDOW: Duration = Duration::from_secs(60);
const REGISTRATION_LIMIT: u32 = 20;
const REGISTRATION_WINDOW: Duration = Duration::from_secs(60 * 60);
const MAX_SUBSCRIPTIONS_PER_TOKEN: u32 = 10;

#[derive(Clone)]
pub struct AppState {
    store: Store,
    apns: Arc<Apns>,
    limiter: Arc<RateLimiter>,
    registrations: Arc<RateLimiter>,
    allowed_bundle_ids: Arc<HashSet<String>>,
    trusted_proxy: bool,
}

impl AppState {
    pub fn new(
        store: Store, apns: Arc<Apns>, allowed_bundle_ids: HashSet<String>, trusted_proxy: bool,
    ) -> Self {
        Self {
            store,
            apns,
            limiter: Arc::new(RateLimiter::new(RATE_LIMIT, RATE_WINDOW)),
            registrations: Arc::new(RateLimiter::new(REGISTRATION_LIMIT, REGISTRATION_WINDOW)),
            allowed_bundle_ids: Arc::new(allowed_bundle_ids),
            trusted_proxy,
        }
    }

    pub fn prune_limiters(&self, now: Instant) {
        self.limiter.prune(now);
        self.registrations.prune(now);
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/subscriptions", post(create))
        .route(
            "/v1/subscriptions/{subscription_id}",
            put(rotate).delete(remove),
        )
        .route("/v1/notifications", post(notify))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .with_state(state)
}

enum ApiError {
    BadRequest(&'static str),
    Forbidden,
    NotFound,
    Gone,
    TooManyRequests,
    BadGateway,
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Forbidden => (StatusCode::FORBIDDEN, "bundle id not allowed"),
            Self::NotFound => (StatusCode::NOT_FOUND, "unknown subscription"),
            Self::Gone => (StatusCode::GONE, "device token is no longer registered"),
            Self::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded"),
            Self::BadGateway => (
                StatusCode::BAD_GATEWAY,
                "APNs did not accept the notification",
            ),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "Database error");
        Self::Internal
    }
}

#[derive(Deserialize)]
struct CreateRequest {
    device_token: String,
    environment: Environment,
    bundle_id: String,
}

#[derive(Serialize)]
struct Created {
    subscription_id: String,
}

#[derive(Deserialize)]
struct RotateRequest {
    device_token: String,
}

#[derive(Deserialize)]
struct NotifyRequest {
    subscription_id: String,
    alert_id: Option<String>,
    title: Option<String>,
    body: Option<String>,
    thread_id: Option<String>,
    pipeline_id: Option<String>,
    provider_id: Option<u64>,
    run_number: Option<u64>,
}

fn normalize_token(token: &str) -> Result<String, ApiError> {
    let valid = (MIN_TOKEN_HEX_LEN..=MAX_TOKEN_HEX_LEN).contains(&token.len())
        && token.len() % 2 == 0
        && token.bytes().all(|b| b.is_ascii_hexdigit());

    if valid {
        Ok(token.to_ascii_lowercase())
    } else {
        Err(ApiError::BadRequest("invalid device token"))
    }
}

fn new_subscription_id() -> Result<String, ApiError> {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| ApiError::Internal)?;

    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn client_key(peer: IpAddr, headers: &HeaderMap, trusted_proxy: bool) -> String {
    let forwarded = trusted_proxy
        .then(|| {
            headers
                .get_all("x-forwarded-for")
                .iter()
                .next_back()
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.rsplit(',').next())
                .and_then(|entry| entry.trim().parse::<IpAddr>().ok())
        })
        .flatten();

    match forwarded.unwrap_or(peer) {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            format!(
                "{:x}:{:x}:{:x}:{:x}::/64",
                segments[0], segments[1], segments[2], segments[3]
            )
        }
    }
}

async fn healthz() -> &'static str {
    "ok"
}

async fn create(
    State(state): State<AppState>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap,
    Json(request): Json<CreateRequest>,
) -> Result<(StatusCode, Json<Created>), ApiError> {
    let client = client_key(peer.ip(), &headers, state.trusted_proxy);
    if !state.registrations.allow(&client, Instant::now()) {
        return Err(ApiError::TooManyRequests);
    }

    let device_token = normalize_token(&request.device_token)?;
    if !state.allowed_bundle_ids.contains(&request.bundle_id) {
        return Err(ApiError::Forbidden);
    }

    let subscription_id = new_subscription_id()?;
    let subscription = Subscription {
        device_token,
        environment: request.environment,
        bundle_id: request.bundle_id,
    };
    state
        .store
        .insert(
            &subscription_key(&subscription_id),
            &subscription,
            unix_now(),
            MAX_SUBSCRIPTIONS_PER_TOKEN,
        )
        .await?;

    Ok((StatusCode::CREATED, Json(Created { subscription_id })))
}

async fn rotate(
    State(state): State<AppState>, Path(subscription_id): Path<String>,
    Json(request): Json<RotateRequest>,
) -> Result<StatusCode, ApiError> {
    let device_token = normalize_token(&request.device_token)?;

    if state
        .store
        .update_token(
            &subscription_key(&subscription_id),
            &device_token,
            unix_now(),
        )
        .await?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

async fn remove(
    State(state): State<AppState>, Path(subscription_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .store
        .delete(&subscription_key(&subscription_id))
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

fn notification(request: &NotifyRequest) -> Result<Notification<'_>, ApiError> {
    let invalid = ApiError::BadRequest("invalid notification");

    if let Some(alert_id) = request.alert_id.as_deref() {
        let legacy_field = request.title.is_some()
            || request.body.is_some()
            || request.thread_id.is_some()
            || request.pipeline_id.is_some()
            || request.provider_id.is_some()
            || request.run_number.is_some();
        let valid_id = (1..=MAX_ALERT_ID_BYTES).contains(&alert_id.len())
            && alert_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if legacy_field || !valid_id {
            return Err(invalid);
        }
        return Ok(Notification::Alert { alert_id });
    }

    let (Some(title), Some(body)) = (request.title.as_deref(), request.body.as_deref()) else {
        return Err(invalid);
    };
    if title.is_empty()
        || title.len() > MAX_TITLE_BYTES
        || body.len() > MAX_BODY_BYTES
        || request
            .thread_id
            .as_ref()
            .is_some_and(|thread_id| thread_id.len() > MAX_THREAD_ID_BYTES)
        || request.pipeline_id.as_ref().is_some_and(|pipeline_id| {
            pipeline_id.is_empty() || pipeline_id.len() > MAX_THREAD_ID_BYTES
        })
    {
        return Err(invalid);
    }

    Ok(Notification::Legacy {
        title,
        body,
        thread_id: request.thread_id.as_deref(),
        pipeline_id: request.pipeline_id.as_deref(),
        provider_id: request.provider_id,
        run_number: request.run_number,
    })
}

async fn notify(
    State(state): State<AppState>, Json(request): Json<NotifyRequest>,
) -> Result<StatusCode, ApiError> {
    let notification = notification(&request)?;

    let key = subscription_key(&request.subscription_id);
    let Some(subscription) = state.store.get(&key).await? else {
        return Err(ApiError::NotFound);
    };
    if !state.limiter.allow(&key, Instant::now()) {
        return Err(ApiError::TooManyRequests);
    }
    match state.apns.send(&subscription, &notification).await {
        Outcome::Delivered => {
            if let Err(error) = state.store.touch(&key, unix_now()).await {
                tracing::warn!(%error, "Couldn't record a successful push");
            }
            Ok(StatusCode::ACCEPTED)
        }
        Outcome::Unregistered => {
            let deleted = state
                .store
                .delete_if_token(&key, &subscription.device_token)
                .await?;
            if deleted {
                Err(ApiError::Gone)
            } else {
                Err(ApiError::BadGateway)
            }
        }
        Outcome::Failed => Err(ApiError::BadGateway),
    }
}

#[cfg(test)]
mod tests;
