use axum::{
    body::Body,
    http::{
        header,
        HeaderValue,
        StatusCode,
        Uri,
    },
    response::Response,
};
use rust_embed::Embed;

const INDEX_HTML: &str = "index.html";
const DEFAULT_BASE_HREF: &str = r#"<base href="/" />"#;

#[derive(Embed)]
#[folder = "dist/"]
pub struct StaticAssets;

pub fn serve_static(uri: Uri, base_path: &str) -> Response {
    let path = uri.path().trim_start_matches('/');

    if path.is_empty() || path == INDEX_HTML {
        return serve_index_html(base_path);
    }

    match StaticAssets::get(path) {
        Some(content) => serve_file_content(content.data.as_ref(), path),
        None => {
            if path_has_no_extension(path) {
                serve_index_html(base_path)
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

fn serve_index_html(base_path: &str) -> Response {
    match StaticAssets::get(INDEX_HTML) {
        Some(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(with_base_href(
                &String::from_utf8_lossy(&content.data),
                base_path,
            )))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::from("Frontend not built. Run: bun run build"))
            .unwrap(),
    }
}

fn with_base_href(html: &str, base_path: &str) -> String {
    html.replacen(
        DEFAULT_BASE_HREF,
        &format!(r#"<base href="{base_path}" />"#),
        1,
    )
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

    #[test]
    fn index_html_gets_the_configured_base_href() {
        let html = r#"<head><base href="/" /><script src="./assets/index.js"></script></head>"#;

        assert_eq!(with_base_href(html, "/"), html);
        assert_eq!(
            with_base_href(html, "/pipedash/"),
            r#"<head><base href="/pipedash/" /><script src="./assets/index.js"></script></head>"#
        );
    }

    #[test]
    fn test_serve_static_root() {
        let uri: Uri = "/".parse().unwrap();
        let response = serve_static(uri, "/");
        assert!(
            response.status() == StatusCode::OK
                || response.status() == StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_serve_static_missing_file() {
        let uri: Uri = "/nonexistent.xyz".parse().unwrap();
        let response = serve_static(uri, "/");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_serve_static_spa_route() {
        let uri: Uri = "/pipelines".parse().unwrap();
        let response = serve_static(uri, "/");
        assert!(
            response.status() == StatusCode::OK
                || response.status() == StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
