use std::io::{
    Read,
    Write,
};
use std::net::TcpListener;
use std::thread::{
    self,
    JoinHandle,
};
use std::time::Duration;

use super::*;

fn server(responses: Vec<(u16, Value)>) -> (CircleCIClient, JoinHandle<Vec<String>>) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..end]);
                    let length = headers
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                        .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(bytes).unwrap());
            let body = body.to_string();
            let retry = if matches!(status, 429 | 503) {
                "Retry-After: 12\r\n"
            } else {
                ""
            };
            write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{retry}Connection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        requests
    });
    let client = CircleCIClient::new(
        Arc::new(reqwest::Client::new()),
        "test-token".into(),
        &origin,
    )
    .unwrap();
    (client, handle)
}

fn pipeline(number: i64) -> Value {
    json!({"id": format!("id-{number}"), "number": number, "state": "created",
        "created_at": "2026-01-01T00:00:00Z"})
}

#[tokio::test]
async fn pipeline_pages_preserve_order_encode_cursors_and_stop_at_limit() {
    let (client, handle) = server(vec![
        (
            200,
            json!({"items": [pipeline(4)], "next_page_token": "a+b&c"}),
        ),
        (
            200,
            json!({"items": [pipeline(3), pipeline(2)], "next_page_token": "unused"}),
        ),
    ]);
    assert!(client
        .fetch_pipelines("gh/org/repo", 0)
        .await
        .unwrap()
        .is_empty());
    let pipelines = client.fetch_pipelines("gh/org/repo", 2).await.unwrap();
    assert_eq!(
        pipelines
            .iter()
            .map(|pipeline| pipeline.number)
            .collect::<Vec<_>>(),
        [4, 3]
    );
    let requests = handle.join().unwrap();
    assert!(requests[0].starts_with("GET /api/v2/project/gh/org/repo/pipeline HTTP"));
    assert!(requests[1].contains("?page-token=a%2Bb%26c "));
    assert!(requests[0]
        .to_lowercase()
        .contains("circle-token: test-token\r\n"));
    assert!(requests[0]
        .to_lowercase()
        .contains("accept: application/json\r\n"));
    assert!(!requests[0].lines().next().unwrap().contains("test-token"));
}

#[tokio::test]
async fn workflows_follow_all_pages_and_encode_ids() {
    let (client, handle) = server(vec![
        (
            200,
            json!({"items": [{"id": "one", "name": "build", "status": "success"}], "next_page_token": "next"}),
        ),
        (
            200,
            json!({"items": [{"id": "two", "name": "test", "status": "failed"}], "next_page_token": null}),
        ),
    ]);
    let workflows = client.fetch_workflows("id/with?query").await.unwrap();
    assert_eq!(workflows.len(), 2);
    assert_eq!(workflows[1].status, "failed");
    let requests = handle.join().unwrap();
    assert!(requests[0].starts_with("GET /api/v2/pipeline/id%2Fwith%3Fquery/workflow "));
    assert!(requests[1].contains("?page-token=next "));
}

#[tokio::test]
async fn pagination_rejects_repeated_tokens_and_propagates_later_errors() {
    let page = json!({"items": [], "next_page_token": "same"});
    let (client, handle) = server(vec![(200, page.clone()), (200, page)]);
    assert!(matches!(
        client.fetch_workflows("id").await,
        Err(PluginError::ApiError(_))
    ));
    handle.join().unwrap();
    let (client, handle) = server(vec![
        (
            200,
            json!({"items": [pipeline(1)], "next_page_token": "next"}),
        ),
        (503, json!({"message": "unavailable"})),
    ]);
    assert!(matches!(
        client.fetch_pipelines("gh/org/repo", 2).await,
        Err(PluginError::ServiceUnavailable { .. })
    ));
    handle.join().unwrap();
}

#[tokio::test]
async fn triggers_send_branch_and_typed_parameters_and_return_pipeline_number() {
    let (client, handle) = server(vec![
        (201, json!({"number": 42})),
        (201, json!({"number": 43})),
    ]);
    let body = json!({"branch": "release/next", "parameters": {"deploy": true, "count": 3, "name": "ship"}});
    assert_eq!(client.trigger("gh/org/repo", &body).await.unwrap(), 42);
    assert_eq!(client.trigger("bb/org/repo", &json!({})).await.unwrap(), 43);
    assert!(matches!(
        client.trigger("circleci/org/project", &body).await,
        Err(PluginError::NotSupported(_))
    ));
    let requests = handle.join().unwrap();
    assert!(requests[0].starts_with("POST /api/v2/project/gh/org/repo/pipeline "));
    assert!(requests[1].starts_with("POST /api/v2/project/bb/org/repo/pipeline "));
    assert_eq!(
        serde_json::from_str::<Value>(requests[0].split_once("\r\n\r\n").unwrap().1).unwrap(),
        body
    );
}

