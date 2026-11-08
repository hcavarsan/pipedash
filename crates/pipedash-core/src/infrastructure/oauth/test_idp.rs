use std::sync::{
    Arc,
    Mutex,
};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde_json::{
    json,
    Value,
};
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};

pub(crate) fn jwt(claims: Value) -> String {
    format!(
        "{}.{}.sig",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    )
}

#[derive(Clone)]
pub(crate) struct Idp {
    pub base_url: String,
    pub forms: Arc<Mutex<Vec<String>>>,
    reply: Arc<Mutex<(u16, String)>>,
}

impl Idp {
    pub(crate) async fn start() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let idp = Self {
            base_url: base_url.clone(),
            forms: Arc::default(),
            reply: Arc::new(Mutex::new((200, "{}".into()))),
        };
        let server = idp.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let server = server.clone();
                tokio::spawn(async move {
                    let (target, body) = read_request(&mut stream).await;
                    let (status, reply) = if target.ends_with("/.well-known/openid-configuration") {
                        (
                            200,
                            json!({
                                "authorization_endpoint": format!("{}/auth", server.base_url),
                                "token_endpoint": format!("{}/token", server.base_url),
                                "scopes_supported": ["openid", "email", "offline_access"],
                            })
                            .to_string(),
                        )
                    } else {
                        server.forms.lock().unwrap().push(body);
                        server.reply.lock().unwrap().clone()
                    };
                    let response = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                        reply.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        idp
    }

    pub(crate) fn token_endpoint(&self) -> String {
        format!("{}/token", self.base_url)
    }

    pub(crate) fn reply(&self, status: u16, body: Value) {
        *self.reply.lock().unwrap() = (status, body.to_string());
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, String) {
    let mut data = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..end]).to_string();
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if data.len() >= end + 4 + length {
                let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let body = String::from_utf8_lossy(&data[end + 4..end + 4 + length]);
                return (target, body.to_string());
            }
        }
        match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => return (String::new(), String::new()),
            Ok(n) => data.extend_from_slice(&buffer[..n]),
        }
    }
}
