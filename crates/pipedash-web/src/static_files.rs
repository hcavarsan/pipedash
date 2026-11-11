use axum::{
    body::Body,
    http::{
        header,
        response::Builder,
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

    finish(
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime_type)
            .header(header::CACHE_CONTROL, cache_control),
        Body::from(content.to_vec()),
    )
}

fn serve_index_html(base_path: &str) -> Response {
    match StaticAssets::get(INDEX_HTML) {
        Some(content) => finish(
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .header(header::CACHE_CONTROL, "no-cache"),
            Body::from(with_base_href(
                &String::from_utf8_lossy(&content.data),
                base_path,
            )),
        ),
        None => finish(
            Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR),
            Body::from("Frontend not built. Run: bun run build"),
        ),
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
    finish(
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain"),
        Body::from("404 Not Found"),
    )
}

fn finish(builder: Builder, body: Body) -> Response {
    builder
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
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
mod tests;
