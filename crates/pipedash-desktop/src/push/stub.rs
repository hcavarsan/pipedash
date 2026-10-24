use std::sync::{
    Arc,
    Mutex,
    PoisonError,
};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::net::{
    TcpListener,
    TcpStream,
};
use tokio::sync::Notify;

#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Value,
}

#[derive(Default)]
pub struct Gate {
    pub arrived: Notify,
    pub release: Notify,
}

pub struct Reply {
    pub status: u16,
    pub body: String,
    pub gate: Option<Arc<Gate>>,
}

impl Reply {
    pub fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            body: body.to_string(),
            gate: None,
        }
    }

    pub fn empty(status: u16) -> Self {
        Self {
            status,
            body: String::new(),
            gate: None,
        }
    }

    pub fn held_by(mut self, gate: &Arc<Gate>) -> Self {
        self.gate = Some(Arc::clone(gate));
        self
    }
}

type Handler = Arc<dyn Fn(&Request) -> Reply + Send + Sync>;

pub struct Stub {
    pub url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Stub {
    pub async fn start(handler: impl Fn(&Request) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let handler: Handler = Arc::new(handler);
        let recorded = Arc::clone(&requests);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(serve(stream, Arc::clone(&handler), Arc::clone(&recorded)));
            }
        });
        Self { url, requests }
    }

    pub fn requests(&self) -> Vec<Request> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn count(&self, method: &str, path_prefix: &str) -> usize {
        self.requests()
            .iter()
            .filter(|r| r.method == method && r.path.starts_with(path_prefix))
            .count()
    }

    pub async fn saw(&self, method: &str, path_prefix: &str, within: Duration) -> bool {
        tokio::time::timeout(within, async {
            while self.count(method, path_prefix) == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .is_ok()
    }
}

async fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    let head_end = loop {
        if let Some(position) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break position + 4;
        }
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_owned();
    let path = request_line.next()?.to_owned();
    let length = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
        .unwrap_or(0);

    while buffer.len() < head_end + length {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = serde_json::from_slice(&buffer[head_end..head_end + length]).unwrap_or(Value::Null);

    Some(Request { method, path, body })
}

async fn serve(mut stream: TcpStream, handler: Handler, recorded: Arc<Mutex<Vec<Request>>>) {
    let Some(request) = read_request(&mut stream).await else {
        return;
    };
    let reply = handler(&request);
    recorded
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(request);
    if let Some(gate) = &reply.gate {
        gate.arrived.notify_one();
        gate.release.notified().await;
    }
    let response = format!(
        "HTTP/1.1 {} Stub\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        reply.status,
        reply.body.len(),
        reply.body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}