#[tokio::test]
async fn actions_reject_failures_and_missing_trigger_numbers() {
    let (client, handle) = server(vec![
        (201, json!({"id": "uuid"})),
        (404, json!({"message": "missing"})),
        (202, json!({"message": "canceled"})),
        (202, json!({"workflow_id": "new"})),
    ]);
    assert!(matches!(
        client.trigger("gh/org/repo", &json!({})).await,
        Err(PluginError::ApiError(_))
    ));
    assert!(matches!(
        client.cancel("missing").await,
        Err(PluginError::PipelineNotFound(_))
    ));
    client.cancel("workflow/id").await.unwrap();
    client.rerun("workflow/id", true).await.unwrap();
    let requests = handle.join().unwrap();
    assert!(requests[2].starts_with("POST /api/v2/workflow/workflow%2Fid/cancel "));
    assert!(requests[3].starts_with("POST /api/v2/workflow/workflow%2Fid/rerun "));
    assert_eq!(
        serde_json::from_str::<Value>(requests[3].split_once("\r\n\r\n").unwrap().1).unwrap(),
        json!({"from_failed": true})
    );
}

#[tokio::test]
async fn app_pipelines_run_by_definition_and_report_skipped_triggers() {
    let (client, handle) = server(vec![
        (
            200,
            json!({"items": [{"id": "def-1", "name": "build"}], "next_page_token": null}),
        ),
        (201, json!({"number": 7, "id": "p", "state": "created"})),
        (
            200,
            json!({"message": "Ignoring pipeline due to CI skip in the commit"}),
        ),
    ]);
    let definitions = client.fetch_definitions("project-uuid").await.unwrap();
    assert_eq!(definitions[0].id, "def-1");
    let body = json!({"definition_id": "def-1", "config": {"branch": "main"}, "checkout": {"branch": "main"}});
    assert_eq!(
        client
            .run_pipeline("circleci/org/project", &body)
            .await
            .unwrap(),
        7
    );
    match client.run_pipeline("circleci/org/project", &body).await {
        Err(PluginError::ApiError(message)) => assert!(message.contains("CI skip")),
        other => panic!("{other:?}"),
    }
    let requests = handle.join().unwrap();
    assert!(requests[0].starts_with("GET /api/v2/projects/project-uuid/pipeline-definitions HTTP"));
    assert!(requests[1].starts_with("POST /api/v2/project/circleci/org/project/pipeline/run "));
}

#[tokio::test]
async fn job_steps_use_the_v1_project_path_and_step_output_goes_without_the_token() {
    let (client, handle) = server(vec![
        (
            200,
            json!({"steps": [{"name": "Checkout code", "actions": [{"index": 0, "status": "success",
            "output_url": "https://example.invalid/out?sig=1"}]}]}),
        ),
        (200, json!({"steps": []})),
    ]);
    let steps = client.fetch_job_steps("gh/org/repo", 12).await.unwrap();
    assert_eq!(steps.steps[0].name, "Checkout code");
    client
        .fetch_job_steps("circleci/org-id/project-id", 13)
        .await
        .unwrap();
    let requests = handle.join().unwrap();
    assert!(requests[0].starts_with("GET /api/v1.1/project/github/org/repo/12 "));
    assert!(requests[1].starts_with("GET /api/v1.1/project/circleci/org-id/project-id/13 "));

    let (client, handle) = server(vec![(
        200,
        json!([{"message": "hello\r\n", "time": "2026-01-01T00:00:00Z", "type": "out"}]),
    )]);
    let origin = client.base_url.clone();
    let output = client
        .fetch_step_output(&format!("{origin}output/step?X-Amz-Signature=secret"), 1024)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(output[0].message, "hello\r\n");
    let request = handle.join().unwrap().remove(0);
    assert!(request.starts_with("GET /output/step?X-Amz-Signature=secret "));
    assert!(!request.to_lowercase().contains("circle-token"));
}

#[tokio::test]
async fn expired_step_output_errors_hide_the_signed_query() {
    let (client, handle) = server(vec![(403, json!({"message": "expired"}))]);
    let origin = client.base_url.clone();
    let error = client
        .fetch_step_output(&format!("{origin}output?X-Amz-Signature=secret"), 1024)
        .await
        .unwrap_err();
    assert!(!error.to_string().contains("secret"), "{error}");
    handle.join().unwrap();
    for url in [
        "file:///etc/passwd".to_string(),
        "http://169.254.169.254/latest".to_string(),
        format!("http://user:pw@{}/out", origin.host_str().unwrap()),
    ] {
        assert!(client.fetch_step_output(&url, 1024).await.is_err(), "{url}");
    }
}

