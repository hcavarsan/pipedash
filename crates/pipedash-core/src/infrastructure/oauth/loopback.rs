use std::net::{
    IpAddr,
    Ipv4Addr,
};
use std::time::Duration;

use reqwest::Url;
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::net::{
    TcpListener,
    TcpStream,
};
use tokio::task::JoinSet;

const MAX_HEAD_BYTES: usize = 8 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(10);
const NOT_FOUND: &str = "404 Not Found";

pub(crate) struct Loopback {
    listener: TcpListener,
    redirect_uri: Url,
}

impl Loopback {
    pub(crate) async fn bind(redirect_uri: &Url) -> Option<Self> {
        let host = redirect_uri.host_str()?;
        let is_loopback = host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback());
        if redirect_uri.scheme() != "http" || !is_loopback {
            return None;
        }
        let port = redirect_uri.port()?;
        match TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await {
            Ok(listener) => Some(Self {
                listener,
                redirect_uri: redirect_uri.clone(),
            }),
            Err(error) => {
                tracing::warn!(port, %error, "Couldn't listen for the sign-in redirect; the address must be pasted");
                None
            }
        }
    }

    pub(crate) async fn accept(&self) -> std::io::Result<(TcpStream, Url)> {
        let mut requests = JoinSet::new();
        loop {
            tokio::select! {
                accepted = self.listener.accept() => {
                    let (stream, _) = accepted?;
                    requests.spawn(read_target(stream));
                }
                Some(Ok(Some((stream, target)))) = requests.join_next() => {
                    match self.redirect_uri.join(&target) {
                        Ok(url) if url.path() == self.redirect_uri.path() => return Ok((stream, url)),
                        _ => reply(stream, NOT_FOUND, "Not found", "").await,
                    }
                }
            }
        }
    }
}

pub(crate) async fn respond(stream: TcpStream, outcome: &Result<(), String>) {
    match outcome {
        Ok(()) => {
            reply(
                stream,
                "200 OK",
                "Signed in",
                "You can close this tab and go back to Pipedash.",
            )
            .await
        }
        Err(message) => {
            reply(
                stream,
                "400 Bad Request",
                "Couldn't sign in",
                &escape_html(message),
            )
            .await
        }
    }
}

async fn read_target(mut stream: TcpStream) -> Option<(TcpStream, String)> {
    let mut head = Vec::new();
    let mut buffer = [0; 1024];
    let read_head = async {
        while !head.windows(4).any(|w| w == b"\r\n\r\n") {
            if head.len() > MAX_HEAD_BYTES {
                return None;
            }
            let read = stream.read(&mut buffer).await.ok().filter(|n| *n > 0)?;
            head.extend_from_slice(&buffer[..read]);
        }
        Some(())
    };
    tokio::time::timeout(READ_TIMEOUT, read_head).await.ok()??;

    let request_line = std::str::from_utf8(&head).ok()?.lines().next()?;
    let mut parts = request_line.split(' ');
    let (method, target, version) = (parts.next()?, parts.next()?, parts.next()?);
    if method != "GET" || !version.starts_with("HTTP/1.") || !target.starts_with('/') {
        return None;
    }
    let target = target.to_string();
    Some((stream, target))
}

async fn reply(mut stream: TcpStream, status: &str, title: &str, message: &str) {
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{title}</title></head>\
         <body style=\"font-family:system-ui,sans-serif;text-align:center;margin-top:20vh\"><h1>{title}</h1><p>{message}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    if let Err(error) = stream.write_all(response.as_bytes()).await {
        tracing::debug!(%error, "Couldn't answer the sign-in redirect");
    }
    let _ = stream.shutdown().await;
}

fn escape_html(text: &str) -> String {
    text.chars()
        .fold(String::with_capacity(text.len()), |mut escaped, c| {
            match c {
                '&' => escaped.push_str("&amp;"),
                '<' => escaped.push_str("&lt;"),
                '>' => escaped.push_str("&gt;"),
                '"' => escaped.push_str("&quot;"),
                '\'' => escaped.push_str("&#39;"),
                _ => escaped.push(c),
            }
            escaped
        })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) async fn free_redirect_uri() -> Url {
        let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = probe.local_addr().unwrap().port();
        Url::parse(&format!("http://localhost:{port}/auth/callback")).unwrap()
    }

    pub(crate) async fn get(url: &Url) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", url.port().unwrap()))
            .await
            .unwrap();
        let target = match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_string(),
        };
        stream
            .write_all(format!("GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        let status = response[9..12].parse().unwrap();
        (status, response)
    }

    #[tokio::test]
    async fn accept_skips_other_paths_and_returns_the_callback() {
        let redirect_uri = free_redirect_uri().await;
        let loopback = Loopback::bind(&redirect_uri).await.unwrap();
        let idle = TcpStream::connect(("127.0.0.1", redirect_uri.port().unwrap()))
            .await
            .unwrap();

        let favicon = redirect_uri.join("/favicon.ico").unwrap();
        let callback = redirect_uri.join("/auth/callback?code=c&state=s").unwrap();
        let client = tokio::spawn(async move { (get(&favicon).await.0, get(&callback).await) });

        let (stream, url) = loopback.accept().await.unwrap();
        respond(stream, &Err("<bad>".into())).await;
        let (favicon_status, (callback_status, body)) = client.await.unwrap();

        assert_eq!(url.query(), Some("code=c&state=s"));
        assert_eq!(favicon_status, 404);
        assert_eq!(callback_status, 400);
        assert!(body.contains("&lt;bad&gt;"));
        drop(idle);
    }

    #[tokio::test]
    async fn bind_rejects_non_loopback_redirects() {
        for uri in [
            "https://localhost:8085/cb",
            "http://example.com:8085/cb",
            "http://localhost/cb",
        ] {
            assert!(Loopback::bind(&Url::parse(uri).unwrap()).await.is_none());
        }
    }
}
