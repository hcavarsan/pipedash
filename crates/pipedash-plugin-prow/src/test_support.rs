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
