use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::sync::Arc;
use std::time::Duration;

use pipedash_plugin_api::PluginError;

use super::*;
use crate::test_support::{
    stub_server,
    Reply,
};

fn unauthorized(_: &str) -> Reply {
    Reply {
        status: "401 Unauthorized",
        headers: "Content-Type: application/json\r\n",
        body: r#"{"message":"Authentication required. Please supply a valid API Access Token"}"#,
    }
}

fn not_found(_: &str) -> Reply {
    Reply {
        status: "404 Not Found",
        headers: "",
        body: r#"{"message":"No pipeline found"}"#,
    }
}

fn rate_limited(_: &str) -> Reply {
    Reply {
        status: "429 Too Many Requests",
        headers: "Retry-After: 120\r\n",
        body: "slow down",
    }
}

fn unavailable(_: &str) -> Reply {
    Reply {
        status: "503 Service Unavailable",
        headers: "",
        body: "maintenance",
    }
}

fn created(_: &str) -> Reply {
    Reply {
        status: "201 Created",
        headers: "Content-Type: application/json\r\n",
        body: "{}",
    }
}

async fn client_for(
    reply: fn(&str) -> Reply, delay: Duration,
) -> (BuildkiteClient, Arc<AtomicUsize>) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let requests = Arc::new(AtomicUsize::new(0));
    let base_url = stub_server(requests.clone(), delay, reply).await;
    let http_client = Client::builder()
        .timeout(Duration::from_millis(300))
        .build()
        .unwrap();
    let client =
        BuildkiteClient::new(Arc::new(http_client), "token".to_string()).with_base_url(base_url);
    (client, requests)
}

fn build_history_page(path: &str) -> Reply {
    static PAGES: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
        (0..11)
            .map(|page| {
                serde_json::to_string(
                    &(page * 100 + 1..=((page + 1) * 100).min(1025))
                        .map(|number| {
                            serde_json::json!({
                                "id": format!("build-{number}"),
                                "number": number,
                                "state": "passed",
                                "web_url": format!("https://buildkite.com/acme/app/{number}"),
                                "created_at": "2026-01-01T00:00:00Z"
                            })
                        })
                        .collect::<Vec<_>>(),
                )
                .unwrap()
            })
            .collect()
    });
    let page = path
        .strip_prefix("/organizations/acme/pipelines/app/builds?per_page=100&page=")
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!((1..=11).contains(&page), "Unexpected build page: {page}");
    Reply {
        status: "200 OK",
        headers: "Content-Type: application/json\r\n",
        body: &PAGES[page - 1],
    }
}

#[tokio::test]
async fn build_history_includes_page_eleven_and_stops_after_a_short_page() {
    let (client, requests) = client_for(build_history_page, Duration::ZERO).await;

    let builds = client.fetch_builds("acme", "app", 1200).await.unwrap();

    assert_eq!(builds.len(), 1025);
    for (index, build) in builds.iter().enumerate() {
        let number = index + 1;
        assert_eq!(build.id, format!("build-{number}"));
        assert_eq!(build.number, number as i64);
    }
    assert_eq!(requests.load(Ordering::SeqCst), 11);
}

#[tokio::test]
async fn build_history_preserves_the_requested_limit_on_page_eleven() {
    let (client, requests) = client_for(build_history_page, Duration::ZERO).await;

    let builds = client.fetch_builds("acme", "app", 1010).await.unwrap();

    assert_eq!(builds.len(), 1010);
    for (index, build) in builds.iter().enumerate() {
        let number = index + 1;
        assert_eq!(build.id, format!("build-{number}"));
        assert_eq!(build.number, number as i64);
    }
    assert_eq!(requests.load(Ordering::SeqCst), 11);
}

