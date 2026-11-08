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
            || (route.starts_with("push/") && route != "push/relay");
    }

    !(*method == Method::POST && allows_post(route))
}

fn allows_post(route: &str) -> bool {
    route == "vault/unlock"
        || route == "refresh/all"
        || route == "metrics/dashboard"
        || route == "devices/pairing"
        || (route.starts_with("providers/") && route.ends_with("/refresh"))
        || (route.starts_with("pipelines/")
            && (route.ends_with("/search")
                || route.ends_with("/download-ticket")
                || route.ends_with("/runs/query")))
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
        || path == "/api/v1/devices/current/notification-preferences"
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
mod tests;