#[tokio::test]
async fn step_output_larger_than_the_limit_is_left_out_unread() {
    let big = json!([{"message": "x".repeat(4096)}]);
    let (client, handle) = server(vec![(200, big)]);
    let origin = client.base_url.clone();
    assert!(client
        .fetch_step_output(&format!("{origin}out"), 1024)
        .await
        .unwrap()
        .is_none());
    handle.join().unwrap();
}

#[tokio::test]
async fn shared_http_errors_preserve_retry_delays_and_hide_response_secrets() {
    for status in [401, 403, 404, 429, 503, 302, 400] {
        let (client, handle) = server(vec![(status, json!({"message": "test-token"}))]);
        let error = client.validate_credentials().await.unwrap_err();
        match status {
            401 => assert!(matches!(error, PluginError::AuthenticationFailed(_))),
            403 => assert!(matches!(error, PluginError::PermissionDenied(_))),
            404 => assert!(matches!(error, PluginError::PipelineNotFound(_))),
            429 => assert!(matches!(error, PluginError::RateLimited { .. })),
            503 => assert!(matches!(error, PluginError::ServiceUnavailable { .. })),
            _ => assert!(matches!(error, PluginError::ApiError(_))),
        }
        if matches!(status, 429 | 503) {
            assert_eq!(error.retry_after(), Some(Duration::from_secs(12)));
        }
        assert!(!error.to_string().contains("test-token"));
        handle.join().unwrap();
    }
}

#[tokio::test]
async fn discovers_followed_projects_and_fetches_app_projects_and_details() {
    let (client, handle) = server(vec![
        (
            200,
            json!([{"vcs_type": "github", "username": "org", "reponame": "repo",
            "vcs_url": "https://github.com/org/repo", "default_branch": "main"}]),
        ),
        (
            200,
            json!({"slug": "circleci/org/project", "name": "app", "organization_name": "org",
            "vcs_info": {"vcs_url": "https://github.com/org/app", "default_branch": "develop"}}),
        ),
        (200, pipeline(42)),
        (200, json!({"items": "invalid"})),
    ]);
    let projects = client.fetch_projects().await.unwrap();
    assert_eq!(projects[0].slug, "gh/org/repo");
    assert_eq!(projects[0].default_branch.as_deref(), Some("main"));
    let project = client.fetch_project("circleci/org/project").await.unwrap();
    assert_eq!(project.name, "app");
    assert_eq!(project.default_branch.as_deref(), Some("develop"));
    assert_eq!(
        client
            .fetch_pipeline("circleci/org/project", 42)
            .await
            .unwrap()
            .number,
        42
    );
    assert!(matches!(
        client.fetch_pipelines("gh/org/repo", 1).await,
        Err(PluginError::SerializationError(_))
    ));
    let requests = handle.join().unwrap();
    assert!(requests[0].starts_with("GET /api/v1.1/projects "));
    assert!(requests[2].starts_with("GET /api/v2/project/circleci/org/project/pipeline/42 "));
}

#[tokio::test]
async fn discovers_projects_when_vcs_type_is_omitted() {
    let (client, handle) = server(vec![(
        200,
        json!([
            {"username": "org", "reponame": "repo", "vcs_url": "https://github.com/org/repo"},
            {"username": "org", "reponame": "app", "vcs_url": "https://bitbucket.org/org/app"}
        ]),
    )]);
    let projects = client.fetch_projects().await.unwrap();
    assert_eq!(projects[0].slug, "gh/org/repo");
    assert_eq!(projects[1].slug, "bb/org/app");
    handle.join().unwrap();
}

#[test]
fn rejects_unsafe_origins_and_project_paths() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    for origin in [
        "ftp://circleci.com",
        "https://user:token@circleci.com",
        "https://circleci.com/api",
        "https://circleci.com?token=x",
    ] {
        assert!(
            CircleCIClient::new(Arc::new(reqwest::Client::new()), "token".into(), origin).is_err()
        );
    }
    for slug in [
        "gh/org",
        "gh/org/repo/extra",
        "gh/../repo",
        "gh/org/repo?x",
        "gh/org/%2e%2e",
        "other/org/repo",
    ] {
        assert!(validate_project_slug(slug).is_err(), "{slug}");
    }
    for slug in ["gh/org/repo", "bb/org/repo", "circleci/org-id/project-id"] {
        validate_project_slug(slug).unwrap();
    }
}
