use axum::{
    extract::Request,
    http::{
        Method,
        StatusCode,
    },
    middleware::Next,
    response::{
        IntoResponse,
        Response,
    },
};

use crate::error::{
    ApiError,
    AppError,
};
use crate::state::CurrentDevice;

pub fn enabled() -> bool {
    std::env::var("PIPEDASH_DEMO_MODE").is_ok_and(|value| crate::env_flag(&value))
}

pub fn is_blocked(method: &Method, path: &str) -> bool {
    let Some(route) = path.strip_prefix("/api/v1/") else {
        return false;
    };

    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return route.starts_with("storage/")
            || route == "providers/export"
            || route.starts_with("push/");
    }

    !(*method == Method::POST && allows_post(route))
}

fn allows_post(route: &str) -> bool {
    route == "vault/unlock"
        || route == "refresh/all"
        || route == "metrics/aggregated"
        || route == "devices/pairing"
        || (route.starts_with("providers/") && route.ends_with("/refresh"))
        || (route.starts_with("metrics/pipelines/") && route.ends_with("/query"))
        || (route.starts_with("pipelines/")
            && (route.ends_with("/search") || route.ends_with("/download-ticket")))
}

pub fn device_name(platform: &str) -> String {
    let label = match platform {
        "macos" => "Mac",
        "windows" => "Windows PC",
        "ios" => "iPhone or iPad",
        "android" => "Android device",
        _ => "Linux PC",
    };
    format!("Demo {label}")
}

fn device_owned_write(req: &Request) -> bool {
    let Some(device) = req.extensions().get::<CurrentDevice>() else {
        return false;
    };
    let path = req.uri().path();

    (*req.method() == Method::DELETE
        && path
            .strip_prefix("/api/v1/devices/")
            .is_some_and(|id| id == device.0))
        || path.starts_with("/api/v1/devices/current/push")
}

