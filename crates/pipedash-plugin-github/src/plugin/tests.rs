use std::sync::Arc;
use std::time::Duration;

use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::sync::Mutex;

use super::*;

const PIPELINE_ID: &str = "acme__app__42";
const PENDING_PATH: &str = "/api/v3/repos/acme/app/actions/runs/9001/pending_deployments";

type Recorded = Arc<Mutex<Vec<(String, String, String)>>>;

fn waiting_run() -> serde_json::Value {
    let url = "http://stub.invalid/acme/app";
    serde_json::json!({
        "id": 9001, "workflow_id": 42, "node_id": "n", "name": "Deploy",
        "head_branch": "main", "head_sha": "abc", "run_number": 7, "event": "push",
        "status": "waiting", "conclusion": null,
        "created_at": "2026-10-20T10:00:00Z", "updated_at": "2026-10-20T10:01:00Z",
        "url": url, "html_url": url, "jobs_url": url, "logs_url": url,
        "check_suite_url": url, "artifacts_url": url, "cancel_url": url,
        "rerun_url": url, "workflow_url": url,
        "head_commit": {
            "id": "abc", "tree_id": "def", "message": "ship it",
            "timestamp": "2026-10-20T09:59:00Z",
            "author": {"name": "Ada", "email": "ada@example.com"},
            "committer": {"name": "Ada", "email": "ada@example.com"}
        },
        "repository": {"id": 1, "name": "app", "url": url}
    })
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, String, String) {
    let mut request = Vec::new();
    let mut buffer = [0; 4096];
    let head_end = loop {
        if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
        let count = stream.read(&mut buffer).await.unwrap();
        assert_ne!(count, 0, "stub client closed early");
        request.extend_from_slice(&buffer[..count]);
    };
    let head = String::from_utf8(request[..head_end].to_vec()).unwrap();
    let length: usize = head
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(str::to_string)
        })
        .map(|value| value.trim().parse().unwrap())
        .unwrap_or(0);
    while request.len() < head_end + length {
        let count = stream.read(&mut buffer).await.unwrap();
        request.extend_from_slice(&buffer[..count]);
    }
    let body = String::from_utf8(request[head_end..head_end + length].to_vec()).unwrap();
    let mut first_line = head.split_whitespace();
    let method = first_line.next().unwrap().to_string();
    let target = first_line.next().unwrap();
    let path = target.split('?').next().unwrap().to_string();
    (method, path, body)
}

struct Reply {
    status: u16,
    headers: Vec<(&'static str, &'static str)>,
    body: serde_json::Value,
}

async fn serve(
    config: &[(&str, &str)], handler: impl Fn(&str, &str) -> Reply + Send + 'static,
) -> (GitHubPlugin, Recorded, tokio::task::JoinHandle<()>) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let recorded: Recorded = Arc::default();
    let log = recorded.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (method, path, body) = read_request(&mut stream).await;
            let reply = handler(&method, &path);
            log.lock().await.push((method, path, body));
            let body = match reply.body {
                serde_json::Value::String(text) => text,
                json => json.to_string(),
            };
            let extra: String = reply
                .headers
                .iter()
                .map(|(name, value)| format!("{name}: {value}\r\n"))
                .collect();
            let response = if reply.status == 204 {
                "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_string()
            } else {
                format!(
                    "HTTP/1.1 {} Stub\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    reply.status,
                    body.len()
                )
            };
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let mut plugin = GitHubPlugin::new();
    let mut settings = config_with(config);
    settings.insert("base_url".to_string(), format!("http://{addr}"));
    plugin.initialize(1, settings, test_client()).unwrap();
    (plugin, recorded, server)
}

