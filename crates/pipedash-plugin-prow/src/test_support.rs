use std::collections::HashMap;
use std::io::{
    Read,
    Write,
};
use std::net::TcpListener;
use std::sync::{
    Arc,
    Mutex,
};

use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::json;

#[derive(Debug, Clone)]
pub(crate) struct Request {
    pub method: String,
    pub target: String,
    pub head: String,
    pub body: Vec<u8>,
}

impl Request {
    pub(crate) fn header(&self, name: &str) -> Option<String> {
        self.head
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(key, _)| key.trim().eq_ignore_ascii_case(name))
            .map(|(_, value)| value.trim().to_string())
    }
}

pub(crate) struct Response {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

impl Response {
    fn with_type(status: u16, content_type: &str, body: &str) -> Self {
        Self {
            status,
            headers: vec![("Content-Type", content_type.to_string())],
            body: body.as_bytes().to_vec(),
        }
    }

    pub(crate) fn json(status: u16, body: &str) -> Self {
        Self::with_type(status, "application/json", body)
    }

    pub(crate) fn text(status: u16, body: &str) -> Self {
        Self::with_type(status, "text/plain; charset=utf-8", body)
    }

    pub(crate) fn html(status: u16, body: &str) -> Self {
        Self::with_type(status, "text/html; charset=utf-8", body)
    }

    pub(crate) fn gzip_json(body: &str) -> Self {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(body.as_bytes()).unwrap();
        Self {
            status: 200,
            headers: vec![
                ("Content-Type", "application/json".to_string()),
                ("Content-Encoding", "gzip".to_string()),
            ],
            body: encoder.finish().unwrap(),
        }
    }

    pub(crate) fn header(mut self, name: &'static str, value: &str) -> Self {
        self.headers.push((name, value.to_string()));
        self
    }
}

pub(crate) type Requests = Arc<Mutex<Vec<Request>>>;

pub(crate) fn serve(
    handler: impl Fn(&Request) -> Response + Send + Sync + 'static,
) -> (String, Requests) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let requests: Requests = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&requests);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") {
                match stream.read(&mut byte) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => head.push(byte[0]),
                }
            }
            let head = String::from_utf8_lossy(&head).to_string();
            let mut parts = head.split_whitespace();
            let mut request = Request {
                method: parts.next().unwrap_or_default().to_string(),
                target: parts.next().unwrap_or_default().to_string(),
                head: head.clone(),
                body: Vec::new(),
            };
            let length = request
                .header("content-length")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            request.body = vec![0u8; length];
            let _ = stream.read_exact(&mut request.body);

            let response = handler(&request);
            log.lock().unwrap().push(request);

            let mut raw = format!("HTTP/1.1 {} Stub\r\n", response.status);
            for (name, value) in &response.headers {
                raw.push_str(&format!("{name}: {value}\r\n"));
            }
            raw.push_str(&format!(
                "Content-Length: {}\r\nConnection: close\r\n\r\n",
                response.body.len()
            ));
            let _ = stream.write_all(raw.as_bytes());
            let _ = stream.write_all(&response.body);
        }
    });
    (format!("http://{addr}"), requests)
}

const GCS_PAGE_SIZE: usize = 2;

pub(crate) fn gcs(objects: Vec<(String, String)>) -> impl Fn(&Request) -> Response + Send + Sync {
    move |request| {
        let url = reqwest::Url::parse(&format!("http://stub{}", request.target)).unwrap();
        if url.path() == "/storage/v1/b/bucket/o" {
            let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
            return Response::json(200, &gcs_listing(&objects, &query));
        }
        url.path()
            .strip_prefix("/bucket/")
            .and_then(|name| objects.iter().find(|(key, _)| key == name))
            .map_or_else(
                || Response::text(404, "No such object"),
                |(_, body)| Response::text(200, body),
            )
    }
}

fn gcs_listing(objects: &[(String, String)], query: &HashMap<String, String>) -> String {
    let get = |key: &str| query.get(key).map(String::as_str);
    let prefix = get("prefix").unwrap_or_default();
    let mut names: Vec<&str> = objects
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| name.starts_with(prefix))
        .filter(|name| get("startOffset").is_none_or(|start| *name >= start))
        .filter(|name| get("endOffset").is_none_or(|end| *name < end))
        .collect();
    names.sort_unstable();
    let delimiter = get("delimiter");
    let mut entries: Vec<String> = match delimiter {
        Some(delimiter) => names
            .iter()
            .filter_map(|name| name[prefix.len()..].split_once(delimiter))
            .map(|(head, _)| format!("{prefix}{head}{delimiter}"))
            .collect(),
        None => names.iter().map(|name| name.to_string()).collect(),
    };
    entries.dedup();
    let start: usize = get("pageToken")
        .and_then(|token| token.parse().ok())
        .unwrap_or(0);
    let page = entries.iter().skip(start).take(GCS_PAGE_SIZE);
    let next = (start + GCS_PAGE_SIZE < entries.len()).then(|| (start + GCS_PAGE_SIZE).to_string());
    match delimiter {
        Some(_) => json!({ "prefixes": page.collect::<Vec<_>>(), "nextPageToken": next }),
        None => json!({
            "items": page.map(|name| json!({ "name": name })).collect::<Vec<_>>(),
            "nextPageToken": next,
        }),
    }
    .to_string()
}
