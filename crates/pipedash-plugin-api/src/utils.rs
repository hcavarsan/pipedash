use std::future::Future;
use std::time::Duration;

use reqwest::Url;

use crate::http::map_send_error;
use crate::{
    PluginError,
    PluginResult,
};

pub struct RetryPolicy {
    pub max_retries: usize,
    pub initial_delay: Duration,
    pub exponential_backoff: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            exponential_backoff: true,
        }
    }
}

impl RetryPolicy {
    pub fn new(max_retries: usize, initial_delay: Duration, exponential_backoff: bool) -> Self {
        Self {
            max_retries,
            initial_delay,
            exponential_backoff,
        }
    }

    pub async fn retry<F, Fut, T>(&self, operation: F) -> PluginResult<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = PluginResult<T>>,
    {
        let mut delay = self.initial_delay;
        let mut attempt = 1;

        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) if attempt < self.max_retries && Self::should_retry(&e, delay) => {
                    tokio::time::sleep(delay).await;
                    delay = self.next_delay(delay);
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub async fn retry_unsent<F, Fut>(&self, url: &Url, send: F) -> PluginResult<reqwest::Response>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<reqwest::Response, reqwest::Error>>,
    {
        let mut delay = self.initial_delay;
        let mut attempt = 1;

        loop {
            match send().await {
                Ok(response) => return Ok(response),
                Err(e) if attempt < self.max_retries && e.is_connect() && !e.is_timeout() => {
                    tokio::time::sleep(delay).await;
                    delay = self.next_delay(delay);
                    attempt += 1;
                }
                Err(e) => return Err(map_send_error(e, url)),
            }
        }
    }

    fn should_retry(error: &PluginError, delay: Duration) -> bool {
        error.is_transient() && error.retry_after().is_none_or(|hint| hint <= delay)
    }

    fn next_delay(&self, delay: Duration) -> Duration {
        if self.exponential_backoff {
            delay.saturating_mul(2)
        } else {
            delay
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{
        Read,
        Write,
    };
    use std::net::TcpListener;
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };
    use std::sync::Arc;

    use super::*;

    fn fast_policy() -> RetryPolicy {
        RetryPolicy::new(3, Duration::from_millis(5), false)
    }

    fn client(timeout: Duration) -> reqwest::Client {
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::Client::builder().timeout(timeout).build().unwrap()
    }

    async fn attempts_for(error: fn() -> PluginError) -> usize {
        let attempts = AtomicUsize::new(0);
        let result: PluginResult<()> = fast_policy()
            .retry(|| async {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(error())
            })
            .await;
        assert!(result.is_err());
        attempts.load(Ordering::SeqCst)
    }

    #[tokio::test]
    async fn test_retry_success() {
        let policy = RetryPolicy::default();
        let result = policy.retry(|| async { Ok::<_, PluginError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_eventual_success() {
        let policy = RetryPolicy::new(3, Duration::from_millis(10), false);
        let attempts = std::cell::Cell::new(0);

        let result = policy
            .retry(|| async {
                let count = attempts.get() + 1;
                attempts.set(count);
                if count < 2 {
                    Err(PluginError::NetworkError("Temporary failure".to_string()))
                } else {
                    Ok(42)
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn retries_network_and_short_hint_unavailable() {
        assert_eq!(
            attempts_for(|| PluginError::NetworkError("x".into())).await,
            3
        );
        assert_eq!(
            attempts_for(|| PluginError::ServiceUnavailable {
                message: "x".into(),
                retry_after: None,
            })
            .await,
            3
        );
        assert_eq!(
            attempts_for(|| PluginError::ServiceUnavailable {
                message: "x".into(),
                retry_after: Some(Duration::from_millis(1)),
            })
            .await,
            3
        );
    }

    #[tokio::test]
    async fn does_not_retry_permanent_errors_or_long_hints() {
        assert_eq!(
            attempts_for(|| PluginError::AuthenticationFailed("x".into())).await,
            1
        );
        assert_eq!(
            attempts_for(|| PluginError::PipelineNotFound("x".into())).await,
            1
        );
        assert_eq!(attempts_for(|| PluginError::ApiError("x".into())).await, 1);
        assert_eq!(
            attempts_for(|| PluginError::RateLimited {
                message: "x".into(),
                retry_after: Some(Duration::from_secs(120)),
            })
            .await,
            1
        );
        assert_eq!(
            attempts_for(|| PluginError::ServiceUnavailable {
                message: "x".into(),
                retry_after: Some(Duration::from_secs(120)),
            })
            .await,
            1
        );
    }

    #[tokio::test]
    async fn retry_unsent_retries_connect_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let url = Url::parse(&format!("http://{addr}/trigger")).unwrap();
        let client = client(Duration::from_secs(5));
        let attempts = AtomicUsize::new(0);

        let result = fast_policy()
            .retry_unsent(&url, || {
                attempts.fetch_add(1, Ordering::SeqCst);
                client.post(url.clone()).send()
            })
            .await;

        assert!(matches!(result, Err(PluginError::NetworkError(_))));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_unsent_never_resends_after_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let received = Arc::new(AtomicUsize::new(0));
        let counter = received.clone();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut buf = [0u8; 1024];
                if stream.read(&mut buf).unwrap_or(0) > 0 {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(300));
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
            }
        });
        let url = Url::parse(&format!("http://{addr}/trigger")).unwrap();
        let client = client(Duration::from_millis(50));

        let result = fast_policy()
            .retry_unsent(&url, || client.post(url.clone()).send())
            .await;

        assert!(matches!(result, Err(PluginError::NetworkError(_))));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(received.load(Ordering::SeqCst), 1);
    }
}