async fn stub_github(
    pending_deployments: serde_json::Value,
) -> (GitHubPlugin, Recorded, tokio::task::JoinHandle<()>) {
    serve(&[], move |method, path| match (method, path) {
        ("GET", "/api/v3/repos/acme/app/actions/workflows/42/runs") => {
            ok(serde_json::json!({"total_count": 1, "workflow_runs": [waiting_run()]}))
        }
        ("GET", PENDING_PATH) => ok(pending_deployments.clone()),
        ("POST", PENDING_PATH) => ok(serde_json::json!([])),
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await
}

fn pending(id: u64, name: &str, can_approve: bool) -> serde_json::Value {
    serde_json::json!({
        "environment": {"id": id, "name": name},
        "wait_timer": 0,
        "current_user_can_approve": can_approve,
        "reviewers": []
    })
}

async fn posts(recorded: &Recorded) -> Vec<serde_json::Value> {
    recorded
        .lock()
        .await
        .iter()
        .filter(|(method, _, _)| method == "POST")
        .map(|(_, _, body)| serde_json::from_str(body).unwrap())
        .collect()
}

#[test]
fn trigger_run_matching_uses_second_precision() {
    let trigger_time = "2026-10-20T10:00:00.500Z".parse().unwrap();
    let cases = [
        ("2026-10-20T10:00:00Z", 8, "main", true),
        ("2026-10-20T09:59:59Z", 8, "main", false),
        ("2026-10-20T10:00:01Z", 8, "main", true),
        ("2026-10-20T10:00:00Z", 7, "main", false),
        ("2026-10-20T10:00:00Z", 8, "other", false),
    ];
    for (created_at, run_number, head_branch, expected) in cases {
        let mut run: crate::types::Run = serde_json::from_value(waiting_run()).unwrap();
        run.created_at = created_at.parse().unwrap();
        run.run_number = run_number;
        run.head_branch = head_branch.to_string();

        assert_eq!(
            GitHubPlugin::matches_trigger(&run, 7, "main", trigger_time),
            expected,
            "created_at={created_at}, run_number={run_number}, head_branch={head_branch}"
        );
    }
}

#[tokio::test]
async fn trigger_accepts_204_no_content_from_dispatch() {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };

    let dispatched_at = std::sync::Mutex::new(None);
    let history_reads = AtomicUsize::new(0);
    let (plugin, recorded, server) = serve(&[], move |method, path| match (method, path) {
        ("POST", "/api/v3/repos/acme/app/actions/workflows/42/dispatches") => {
            *dispatched_at.lock().unwrap() =
                Some(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
            Reply {
                status: 204,
                headers: Vec::new(),
                body: serde_json::Value::Null,
            }
        }
        ("GET", "/api/v3/repos/acme/app/actions/workflows/42/runs") => {
            let mut run = waiting_run();
            if let Some(created_at) = dispatched_at.lock().unwrap().as_ref() {
                let first_read = history_reads.fetch_add(1, Ordering::SeqCst) == 0;
                run["run_number"] = serde_json::json!(if first_read { 8 } else { 9 });
                run["head_branch"] =
                    serde_json::json!(if first_read { "release" } else { "other" });
                run["created_at"] = serde_json::json!(created_at);
            }
            ok(serde_json::json!({"total_count": 1, "workflow_runs": [run]}))
        }
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await;

    let result = plugin
        .trigger_pipeline(TriggerParams {
            workflow_id: PIPELINE_ID.to_string(),
            inputs: Some(serde_json::json!({
                "ref": "release",
                "inputs.ref": "abc123",
                "inputs.environment": "production",
                "inputs.dry_run": false
            })),
        })
        .await;
    server.abort();

    assert_eq!(
        result.unwrap(),
        TriggerResult {
            run_id: None,
            run_number: Some(8),
        }
    );
    assert_eq!(
        posts(&recorded).await,
        [serde_json::json!({
            "ref": "release",
            "inputs": {"ref": "abc123", "environment": "production", "dry_run": false}
        })]
    );
}

#[tokio::test]
async fn trigger_preserves_dispatch_errors_without_retrying() {
    for status in [401, 403, 429, 503] {
        let (plugin, recorded, server) = serve(&[], move |method, path| match (method, path) {
            ("GET", "/api/v3/repos/acme/app/actions/workflows/42/runs") => {
                ok(serde_json::json!({"total_count": 0, "workflow_runs": []}))
            }
            ("POST", "/api/v3/repos/acme/app/actions/workflows/42/dispatches") => {
                failure(status, "Dispatch refused", &[])
            }
            (method, path) => panic!("Unexpected stub request: {method} {path}"),
        })
        .await;

        let error = plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: PIPELINE_ID.to_string(),
                inputs: None,
            })
            .await
            .unwrap_err();
        server.abort();

        assert!(
            matches!(
                (status, &error),
                (401, PluginError::AuthenticationFailed(_))
                    | (403, PluginError::PermissionDenied(_))
                    | (429, PluginError::RateLimited { .. })
                    | (503, PluginError::ServiceUnavailable { .. })
            ),
            "{status}: {error}"
        );
        assert_eq!(posts(&recorded).await, [serde_json::json!({"ref": "main"})]);
    }
}

#[tokio::test]
async fn trigger_rejects_invalid_workflow_ids_before_dispatching() {
    let (plugin, recorded, server) = serve(&[], |_, _| ok(serde_json::json!({}))).await;

    let error = plugin
        .trigger_pipeline(TriggerParams {
            workflow_id: "acme__app__invalid".to_string(),
            inputs: None,
        })
        .await
        .unwrap_err();
    server.abort();

    assert!(matches!(error, PluginError::InvalidConfig(_)), "{error}");
    assert!(recorded.lock().await.is_empty());
}

#[tokio::test]
async fn approve_posts_only_environments_the_token_can_approve() {
    let (plugin, recorded, server) = stub_github(serde_json::json!([
        pending(11, "production", true),
        pending(12, "staging", true),
        pending(13, "audit", false),
    ]))
    .await;

    plugin
        .review_run(PIPELINE_ID, 7, ReviewDecision::Approve)
        .await
        .unwrap();

    assert_eq!(
        posts(&recorded).await,
        [serde_json::json!({
            "environment_ids": [11, 12],
            "state": "approved",
            "comment": "Approved in Pipedash"
        })]
    );
    server.abort();
}

#[tokio::test]
async fn reject_posts_rejected_state() {
    let (plugin, recorded, server) =
        stub_github(serde_json::json!([pending(11, "production", true)])).await;

    plugin
        .review_run(PIPELINE_ID, 7, ReviewDecision::Reject)
        .await
        .unwrap();

    assert_eq!(
        posts(&recorded).await,
        [serde_json::json!({
            "environment_ids": [11],
            "state": "rejected",
            "comment": "Rejected in Pipedash"
        })]
    );
    server.abort();
}

#[tokio::test]
async fn review_fails_without_posting_when_token_can_approve_none() {
    let (plugin, recorded, server) = stub_github(serde_json::json!([
        pending(11, "production", false),
        pending(12, "staging", false),
    ]))
    .await;

    let error = plugin
        .review_run(PIPELINE_ID, 7, ReviewDecision::Approve)
        .await
        .unwrap_err();

    assert!(matches!(error, PluginError::PermissionDenied(_)), "{error}");
    assert!(error.to_string().contains("production, staging"), "{error}");
    assert!(posts(&recorded).await.is_empty());
    server.abort();
}

#[tokio::test]
async fn review_fails_without_posting_when_run_waits_for_nothing() {
    let (plugin, recorded, server) = stub_github(serde_json::json!([])).await;

    let error = plugin
        .review_run(PIPELINE_ID, 7, ReviewDecision::Approve)
        .await
        .unwrap_err();

    assert!(matches!(error, PluginError::PipelineNotFound(_)), "{error}");
    assert!(posts(&recorded).await.is_empty());
    server.abort();
}

#[tokio::test]
async fn waiting_run_is_pending_and_flagged_for_review() {
    let (plugin, _recorded, server) = stub_github(serde_json::json!([])).await;

    let run = plugin.fetch_run_details(PIPELINE_ID, 7).await.unwrap();

    assert_eq!(run.status, PipelineStatus::Pending);
    assert_eq!(
        run.metadata.get("awaiting_review"),
        Some(&serde_json::json!(true))
    );
    server.abort();
}

fn finished_run(
    id: u64, run_number: i64, status: &str, conclusion: Option<&str>,
) -> serde_json::Value {
    let mut run = waiting_run();
    run["id"] = serde_json::json!(id);
    run["run_number"] = serde_json::json!(run_number);
    run["status"] = serde_json::json!(status);
    run["conclusion"] = serde_json::json!(conclusion);
    run
}

async fn stub_failed_runs(
    rerun_reply: fn() -> Reply,
) -> (GitHubPlugin, Recorded, tokio::task::JoinHandle<()>) {
    serve(&[], move |method, path| match (method, path) {
        ("GET", "/api/v3/repos/acme/app/actions/workflows/42/runs") => ok(serde_json::json!({
            "total_count": 2,
            "workflow_runs": [
                finished_run(9002, 8, "completed", Some("failure")),
                finished_run(9001, 7, "completed", Some("failure")),
            ]
        })),
        ("POST", "/api/v3/repos/acme/app/actions/runs/9001/rerun-failed-jobs")
        | ("POST", "/api/v3/repos/acme/app/actions/runs/9001/rerun") => rerun_reply(),
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await
}

#[tokio::test]
async fn reruns_post_the_matching_endpoint_for_the_run_with_that_number() {
    let (plugin, recorded, server) = stub_failed_runs(|| Reply {
        status: 201,
        headers: Vec::new(),
        body: serde_json::json!({}),
    })
    .await;

    plugin.rerun_failed_jobs(PIPELINE_ID, 7).await.unwrap();
    plugin.rerun_run(PIPELINE_ID, 7).await.unwrap();

    let posted: Vec<String> = recorded
        .lock()
        .await
        .iter()
        .filter(|(method, _, _)| method == "POST")
        .map(|(_, path, _)| path.clone())
        .collect();
    assert_eq!(
        posted,
        [
            "/api/v3/repos/acme/app/actions/runs/9001/rerun-failed-jobs",
            "/api/v3/repos/acme/app/actions/runs/9001/rerun"
        ]
    );
    server.abort();
}

#[tokio::test]
async fn rerun_failed_jobs_returns_the_github_refusal() {
    let (plugin, _recorded, server) = stub_failed_runs(|| {
        failure(
            403,
            "Unable to retry this workflow run because it was created over a month ago",
            &[],
        )
    })
    .await;

    let error = plugin.rerun_failed_jobs(PIPELINE_ID, 7).await.unwrap_err();

    assert!(matches!(error, PluginError::PermissionDenied(_)), "{error}");
    assert!(error.to_string().contains("over a month ago"), "{error}");
    server.abort();
}

#[tokio::test]
async fn force_cancel_resolves_the_selected_run_and_preserves_normal_cancel() {
    let (plugin, recorded, server) = serve(&[], |method, path| match (method, path) {
        ("GET", "/api/v3/repos/acme/app/actions/workflows/42/runs") => ok(serde_json::json!({
            "total_count": 2,
            "workflow_runs": [
                finished_run(9002, 8, "in_progress", None),
                finished_run(9001, 7, "in_progress", None),
            ]
        })),
        ("POST", "/api/v3/repos/acme/app/actions/runs/9001/cancel")
        | ("POST", "/api/v3/repos/acme/app/actions/runs/9001/force-cancel") => Reply {
            status: 202,
            headers: Vec::new(),
            body: serde_json::json!({}),
        },
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await;

    plugin.cancel_run(PIPELINE_ID, 7).await.unwrap();
    plugin.force_cancel_run(PIPELINE_ID, 7).await.unwrap();

    let posted: Vec<String> = recorded
        .lock()
        .await
        .iter()
        .filter(|(method, _, _)| method == "POST")
        .map(|(_, path, _)| path.clone())
        .collect();
    assert_eq!(
        posted,
        [
            "/api/v3/repos/acme/app/actions/runs/9001/cancel",
            "/api/v3/repos/acme/app/actions/runs/9001/force-cancel"
        ]
    );
    assert!(plugin.metadata().capabilities.force_cancel);
    server.abort();
}

#[tokio::test]
async fn force_cancel_propagates_lookup_and_action_rejections_without_other_posts() {
    for (lookup_status, action_status) in [(403, 202), (200, 403), (200, 409)] {
        let (plugin, recorded, server) = serve(&[], move |method, path| match (method, path) {
            ("GET", "/api/v3/repos/acme/app/actions/workflows/42/runs") => {
                if lookup_status != 200 {
                    return failure(lookup_status, "Lookup denied", &[]);
                }
                ok(serde_json::json!({
                    "total_count": 1,
                    "workflow_runs": [finished_run(9001, 7, "in_progress", None)]
                }))
            }
            ("POST", "/api/v3/repos/acme/app/actions/runs/9001/force-cancel") => {
                failure(action_status, "Force cancellation denied", &[])
            }
            (method, path) => panic!("Unexpected stub request: {method} {path}"),
        })
        .await;

        let error = plugin.force_cancel_run(PIPELINE_ID, 7).await.unwrap_err();
        if lookup_status == 403 || action_status == 403 {
            assert!(matches!(error, PluginError::PermissionDenied(_)), "{error}");
        }
        let message = if lookup_status != 200 {
            "Lookup denied"
        } else {
            "Force cancellation denied"
        };
        assert!(error.to_string().contains(message), "{error}");
        assert_eq!(
            recorded
                .lock()
                .await
                .iter()
                .filter(|(method, _, _)| method == "POST")
                .count(),
            usize::from(lookup_status == 200)
        );
        server.abort();
    }
}

#[test]
fn only_failed_and_timed_out_runs_offer_a_failed_jobs_rerun() {
    let cases = [
        ("completed", Some("failure"), true),
        ("completed", Some("timed_out"), true),
        ("completed", Some("success"), false),
        ("completed", Some("cancelled"), false),
        ("completed", Some("startup_failure"), false),
        ("completed", Some("action_required"), false),
        ("in_progress", None, false),
        ("waiting", None, false),
    ];
    for (status, conclusion, expected) in cases {
        let run: crate::types::Run =
            serde_json::from_value(finished_run(9001, 7, status, conclusion)).unwrap();

        let run = crate::mapper::run_to_pipeline_run(run, PIPELINE_ID);

        assert_eq!(
            run.metadata.get("can_rerun_failed_jobs") == Some(&serde_json::json!(true)),
            expected,
            "{status} {conclusion:?}"
        );
    }
}

fn failure(status: u16, message: &str, headers: &[(&'static str, &'static str)]) -> Reply {
    Reply {
        status,
        headers: headers.to_vec(),
        body: serde_json::json!({"message": message}),
    }
}

fn ok(body: serde_json::Value) -> Reply {
    Reply {
        status: 200,
        headers: Vec::new(),
        body,
    }
}

fn workflow(id: u64) -> serde_json::Value {
    let url = "http://stub.invalid/acme/app";
    serde_json::json!({
        "id": id, "node_id": "n", "name": "CI", "path": ".github/workflows/ci.yml",
        "state": "active", "created_at": "2026-10-20T10:00:00Z",
        "updated_at": "2026-10-20T10:00:00Z", "url": url, "html_url": url, "badge_url": url
    })
}

fn workflows_of_ok_repo() -> Reply {
    ok(serde_json::json!({"total_count": 1, "workflows": [workflow(42)]}))
}

fn runs_of_ok_repo() -> Reply {
    ok(serde_json::json!({"total_count": 1, "workflow_runs": [waiting_run()]}))
}

fn rate_limited() -> Reply {
    failure(429, "API rate limit exceeded", &[("Retry-After", "120")])
}

#[tokio::test]
async fn rate_limited_response_is_sent_once_and_keeps_the_hint() {
    let (plugin, recorded, server) = serve(&[], |_, _| rate_limited()).await;

    let error = plugin.validate_credentials().await.unwrap_err();

    assert!(
        matches!(
            error,
            PluginError::RateLimited { retry_after: Some(hint), .. }
                if hint == Duration::from_secs(120)
        ),
        "{error}"
    );
    assert_eq!(recorded.lock().await.len(), 1);
    server.abort();
}

#[tokio::test]
async fn server_errors_are_retried_by_the_retry_policy_only() {
    let (plugin, recorded, server) =
        serve(&[], |_, _| failure(503, "Service Unavailable", &[])).await;

    let error = plugin.validate_credentials().await.unwrap_err();

    assert!(
        matches!(error, PluginError::ServiceUnavailable { .. }),
        "{error}"
    );
    assert_eq!(
        recorded.lock().await.len(),
        RetryPolicy::default().max_retries
    );
    server.abort();
}

#[tokio::test]
async fn run_lookups_and_repository_lists_use_the_retry_policy() {
    let attempts = RetryPolicy::default().max_retries;
    let (plugin, recorded, server) =
        serve(&[], |_, _| failure(503, "Service Unavailable", &[])).await;

    let error = plugin.fetch_run_details(PIPELINE_ID, 7).await.unwrap_err();
    assert!(
        matches!(error, PluginError::ServiceUnavailable { .. }),
        "{error}"
    );
    assert_eq!(recorded.lock().await.len(), attempts);

    let error = plugin.fetch_available_pipelines(None).await.unwrap_err();
    assert!(
        matches!(error, PluginError::ServiceUnavailable { .. }),
        "{error}"
    );
    assert_eq!(recorded.lock().await.len(), attempts * 2);

    let error = plugin.fetch_run_history(PIPELINE_ID, 5).await.unwrap_err();
    assert!(
        matches!(error, PluginError::ServiceUnavailable { .. }),
        "{error}"
    );
    assert_eq!(recorded.lock().await.len(), attempts * 3);
    server.abort();
}

#[tokio::test]
async fn bad_credentials_are_authentication_failures_without_retry() {
    let (plugin, recorded, server) = serve(&[], |_, _| failure(401, "Bad credentials", &[])).await;

    let error = plugin.validate_credentials().await.unwrap_err();

    assert!(
        matches!(error, PluginError::AuthenticationFailed(_)),
        "{error}"
    );
    assert_eq!(recorded.lock().await.len(), 1);
    server.abort();
}

#[tokio::test]
async fn forbidden_is_rate_limited_only_when_github_says_so() {
    let (plugin, _recorded, server) = serve(&[], |_, _| {
        failure(
            403,
            "You have exceeded a secondary rate limit.",
            &[("Retry-After", "120")],
        )
    })
    .await;
    let error = plugin.validate_credentials().await.unwrap_err();
    assert!(matches!(error, PluginError::RateLimited { .. }), "{error}");
    server.abort();

    let (plugin, _recorded, server) =
        serve(&[], |_, _| failure(403, "Resource not accessible", &[])).await;
    let error = plugin.validate_credentials().await.unwrap_err();
    assert!(matches!(error, PluginError::PermissionDenied(_)), "{error}");
    server.abort();
}

#[tokio::test]
async fn rate_limit_on_one_repo_fails_the_whole_fetch() {
    let (plugin, _recorded, server) = serve(
        &[("selected_items", "acme/ok,acme/limited")],
        |_, path| match path {
            "/api/v3/repos/acme/ok/actions/workflows" => workflows_of_ok_repo(),
            "/api/v3/repos/acme/ok/actions/runs" => runs_of_ok_repo(),
            "/api/v3/repos/acme/limited/actions/workflows" => rate_limited(),
            path => panic!("Unexpected stub request: {path}"),
        },
    )
    .await;

    let error = plugin.fetch_pipelines().await.unwrap_err();

    assert!(matches!(error, PluginError::RateLimited { .. }), "{error}");
    server.abort();
}

#[tokio::test]
async fn missing_repo_is_skipped_while_the_others_are_returned() {
    let (plugin, _recorded, server) = serve(
        &[("selected_items", "acme/ok,acme/gone")],
        |_, path| match path {
            "/api/v3/repos/acme/ok/actions/workflows" => workflows_of_ok_repo(),
            "/api/v3/repos/acme/ok/actions/runs" => runs_of_ok_repo(),
            "/api/v3/repos/acme/gone/actions/workflows" => failure(404, "Not Found", &[]),
            path => panic!("Unexpected stub request: {path}"),
        },
    )
    .await;

    let pipelines = plugin.fetch_pipelines().await.unwrap();

    assert_eq!(pipelines.len(), 1);
    assert_eq!(pipelines[0].repository, "acme/ok");
    server.abort();
}

#[tokio::test]
async fn rate_limit_on_latest_run_fails_instead_of_reporting_pending() {
    let (plugin, _recorded, server) =
        serve(&[("selected_items", "acme/ok")], |_, path| match path {
            "/api/v3/repos/acme/ok/actions/workflows" => workflows_of_ok_repo(),
            "/api/v3/repos/acme/ok/actions/runs" => {
                ok(serde_json::json!({"total_count": 0, "workflow_runs": []}))
            }
            "/api/v3/repos/acme/ok/actions/workflows/42/runs" => rate_limited(),
            path => panic!("Unexpected stub request: {path}"),
        })
        .await;

    let error = plugin.fetch_pipelines().await.unwrap_err();

    assert!(matches!(error, PluginError::RateLimited { .. }), "{error}");
    server.abort();
}

#[tokio::test]
async fn disabled_workflow_shows_why_and_offers_enable_instead_of_its_last_status() {
    let (plugin, _recorded, server) =
        serve(&[("selected_items", "acme/app")], |_, path| match path {
            "/api/v3/repos/acme/app/actions/workflows" => {
                let mut nightly = workflow(43);
                nightly["state"] = serde_json::json!("disabled_inactivity");
                ok(serde_json::json!({"total_count": 2, "workflows": [workflow(42), nightly]}))
            }
            "/api/v3/repos/acme/app/actions/runs" => ok(serde_json::json!({
                "total_count": 1,
                "workflow_runs": [finished_run(9001, 7, "completed", Some("success"))]
            })),
            "/api/v3/repos/acme/app/actions/workflows/43/runs" => ok(serde_json::json!({
                "total_count": 1,
                "workflow_runs": [finished_run(9001, 7, "completed", Some("success"))]
            })),
            path => panic!("Unexpected stub request: {path}"),
        })
        .await;

    let pipelines = plugin.fetch_pipelines().await.unwrap();
    let workflow_with = |id: &str| pipelines.iter().find(|p| p.id.ends_with(id)).unwrap();
    let (active, nightly) = (workflow_with("__42"), workflow_with("__43"));

    assert_eq!(active.status, PipelineStatus::Success);
    assert_eq!(active.resource_state, None);
    assert!(!active.metadata.contains_key("can_enable"));
    assert_eq!(nightly.status, PipelineStatus::Skipped);
    assert_eq!(
        nightly.resource_state,
        Some(ResourceState::new(
            "Disabled",
            Some("GitHub disabled this workflow after 60 days without repository activity")
        ))
    );
    assert_eq!(nightly.metadata["can_enable"], serde_json::json!(true));
    assert_eq!(nightly.last_run, active.last_run);
    server.abort();
}

#[tokio::test]
async fn latest_runs_come_from_one_repo_runs_page() {
    let (plugin, recorded, server) =
        serve(&[("selected_items", "acme/app,acme/web")], |_, path| {
            if path.ends_with("/actions/workflows") {
                let workflows: Vec<_> = (1..=3).map(workflow).collect();
                return ok(serde_json::json!({"total_count": 3, "workflows": workflows}));
            }
            if path.ends_with("/actions/runs") {
                let runs: Vec<_> = [
                    (3, "failure"),
                    (1, "success"),
                    (2, "success"),
                    (3, "success"),
                ]
                .into_iter()
                .map(|(workflow_id, conclusion)| {
                    let mut run =
                        finished_run(9000 + workflow_id, 7, "completed", Some(conclusion));
                    run["workflow_id"] = serde_json::json!(workflow_id);
                    run
                })
                .collect();
                return ok(serde_json::json!({"total_count": 4, "workflow_runs": runs}));
            }
            panic!("Unexpected stub request: {path}")
        })
        .await;

    let pipelines = plugin.fetch_pipelines().await.unwrap();
    let requests = recorded.lock().await.len();
    server.abort();

    let status = |id: &str| pipelines.iter().find(|p| p.id == id).unwrap().status;
    assert_eq!(pipelines.len(), 6);
    assert_eq!(status("acme__app__1"), PipelineStatus::Success);
    assert_eq!(status("acme__web__3"), PipelineStatus::Failed);
    assert_eq!(requests, 4);
}

#[tokio::test]
async fn a_refused_repo_runs_page_falls_back_and_a_rate_limit_fails_the_fetch() {
    let (plugin, _recorded, server) =
        serve(&[("selected_items", "acme/ok")], |_, path| match path {
            "/api/v3/repos/acme/ok/actions/workflows" => workflows_of_ok_repo(),
            "/api/v3/repos/acme/ok/actions/runs" => failure(404, "Not Found", &[]),
            "/api/v3/repos/acme/ok/actions/workflows/42/runs" => runs_of_ok_repo(),
            path => panic!("Unexpected stub request: {path}"),
        })
        .await;
    let pipelines = plugin.fetch_pipelines().await.unwrap();
    server.abort();
    assert!(pipelines[0].last_run.is_some());

    let (plugin, _recorded, server) =
        serve(&[("selected_items", "acme/ok")], |_, path| match path {
            "/api/v3/repos/acme/ok/actions/workflows" => workflows_of_ok_repo(),
            "/api/v3/repos/acme/ok/actions/runs" => rate_limited(),
            path => panic!("Unexpected stub request: {path}"),
        })
        .await;
    let error = plugin.fetch_pipelines().await.unwrap_err();
    server.abort();
    assert!(matches!(error, PluginError::RateLimited { .. }), "{error}");
}

#[tokio::test]
async fn enable_puts_the_enable_endpoint_of_that_workflow() {
    let (plugin, recorded, server) = serve(&[], |method, path| match (method, path) {
        ("PUT", "/api/v3/repos/acme/app/actions/workflows/42/enable") => Reply {
            status: 204,
            headers: Vec::new(),
            body: serde_json::Value::Null,
        },
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await;

    plugin.enable_pipeline(PIPELINE_ID).await.unwrap();

    let requests: Vec<(String, String)> = recorded
        .lock()
        .await
        .iter()
        .map(|(method, path, _)| (method.clone(), path.clone()))
        .collect();
    assert_eq!(
        requests,
        [(
            "PUT".to_string(),
            "/api/v3/repos/acme/app/actions/workflows/42/enable".to_string()
        )]
    );
    server.abort();
}

#[tokio::test]
async fn enable_returns_the_github_refusal() {
    let (plugin, _recorded, server) = serve(&[], |_, _| {
        failure(403, "Resource not accessible by personal access token", &[])
    })
    .await;

    let error = plugin.enable_pipeline(PIPELINE_ID).await.unwrap_err();

    assert!(matches!(error, PluginError::PermissionDenied(_)), "{error}");
    assert!(
        error.to_string().contains("Resource not accessible"),
        "{error}"
    );
    server.abort();
}

async fn stub_fine_grained_token(
    config: &[(&str, &str)], contents: impl Fn(&str) -> Reply + Send + 'static,
) -> (GitHubPlugin, Recorded, tokio::task::JoinHandle<()>) {
    serve(config, move |method, path| match (method, path) {
        ("GET", "/api/v3/user") => ok(serde_json::json!({})),
        ("GET", "/api/v3/user/repos") => ok(serde_json::json!([{
            "id": 1, "name": "app", "full_name": "acme/app", "private": true,
            "url": "http://stub.invalid/acme/app"
        }])),
        ("GET", "/api/v3/user/memberships/orgs") => ok(serde_json::json!([])),
        ("GET", path) if path.ends_with("/contents") => contents(path),
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await
}

#[tokio::test]
async fn dispatch_requires_contents_access_on_every_selected_repository() {
    for granted in [true, false] {
        let (plugin, recorded, server) = stub_fine_grained_token(
            &[("selected_items", "acme/public,acme/private")],
            move |path| {
                if path.ends_with("/private/contents") && !granted {
                    failure(403, "Resource not accessible by personal access token", &[])
                } else {
                    ok(serde_json::json!([]))
                }
            },
        )
        .await;

        let mut status = plugin.check_permissions().await.unwrap();
        server.abort();
        let contents = status
            .permissions
            .iter()
            .find(|check| check.permission.name == "Contents (Read)")
            .unwrap();
        assert_eq!(contents.granted, granted);
        assert!(contents.permission.required);
        status
            .permissions
            .iter_mut()
            .find(|check| check.permission.name == "Actions (Write)")
            .unwrap()
            .granted = true;
        let features = plugin.get_feature_availability(&status);
        let dispatch = features
            .iter()
            .find(|feature| feature.feature.id == "trigger_dispatch")
            .unwrap();
        assert_eq!(dispatch.available, granted);
        assert_eq!(
            dispatch.missing_permissions,
            if granted {
                vec![]
            } else {
                vec!["Contents (Read)".to_string()]
            }
        );
        assert!(
            features
                .iter()
                .find(|feature| feature.feature.id == "cancel_workflows")
                .unwrap()
                .available
        );
        assert!(posts(&recorded).await.is_empty());
    }
}

#[tokio::test]
async fn contents_access_is_checked_before_repository_selection() {
    let (plugin, _recorded, server) = stub_fine_grained_token(&[], |path| {
        assert_eq!(path, "/api/v3/repos/acme/app/contents");
        ok(serde_json::json!([]))
    })
    .await;

    let status = plugin.check_permissions().await.unwrap();
    server.abort();

    assert!(status
        .permissions
        .iter()
        .any(|check| check.permission.name == "Contents (Read)" && check.granted));
}

#[tokio::test]
async fn contents_probe_errors_are_not_reported_as_missing_permissions() {
    for status in [401, 429, 503] {
        let (plugin, recorded, server) =
            stub_fine_grained_token(&[("selected_items", "acme/app")], move |_| {
                failure(status, "Probe failed", &[("Retry-After", "120")])
            })
            .await;

        let error = plugin.check_permissions().await.unwrap_err();
        server.abort();

        let expected = match (status, &error) {
            (401, PluginError::AuthenticationFailed(_))
            | (503, PluginError::ServiceUnavailable { .. }) => true,
            (
                429,
                PluginError::RateLimited {
                    retry_after: Some(hint),
                    ..
                },
            ) => *hint == Duration::from_secs(120),
            _ => false,
        };
        assert!(expected, "{status}: {error}");
        assert!(posts(&recorded).await.is_empty());
    }
}

#[tokio::test]
async fn workflow_parameters_explain_missing_contents_permission() {
    let (plugin, _recorded, server) = serve(&[], |method, path| match (method, path) {
        ("GET", "/api/v3/repos/acme/app/actions/workflows/42") => ok(workflow(42)),
        ("GET", "/api/v3/repos/acme/app/contents/.github/workflows/ci.yml") => {
            failure(403, "Resource not accessible by personal access token", &[])
        }
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await;

    let error = plugin
        .fetch_workflow_parameters(PIPELINE_ID)
        .await
        .unwrap_err();
    server.abort();

    assert!(matches!(error, PluginError::PermissionDenied(_)), "{error}");
    assert!(error.to_string().contains("Contents (Read)"), "{error}");
}

#[tokio::test]
async fn repositories_without_workflows_are_dropped_from_a_selection() {
    let (plugin, _recorded, server) = serve(&[], |method, path| match (method, path) {
        ("GET", "/api/v3/repos/acme/app/actions/workflows") => {
            ok(serde_json::json!({"total_count": 1, "workflows": [workflow(42)]}))
        }
        ("GET", "/api/v3/repos/acme/empty/actions/workflows") => {
            ok(serde_json::json!({"total_count": 0, "workflows": []}))
        }
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await;

    let kept = plugin
        .ids_with_ci(["acme/app", "acme/empty"].map(String::from).to_vec())
        .await
        .unwrap();
    server.abort();

    assert_eq!(kept, ["acme/app"]);
}

#[tokio::test]
async fn workflow_parameters_explain_a_workflow_file_missing_from_the_default_branch() {
    let (plugin, _recorded, server) = serve(&[], |method, path| match (method, path) {
        ("GET", "/api/v3/repos/acme/app/actions/workflows/42") => ok(workflow(42)),
        ("GET", "/api/v3/repos/acme/app/contents/.github/workflows/ci.yml") => {
            failure(404, "Not Found", &[])
        }
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await;

    let error = plugin
        .fetch_workflow_parameters(PIPELINE_ID)
        .await
        .unwrap_err();
    server.abort();

    assert!(matches!(error, PluginError::PipelineNotFound(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains(".github/workflows/ci.yml isn't on the default branch"),
        "{error}"
    );
}

#[tokio::test]
async fn permission_check_uses_the_configured_api_url() {
    let (plugin, recorded, server) = serve(&[], |method, path| match (method, path) {
        ("GET", "/api/v3/user") => Reply {
            status: 200,
            headers: vec![("X-OAuth-Scopes", "repo, workflow, read:org")],
            body: serde_json::json!({}),
        },
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await;

    let status = plugin.check_permissions().await.unwrap();

    assert_eq!(
        status.metadata.get("token_type").map(String::as_str),
        Some("classic_pat")
    );
    assert!(status.all_granted);
    assert_eq!(recorded.lock().await.len(), 1);
    server.abort();
}

#[tokio::test]
async fn workflow_parameters_list_the_workflow_dispatch_inputs_after_ref() {
    let source = "on:\n  push:\n  workflow_dispatch:\n    inputs:\n      environment:\n        description: Target\n        required: true\n        type: choice\n        options: [staging, production]\n      ref:\n        default: v1\n";
    let (plugin, recorded, server) = serve(&[], move |method, path| match (method, path) {
        ("GET", "/api/v3/repos/acme/app/actions/workflows/42") => ok(workflow(42)),
        ("GET", "/api/v3/repos/acme/app/contents/.github/workflows/ci.yml") => {
            ok(serde_json::json!(source))
        }
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await;

    let params = plugin.fetch_workflow_parameters(PIPELINE_ID).await.unwrap();
    let summary: Vec<_> = params
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.required,
                serde_json::to_value(&p.param_type).unwrap(),
            )
        })
        .collect();

    assert_eq!(
        summary,
        [
            (
                "ref",
                true,
                serde_json::json!({"type": "string", "default": "main"})
            ),
            (
                "inputs.environment",
                true,
                serde_json::json!({"type": "choice", "options": ["staging", "production"], "default": null})
            ),
            (
                "inputs.ref",
                false,
                serde_json::json!({"type": "string", "default": "v1"})
            ),
        ]
    );
    assert_eq!(params[1].label.as_deref(), Some("environment"));
    assert_eq!(recorded.lock().await.len(), 2);
    server.abort();
}

fn config_with(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    let mut config = HashMap::from([("token".to_string(), "stub-token".to_string())]);
    config.extend(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    config
}

fn test_client() -> Option<Arc<reqwest::Client>> {
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .user_agent("pipedash")
        .build()
        .unwrap();
    Some(Arc::new(client))
}

#[tokio::test]
async fn status_source_is_recomputed_on_every_initialize() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut plugin = GitHubPlugin::new();
    assert!(plugin.status_source().is_none());

    plugin
        .initialize(
            1,
            config_with(&[("base_url", "https://github.com")]),
            test_client(),
        )
        .unwrap();
    assert_eq!(plugin.status_source().unwrap().name, "GitHub");

    plugin
        .initialize(
            1,
            config_with(&[("base_url", "https://github.enterprise.com")]),
            test_client(),
        )
        .unwrap();
    assert!(plugin.status_source().is_none());

    plugin
        .initialize(
            1,
            config_with(&[
                ("base_url", "https://github.enterprise.com"),
                ("status_page_url", "https://status.example.com"),
            ]),
            test_client(),
        )
        .unwrap();
    assert_eq!(plugin.status_source().unwrap().name, "status.example.com");
}

#[test]
fn invalid_status_page_url_fails_initialize() {
    let mut plugin = GitHubPlugin::new();

    let error = plugin
        .initialize(
            1,
            config_with(&[("status_page_url", "javascript:alert(1)")]),
            None,
        )
        .unwrap_err();

    assert!(matches!(error, PluginError::InvalidConfig(_)), "{error}");
}

#[tokio::test]
async fn github_requests_use_the_proxy_of_the_given_client() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", listener.local_addr().unwrap());
    let recorded: Recorded = Arc::default();
    let log = recorded.clone();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_request(&mut stream).await;
            log.lock().await.push(request);
            let body = r#"{"message":"Bad credentials"}"#;
            let response = format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .user_agent("pipedash")
        .proxy(reqwest::Proxy::all(&proxy).unwrap())
        .build()
        .unwrap();
    let mut plugin = GitHubPlugin::new();
    plugin
        .initialize(
            1,
            config_with(&[("base_url", "http://ghe.pipedash.test")]),
            Some(Arc::new(client)),
        )
        .unwrap();

    let validated = plugin.validate_credentials().await;
    let _ = plugin.check_permissions().await;

    assert!(
        matches!(validated, Err(PluginError::AuthenticationFailed(_))),
        "{validated:?}"
    );
    let targets: Vec<String> = recorded
        .lock()
        .await
        .iter()
        .map(|(method, target, _)| format!("{method} {target}"))
        .collect();
    assert_eq!(
        targets[..2],
        [
            "GET http://ghe.pipedash.test/api/v3/user",
            "GET http://ghe.pipedash.test/api/v3/user",
        ],
        "{targets:?}"
    );
}

fn job(
    id: u64, status: &str, conclusion: Option<&str>, steps: serde_json::Value,
) -> serde_json::Value {
    let url = "http://stub.invalid/acme/app";
    serde_json::json!({
        "id": id, "run_id": 9001, "workflow_name": "CI", "head_branch": "main",
        "run_url": url, "run_attempt": 1, "node_id": "n", "head_sha": "abc",
        "url": url, "html_url": format!("{url}/actions/runs/9001/job/{id}"),
        "status": status, "conclusion": conclusion,
        "created_at": "2026-10-20T10:00:00Z", "started_at": "2026-10-20T10:00:05Z",
        "completed_at": (status == "completed").then_some("2026-10-20T10:01:00Z"),
        "name": format!("job-{id}"), "steps": steps, "check_run_url": url, "labels": []
    })
}

fn step(number: u64, status: &str, conclusion: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "name": format!("step-{number}"), "status": status, "conclusion": conclusion,
        "number": number,
        "started_at": (status != "pending").then_some("2026-10-20T10:00:06Z"),
        "completed_at": (status == "completed").then_some("2026-10-20T10:00:09Z")
    })
}

fn run_ref(metadata: &HashMap<String, serde_json::Value>) -> RunRef<'_> {
    RunRef {
        pipeline_id: PIPELINE_ID,
        run_number: 7,
        run_id: "github-run-9001",
        metadata,
    }
}

const JOBS_PATH: &str = "/api/v3/repos/acme/app/actions/runs/9001/jobs";

#[tokio::test]
async fn log_nodes_map_jobs_and_steps() {
    let (plugin, _recorded, server) = serve(&[], |method, path| match (method, path) {
        ("GET", JOBS_PATH) => ok(serde_json::json!({
            "total_count": 3,
            "jobs": [
                job(1, "completed", Some("failure"), serde_json::json!([
                    step(1, "completed", Some("success")),
                    step(2, "completed", Some("failure")),
                ])),
                job(2, "in_progress", None, serde_json::json!([
                    step(1, "in_progress", None),
                    step(2, "pending", None),
                ])),
                job(3, "queued", None, serde_json::json!([])),
            ]
        })),
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await;

    let from_metadata = HashMap::from([("run_id".to_string(), serde_json::json!(9001))]);
    let nodes = plugin
        .fetch_log_nodes(run_ref(&from_metadata))
        .await
        .unwrap();
    let from_id = plugin
        .fetch_log_nodes(run_ref(&HashMap::new()))
        .await
        .unwrap();
    let missing = plugin
        .fetch_log_nodes(RunRef {
            run_id: "other-9001",
            ..run_ref(&HashMap::new())
        })
        .await
        .unwrap_err();
    server.abort();

    assert_eq!(from_id.len(), nodes.len());
    assert!(
        matches!(missing, PluginError::InvalidConfig(_)),
        "{missing}"
    );
    let shape: Vec<_> = nodes
        .iter()
        .map(|n| {
            (
                n.id.as_str(),
                n.parent_id.as_deref(),
                n.kind,
                n.status,
                n.log,
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (
                "1",
                None,
                LogNodeKind::Job,
                PipelineStatus::Failed,
                Some(LogAvailability::Ready)
            ),
            (
                "1:1",
                Some("1"),
                LogNodeKind::Step,
                PipelineStatus::Success,
                None
            ),
            (
                "1:2",
                Some("1"),
                LogNodeKind::Step,
                PipelineStatus::Failed,
                None
            ),
            (
                "2",
                None,
                LogNodeKind::Job,
                PipelineStatus::Running,
                Some(LogAvailability::Pending)
            ),
            (
                "2:1",
                Some("2"),
                LogNodeKind::Step,
                PipelineStatus::Running,
                None
            ),
            (
                "2:2",
                Some("2"),
                LogNodeKind::Step,
                PipelineStatus::Pending,
                None
            ),
            (
                "3",
                None,
                LogNodeKind::Job,
                PipelineStatus::Running,
                Some(LogAvailability::Pending)
            ),
        ]
    );
    assert_eq!(nodes[0].name, "job-1");
    assert_eq!(
        nodes[0].web_url.as_deref(),
        Some("http://stub.invalid/acme/app/actions/runs/9001/job/1")
    );
    assert!(nodes[0].started_at.is_some() && nodes[0].finished_at.is_some());
    assert!(nodes[3].started_at.is_some() && nodes[3].finished_at.is_none());
    assert!(nodes[6].started_at.is_none());
    assert!(nodes[5].started_at.is_none());
    assert!(nodes[1].started_at.is_some() && nodes[1].finished_at.is_some());
    assert_eq!(nodes[1].name, "step-1");
}

#[tokio::test]
async fn jobs_are_paginated_until_a_short_page() {
    let pages = std::sync::atomic::AtomicUsize::new(0);
    let (plugin, recorded, server) = serve(&[], move |method, path| match (method, path) {
        ("GET", JOBS_PATH) => {
            let page = pages.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let (first, count) = if page == 0 { (1, 100) } else { (101, 3) };
            let jobs: Vec<_> = (first..first + count)
                .map(|id| job(id, "completed", Some("success"), serde_json::json!([])))
                .collect();
            ok(serde_json::json!({"total_count": 500, "jobs": jobs}))
        }
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await;

    let metadata = HashMap::new();
    let nodes = plugin.fetch_log_nodes(run_ref(&metadata)).await.unwrap();
    let requests = recorded.lock().await.len();
    server.abort();

    assert_eq!(nodes.len(), 103);
    assert!(nodes.iter().all(|n| n.kind == LogNodeKind::Job));
    assert_eq!(requests, 2);
}

async fn read_job_log(plugin: &GitHubPlugin, job_id: &str) -> PluginResult<Vec<u8>> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut sink = LogSink::new(tx);
    let metadata = HashMap::new();
    let read = plugin
        .read_log(run_ref(&metadata), job_id, None, &mut sink)
        .await?;
    assert_eq!(read, LogRead::replace(None));
    drop(sink);
    let mut bytes = Vec::new();
    while let Some(chunk) = rx.recv().await {
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[tokio::test]
async fn job_log_follows_redirect_into_the_sink() {
    let (plugin, recorded, server) = serve(&[], |method, path| match (method, path) {
        ("GET", "/api/v3/repos/acme/app/actions/jobs/7/logs") => Reply {
            status: 302,
            headers: vec![("Location", "/blob/7.txt")],
            body: serde_json::Value::String(String::new()),
        },
        ("GET", "/blob/7.txt") => ok(serde_json::Value::String(
            "2026-10-20T10:00:05.0000000Z first\r\nsecond\n".to_string(),
        )),
        (method, path) => panic!("Unexpected stub request: {method} {path}"),
    })
    .await;

    let bytes = read_job_log(&plugin, "7").await.unwrap();
    let paths: Vec<String> = recorded
        .lock()
        .await
        .iter()
        .map(|(_, path, _)| path.clone())
        .collect();
    server.abort();

    assert_eq!(bytes, b"2026-10-20T10:00:05.0000000Z first\r\nsecond\n");
    assert_eq!(
        paths,
        ["/api/v3/repos/acme/app/actions/jobs/7/logs", "/blob/7.txt"]
    );
}

#[tokio::test]
async fn expired_job_log_is_pipeline_not_found() {
    for status in [404, 410] {
        let (plugin, recorded, server) = serve(&[], move |method, path| match (method, path) {
            ("GET", "/api/v3/repos/acme/app/actions/jobs/7/logs") => {
                failure(status, "Logs expired", &[])
            }
            (method, path) => panic!("Unexpected stub request: {method} {path}"),
        })
        .await;

        let error = read_job_log(&plugin, "7").await.unwrap_err();
        let requests = recorded.lock().await.len();
        let not_a_job = read_job_log(&plugin, "7:2").await.unwrap_err();
        server.abort();

        assert!(
            matches!(&error, PluginError::PipelineNotFound(m) if m.contains("job 7")),
            "{status}: {error}"
        );
        assert_eq!(requests, 1, "{status}");
        assert!(
            matches!(not_a_job, PluginError::InvalidConfig(_)),
            "{not_a_job}"
        );
    }
}

#[test]
fn pipeline_actions_should_block_dynamic_workflows_and_rerun_in_place() {
    let plugin = GitHubPlugin::new();
    let metadata = HashMap::new();
    let facts = |workflow_file| PipelineFacts {
        id: "o__r__1",
        status: PipelineStatus::Success,
        branch: None,
        workflow_file,
        resource_state: None,
        metadata: &metadata,
    };

    let dynamic = plugin.pipeline_actions(&facts(Some("dynamic/pages/pages-build-deployment")));
    assert!(!dynamic.trigger.allowed);
    assert_eq!(
        dynamic.trigger.reason.as_deref(),
        Some("GitHub starts this built-in workflow itself. It has no workflow file, so only its existing runs can be rerun.")
    );
    assert_eq!(dynamic.rerun_strategy, RerunStrategy::InPlace);

    let regular = plugin.pipeline_actions(&facts(Some(".github/workflows/ci.yml")));
    assert!(regular.trigger.allowed && regular.trigger.reason.is_none());
    assert!(plugin.pipeline_actions(&facts(None)).trigger.allowed);
}

#[test]
fn metadata_should_expose_token_prefixes_and_force_cancel_warning() {
    let metadata = GitHubPlugin::new().metadata().clone();
    assert_eq!(
        metadata.token_prefixes,
        ["ghp_", "gho_", "ghu_", "ghs_", "github_pat_"]
    );
    assert!(metadata
        .presentation
        .force_cancel_confirmation
        .contains("GitHub bypasses conditions such as always()"));
}
