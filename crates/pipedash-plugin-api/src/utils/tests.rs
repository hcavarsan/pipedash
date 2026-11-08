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

#[tokio::test]
async fn ids_without_ci_are_dropped_and_unchecked_ones_kept_in_order() {
    let ids = ["empty", "ci", "unreachable", "ci-too"]
        .map(String::from)
        .to_vec();

    let kept = retain_with_ci(ids, |id| async move {
        match id.as_str() {
            "empty" => Ok(false),
            "unreachable" => Err(PluginError::NetworkError("reset".into())),
            _ => Ok(true),
        }
    })
    .await;

    assert_eq!(kept, ["ci", "unreachable", "ci-too"]);
}

#[test]
fn conclusion_mapping_uses_supplied_lists() {
    let map = |c| {
        status_from_conclusion(
            "completed",
            Some(c),
            &["success", "neutral"],
            &["cancelled"],
        )
    };
    assert_eq!(map("neutral"), PipelineStatus::Success);
    assert_eq!(map("cancelled"), PipelineStatus::Cancelled);
    assert_eq!(map("skipped"), PipelineStatus::Skipped);
    assert_eq!(map("boom"), PipelineStatus::Failed);
    assert_eq!(
        status_from_conclusion("queued", None, &[], &[]),
        PipelineStatus::Running
    );
    assert_eq!(
        status_from_conclusion("x", None, &[], &[]),
        PipelineStatus::Pending
    );
}

#[test]
fn durations_are_signed_and_optional() {
    let a = parse_timestamp("2026-01-01T00:00:10Z");
    let b = parse_timestamp("2026-01-01T00:00:00Z");
    assert_eq!(duration_seconds(b, a), Some(10));
    assert_eq!(duration_seconds(a, b), Some(-10));
    assert_eq!(duration_seconds(None, a), None);
    assert!(parse_timestamp("nope").is_none());
}

#[test]
fn collect_partial_skips_permanent_and_stops_on_transient() {
    let mut skipped = 0;
    let ok = collect_partial(
        [Ok(1), Err(PluginError::ApiError("x".into())), Ok(2)],
        |_| skipped += 1,
    )
    .unwrap();
    assert_eq!((ok, skipped), (vec![1, 2], 1));
    let transient = PluginError::ServiceUnavailable {
        message: "x".into(),
        retry_after: None,
    };
    assert!(collect_partial([Ok(1), Err(transient)], |_| {}).is_err());
}

#[test]
fn repository_name_ignores_hosts_ports_and_suffixes() {
    for url in [
        "https://git.example.com/acme/web.git",
        "https://git.example.com/acme/web.git/",
        "http://git:3000/acme/web",
        "ssh://git@git.example.com:2222/acme/web",
        "git@git.example.com:acme/web.git",
        "git@git.example.com:acme/web/",
    ] {
        assert_eq!(repository_name(url), "acme/web", "{url}");
    }
    assert_eq!(repository_name("web"), "web");
}

#[tokio::test]
async fn cached_fetches_once_and_keeps_errors_uncached() {
    let cache = Cached::<u32>::default();
    let calls = AtomicUsize::new(0);
    let failed = cache
        .get_or_fetch(|| async { Err(PluginError::NetworkError("down".into())) })
        .await;
    assert!(failed.is_err());
    for _ in 0..2 {
        let value = cache
            .get_or_fetch(|| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(7)
            })
            .await;
        assert_eq!(value.unwrap(), 7);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn svg_data_url_keeps_unreserved_and_safe_bytes() {
    let safe = "AZaz09-._~/:;,=!*'()";
    assert_eq!(svg_data_url(safe), format!("data:image/svg+xml,{safe}"));
    assert_eq!(svg_data_url(""), "data:image/svg+xml,");
}

#[test]
fn svg_data_url_encodes_reserved_characters_and_markup() {
    assert_eq!(
        svg_data_url("<svg viewBox=\"0 0 1 1\"/>"),
        "data:image/svg+xml,%3Csvg%20viewBox=%220%200%201%201%22/%3E"
    );
    assert_eq!(
        svg_data_url("?&@+[]{}|\\^`$"),
        "data:image/svg+xml,%3F%26%40%2B%5B%5D%7B%7D%7C%5C%5E%60%24"
    );
}

#[test]
fn svg_data_url_encodes_hash_and_percent_so_fragments_and_escapes_survive() {
    assert_eq!(
        svg_data_url("fill=\"#fff\" 100%"),
        "data:image/svg+xml,fill=%22%23fff%22%20100%25"
    );
}

#[test]
fn svg_data_url_encodes_every_utf8_byte_of_non_ascii_text() {
    assert_eq!(svg_data_url("é✓"), "data:image/svg+xml,%C3%A9%E2%9C%93");
    assert_eq!(svg_data_url("a\nb\tc"), "data:image/svg+xml,a%0Ab%09c");
}

#[test]
fn selected_items_trims_and_skips_blank_entries() {
    let config = HashMap::from([("selected_items".to_string(), " a ,, b,".to_string())]);
    assert_eq!(selected_items(&config).collect::<Vec<_>>(), ["a", "b"]);
    assert_eq!(selected_items(&HashMap::new()).count(), 0);
}

#[test]
fn provider_type_strips_the_plugin_crate_prefix() {
    assert_eq!(provider_type("pipedash-plugin-acme"), "acme");
    assert_eq!(provider_type("pipedash-plugin-acme-cloud"), "acme-cloud");
}

#[test]
fn provider_type_returns_other_names_unchanged() {
    assert_eq!(provider_type("pipedash-plugins"), "pipedash-plugins");
    assert_eq!(provider_type("pipedash-plugin-"), "");
    assert_eq!(provider_type("pipedash-plugin"), "pipedash-plugin");
    assert_eq!(provider_type("acme"), "acme");
    assert_eq!(provider_type(""), "");
}

#[test]
fn provider_type_is_usable_in_const_context() {
    const TYPE: &str = provider_type("pipedash-plugin-acme");
    assert_eq!(TYPE, "acme");
}