fn organization_page(path: &str) -> Reply {
    static PAGES: std::sync::LazyLock<[String; 2]> = std::sync::LazyLock::new(|| {
        [30, 100].map(|count| {
            serde_json::to_string(
                &(1..=count)
                    .map(|n| {
                        serde_json::json!({"slug": format!("org-{n}"), "name": format!("Org {n}")})
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap()
        })
    });
    let body = match path {
        "/organizations" => PAGES[0].as_str(),
        "/organizations?per_page=100&page=1" => PAGES[1].as_str(),
        "/organizations?per_page=100&page=2" => r#"[{"slug":"org-101","name":"Org 101"}]"#,
        _ => panic!("Unexpected organization request: {path}"),
    };
    Reply {
        status: "200 OK",
        headers: "Content-Type: application/json\r\n",
        body,
    }
}

#[tokio::test]
async fn organizations_include_later_pages() {
    let (client, requests) = client_for(organization_page, Duration::ZERO).await;

    let organizations = client.fetch_organizations().await.unwrap();

    assert_eq!(organizations.first().unwrap().slug, "org-1");
    assert_eq!(organizations.last().unwrap().slug, "org-101");
    assert_eq!(organizations.len(), 101);
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn organizations_stop_after_a_short_page() {
    fn single_page(_: &str) -> Reply {
        Reply {
            status: "200 OK",
            headers: "Content-Type: application/json\r\n",
            body: r#"[{"slug":"acme","name":"Acme"}]"#,
        }
    }
    let (client, requests) = client_for(single_page, Duration::ZERO).await;

    let organizations = client.fetch_organizations().await.unwrap();

    assert_eq!(organizations.len(), 1);
    assert_eq!(organizations[0].slug, "acme");
    assert_eq!(organizations[0].name, "Acme");
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn organizations_stop_after_an_empty_page() {
    fn empty_last_page(path: &str) -> Reply {
        if path.ends_with("page=2") {
            Reply {
                status: "200 OK",
                headers: "Content-Type: application/json\r\n",
                body: "[]",
            }
        } else {
            organization_page(path)
        }
    }
    let (client, requests) = client_for(empty_last_page, Duration::ZERO).await;

    let organizations = client.fetch_organizations().await.unwrap();

    assert_eq!(organizations.len(), 100);
    assert_eq!(organizations.last().unwrap().slug, "org-100");
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn organizations_fail_when_a_later_page_is_rejected() {
    fn rejected_page(path: &str) -> Reply {
        if path.ends_with("page=2") {
            unauthorized(path)
        } else {
            organization_page(path)
        }
    }
    let (client, requests) = client_for(rejected_page, Duration::ZERO).await;

    let result = client.fetch_organizations().await;

    assert!(matches!(result, Err(PluginError::AuthenticationFailed(_))));
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn rejected_token_fails_as_auth_error_without_retry() {
    let (client, requests) = client_for(unauthorized, Duration::ZERO).await;

    let results = [
        client.fetch_organizations().await.err(),
        client
            .fetch_org_pipelines("acme".to_string(), 1, 10)
            .await
            .err(),
        client
            .fetch_pipeline("acme".to_string(), "app".to_string())
            .await
            .err(),
        client.fetch_builds("acme", "app", 10).await.err(),
    ];

    for error in results {
        assert!(
            matches!(error, Some(PluginError::AuthenticationFailed(_))),
            "{error:?}"
        );
    }
    assert_eq!(requests.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn missing_pipeline_is_pipeline_not_found() {
    let (client, requests) = client_for(not_found, Duration::ZERO).await;

    let error = client
        .fetch_pipeline("acme".to_string(), "gone".to_string())
        .await
        .unwrap_err();

    assert!(
        matches!(error, PluginError::PipelineNotFound(_)),
        "{error:?}"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn rate_limit_carries_retry_after_and_is_not_retried() {
    let (client, requests) = client_for(rate_limited, Duration::ZERO).await;

    let error = client.fetch_organizations().await.unwrap_err();

    assert!(
        matches!(error, PluginError::RateLimited { .. }),
        "{error:?}"
    );
    assert_eq!(error.retry_after(), Some(Duration::from_secs(120)));
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn server_error_is_service_unavailable_and_retried() {
    let (client, requests) = client_for(unavailable, Duration::ZERO).await;

    let error = client.fetch_organizations().await.unwrap_err();

    assert!(
        matches!(error, PluginError::ServiceUnavailable { .. }),
        "{error:?}"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn rate_limited_mutations_map_status_without_resending() {
    let (client, requests) = client_for(rate_limited, Duration::ZERO).await;

    let trigger = client
        .trigger_build("acme", "app", "main".to_string(), None)
        .await
        .unwrap_err();
    let cancel = client.cancel_build("acme", "app", 7).await.unwrap_err();

    assert!(
        matches!(trigger, PluginError::RateLimited { .. }),
        "{trigger:?}"
    );
    assert!(
        matches!(cancel, PluginError::RateLimited { .. }),
        "{cancel:?}"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn timed_out_mutations_are_not_sent_again() {
    let (client, requests) = client_for(created, Duration::from_secs(2)).await;

    let trigger = client
        .trigger_build("acme", "app", "main".to_string(), None)
        .await
        .unwrap_err();
    let cancel = client.cancel_build("acme", "app", 7).await.unwrap_err();

    assert!(
        matches!(trigger, PluginError::NetworkError(_)),
        "{trigger:?}"
    );
    assert!(matches!(cancel, PluginError::NetworkError(_)), "{cancel:?}");
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}
