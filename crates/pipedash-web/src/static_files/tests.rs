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
        "/pipelines/stub__3__acme%2Fapi%2Fci.yml",
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

    for uri in ["/assets/missing-abc123.js", "/assets/missing.svg"] {
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
