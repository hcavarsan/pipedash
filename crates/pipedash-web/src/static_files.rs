use axum::{
    body::Body,
    http::{
        header,
        HeaderValue,
        StatusCode,
        Uri,
    },
    response::{
        IntoResponse,
        Response,
    },
};
use rust_embed::Embed;

const INDEX_HTML: &str = "index.html";

#[derive(Embed)]
#[folder = "dist/"]
pub struct StaticAssets;

pub async fn serve_static(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if path.is_empty() || path == INDEX_HTML {
        return serve_index_html();
    }

    match StaticAssets::get(path) {
        Some(content) => serve_file_content(content.data.as_ref(), path),
        None => {
            if path_has_no_extension(path) {
                serve_index_html()
            } else {
                not_found()
            }
        }
    }
}

fn serve_file_content(content: &[u8], path: &str) -> Response {
    let mime_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    let cache_control = determine_cache_control(path);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CACHE_CONTROL, cache_control)
        .body(Body::from(content.to_vec()))
        .unwrap()
}

fn serve_index_html() -> Response {
    match StaticAssets::get(INDEX_HTML) {
        Some(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(content.data))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Frontend not built. Run: bun run build"))
            .unwrap(),
    }
}

fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from("404 Not Found"))
        .unwrap()
}

fn determine_cache_control(path: &str) -> HeaderValue {
    if path.starts_with("assets/") {
        HeaderValue::from_static("public, immutable, max-age=31536000")
    } else if path == INDEX_HTML {
        HeaderValue::from_static("no-cache")
    } else {
        HeaderValue::from_static("public, max-age=3600")
    }
}

fn path_has_no_extension(path: &str) -> bool {
    let last_segment = path.rsplit('/').next().unwrap_or(path);
    !last_segment.contains('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_has_no_extension() {
        assert!(path_has_no_extension("pipelines"));
        assert!(path_has_no_extension("pipelines/some-id"));
        assert!(!path_has_no_extension("favicon.ico"));
        assert!(!path_has_no_extension("assets/vendor-abc123.js"));
    }

    #[test]
    fn test_determine_cache_control() {
        assert_eq!(
            determine_cache_control("assets/vendor-abc123.js"),
            "public, immutable, max-age=31536000"
        );
        assert_eq!(determine_cache_control("index.html"), "no-cache");
        assert_eq!(
            determine_cache_control("favicon.ico"),
            "public, max-age=3600"
        );
    }

    #[tokio::test]
    async fn test_serve_static_root() {
        let uri: Uri = "/".parse().unwrap();
        let response = serve_static(uri).await.into_response();
        assert!(
            response.status() == StatusCode::OK
                || response.status() == StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn test_serve_static_missing_file() {
        let uri: Uri = "/nonexistent.xyz".parse().unwrap();
        let response = serve_static(uri).await.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_serve_static_spa_route() {
        let uri: Uri = "/pipelines".parse().unwrap();
        let response = serve_static(uri).await.into_response();
        assert!(
            response.status() == StatusCode::OK
                || response.status() == StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
