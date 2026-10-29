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
        None if is_asset_path(path) => not_found(),
        None => serve_index_html(base_path),
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

fn is_asset_path(path: &str) -> bool {
    match path.split_once('/') {
        Some((dir, _)) => StaticAssets::iter().any(|file| {
            file.strip_prefix(dir)
                .is_some_and(|rest| rest.starts_with('/'))
        }),
        None => path.contains('.'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn pipeline_links_with_dots_in_the_id_open_the_app() {
        for uri in [
            "/pipelines/gitea__3__acme%2Fapi%2Fci.yml",
            "/pipelines/web.prod",
        ] {
            let response = serve_static(uri.parse().unwrap(), "/");

            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            assert_eq!(
                response.headers()[header::CONTENT_TYPE],
                "text/html; charset=utf-8",
                "{uri}"
            );
        }
    }

    #[test]
    fn missing_files_in_built_asset_folders_are_not_found() {
        let index = StaticAssets::get(INDEX_HTML).expect("dist/index.html");
        if String::from_utf8_lossy(&index.data).contains("Frontend not built") {
            return;
        }

        for uri in ["/assets/missing-abc123.js", "/provider-icons/missing.svg"] {
            let response = serve_static(uri.parse().unwrap(), "/");

            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
        }
    }

    #[tokio::test]
    async fn built_index_html_carries_the_configured_base_href() {
        let index = StaticAssets::get(INDEX_HTML).expect("dist/index.html");
        if String::from_utf8_lossy(&index.data).contains("Frontend not built") {
            return;
        }

        for uri in ["/", "/pipelines/some-id"] {
            let response = serve_static(uri.parse().unwrap(), "/pipedash/");
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let html = String::from_utf8_lossy(&body);

            assert!(
                html.contains(r#"<base href="/pipedash/" />"#),
                "{uri}: {html}"
            );
        }
    }
}