pub async fn reject_changes(req: Request, next: Next) -> Response {
    if is_blocked(req.method(), req.uri().path()) && !device_owned_write(&req) {
        return AppError::new(
            StatusCode::FORBIDDEN,
            ApiError::new(
                "DEMO_READ_ONLY",
                "This is a read-only demo. Install Pipedash to connect your own providers.",
            ),
        )
        .into_response();
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        routing::post,
        Router,
    };
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn blocks_configuration_and_pipeline_actions() {
        let pipeline = "/api/v1/pipelines/github__1__acme__web__ci";
        let blocked = [
            (Method::POST, "/api/v1/factory-reset".to_string()),
            (Method::POST, "/api/v1/providers".to_string()),
            (Method::PUT, "/api/v1/providers/1".to_string()),
            (Method::DELETE, "/api/v1/providers/1".to_string()),
            (
                Method::PUT,
                "/api/v1/providers/1/refresh-interval".to_string(),
            ),
            (Method::POST, format!("{pipeline}/trigger")),
            (Method::POST, format!("{pipeline}/runs/7/cancel")),
            (Method::POST, format!("{pipeline}/runs/7/rerun")),
            (Method::PUT, "/api/v1/settings/general".to_string()),
            (
                Method::PUT,
                "/api/v1/preferences/table/1/pipeline_runs".to_string(),
            ),
            (Method::POST, "/api/v1/vault/lock".to_string()),
            (Method::POST, "/api/v1/setup/config".to_string()),
            (Method::DELETE, "/api/v1/cache".to_string()),
            (Method::GET, "/api/v1/storage/config/content".to_string()),
            (Method::GET, "/api/v1/providers/export".to_string()),
            (Method::GET, "/api/v1/push/subscriptions".to_string()),
            (Method::DELETE, "/api/v1/devices/abc".to_string()),
        ];

        for (method, path) in blocked {
            assert!(
                is_blocked(&method, &path),
                "{method} {path} should be blocked"
            );
        }
    }

    #[test]
    fn allows_reads_and_queries() {
        let allowed = [
            (Method::GET, "/api/v1/pipelines/cached"),
            (Method::GET, "/api/v1/providers/1"),
            (Method::GET, "/api/v1/ws"),
            (Method::GET, "/"),
            (Method::GET, "/assets/index.js"),
            (Method::OPTIONS, "/api/v1/providers"),
            (Method::POST, "/api/v1/pipelines/p/runs/7/logs/job/search"),
            (
                Method::POST,
                "/api/v1/pipelines/p/runs/7/logs/job/download-ticket",
            ),
            (Method::POST, "/api/v1/metrics/aggregated"),
            (Method::POST, "/api/v1/metrics/pipelines/p/query"),
            (Method::POST, "/api/v1/providers/1/refresh"),
            (Method::POST, "/api/v1/refresh/all"),
            (Method::POST, "/api/v1/vault/unlock"),
            (Method::POST, "/api/v1/devices/pairing"),
        ];

        for (method, path) in allowed {
            assert!(
                !is_blocked(&method, path),
                "{method} {path} should be allowed"
            );
        }
    }

    #[test]
    fn demo_devices_get_a_name_without_the_owner_s_hostname() {
        assert_eq!(device_name("ios"), "Demo iPhone or iPad");
        assert_eq!(device_name("macos"), "Demo Mac");
        assert_eq!(device_name("linux"), "Demo Linux PC");
    }

    #[tokio::test]
    async fn rejected_requests_return_the_demo_error() {
        let app = Router::new()
            .route("/api/v1/factory-reset", post(|| async { "ok" }))
            .route("/api/v1/refresh/all", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn(reject_changes));
        let send = |uri: &str| {
            app.clone()
                .oneshot(axum::http::Request::post(uri).body(Body::empty()).unwrap())
        };

        let rejected = send("/api/v1/factory-reset").await.unwrap();
        assert_eq!(rejected.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(rejected.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["code"], "DEMO_READ_ONLY");

        let allowed = send("/api/v1/refresh/all").await.unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_paired_device_can_remove_only_itself() {
        let app = Router::new()
            .route(
                "/api/v1/devices/{id}",
                axum::routing::delete(|| async { "ok" }),
            )
            .layer(axum::middleware::from_fn(reject_changes));
        let remove = |path: &str, device: Option<&str>| {
            let mut request = axum::http::Request::delete(path)
                .body(Body::empty())
                .unwrap();
            if let Some(id) = device {
                request
                    .extensions_mut()
                    .insert(CurrentDevice(id.to_string()));
            }
            app.clone().oneshot(request)
        };

        let own = remove("/api/v1/devices/abc", Some("abc")).await.unwrap();
        assert_eq!(own.status(), StatusCode::OK);
        for (path, device) in [
            ("/api/v1/devices/abc", Some("other")),
            ("/api/v1/devices/abc", None),
        ] {
            let response = remove(path, device).await.unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{device:?}");
        }
    }

    #[tokio::test]
    async fn a_paired_device_manages_only_its_own_push_registration() {
        let app = Router::new()
            .route(
                "/api/v1/devices/current/push",
                axum::routing::put(|| async { "ok" }).delete(|| async { "ok" }),
            )
            .route("/api/v1/devices/current/push/test", post(|| async { "ok" }))
            .route(
                "/api/v1/devices/other",
                axum::routing::put(|| async { "ok" }),
            )
            .layer(axum::middleware::from_fn(reject_changes));
        let send = |method: Method, path: &str, device: Option<&str>| {
            let mut request = axum::http::Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap();
            if let Some(id) = device {
                request
                    .extensions_mut()
                    .insert(CurrentDevice(id.to_string()));
            }
            app.clone().oneshot(request)
        };

        for (method, path) in [
            (Method::PUT, "/api/v1/devices/current/push"),
            (Method::DELETE, "/api/v1/devices/current/push"),
            (Method::POST, "/api/v1/devices/current/push/test"),
        ] {
            let own = send(method.clone(), path, Some("abc")).await.unwrap();
            assert_eq!(own.status(), StatusCode::OK, "{method} {path}");
            let anonymous = send(method.clone(), path, None).await.unwrap();
            assert_eq!(anonymous.status(), StatusCode::FORBIDDEN, "{method} {path}");
        }
        let other = send(Method::PUT, "/api/v1/devices/other", Some("abc"))
            .await
            .unwrap();
        assert_eq!(other.status(), StatusCode::FORBIDDEN);
    }
}
