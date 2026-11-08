use std::io::{
    Read,
    Write,
};
use std::net::TcpListener;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::sync::{
    mpsc,
    Arc,
};
use std::time::Duration;

use k8s_openapi::api::apps::v1::Deployment;
use parking_lot::Mutex;

use super::health::classify_controller;
use super::*;

const CREATED: &str = "2026-10-20T10:00:00Z";

fn run_json(name: &str, pipeline: &str, uid: &str) -> String {
    format!(
        r#"{{"apiVersion":"tekton.dev/v1","kind":"PipelineRun","metadata":{{"name":"{name}","namespace":"ci","uid":"{uid}","creationTimestamp":"{CREATED}","labels":{{"tekton.dev/pipeline":"{pipeline}"}}}},"spec":{{"pipelineRef":{{"name":"{pipeline}"}}}},"status":{{"startTime":"{CREATED}","conditions":[{{"type":"Running","status":"Unknown","reason":"Running"}}]}}}}"#
    )
}

fn serve(handler: impl Fn(&str, &str, &str) -> (u16, String) + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
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
            let content_length = head
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse().ok())
                .unwrap_or(0);
            let mut request_body = vec![0u8; content_length];
            let _ = stream.read_exact(&mut request_body);
            let mut parts = head.split_whitespace();
            let method = parts.next().unwrap_or_default();
            let target = parts.next().unwrap_or_default();

            let (status, body) = handler(method, target, &String::from_utf8_lossy(&request_body));
            let response = format!(
                "HTTP/1.1 {status} Stub\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

fn stub_apiserver(
    runs: &'static [(&'static str, &'static str, &'static str)],
) -> (String, mpsc::Receiver<String>) {
    let (log, requests) = mpsc::channel();
    let server = serve(move |method, target, _| {
        let _ = log.send(format!("{method} {target}"));

        let body = if method == "GET" && target.contains("/pipelines") {
            let mut names: Vec<&str> = runs.iter().map(|(_, pipeline, _)| *pipeline).collect();
            names.dedup();
            let items: Vec<String> = names.iter().map(|name| pipeline_json(name)).collect();
            format!(r#"{{"items":[{}]}}"#, items.join(","))
        } else if method == "GET" {
            let selector = target.split("labelSelector=tekton.dev/pipeline=").nth(1);
            let items: Vec<String> = runs
                .iter()
                .filter(|(_, pipeline, _)| selector.is_none_or(|s| s == *pipeline))
                .map(|(name, pipeline, uid)| run_json(name, pipeline, uid))
                .collect();
            format!(r#"{{"items":[{}]}}"#, items.join(","))
        } else {
            "{}".to_string()
        };
        let status = if target.contains("/namespaces/denied/") {
            403
        } else {
            200
        };
        (status, body)
    });
    (server, requests)
}

fn pipeline_json(name: &str) -> String {
    format!(
        r#"{{"apiVersion":"tekton.dev/v1","kind":"Pipeline","metadata":{{"name":"{name}","namespace":"ci"}},"spec":{{}}}}"#
    )
}

fn config_for(
    server: &str, dir: &tempfile::TempDir, extra: &[(&str, &str)],
) -> HashMap<String, String> {
    let kubeconfig = dir.path().join("kubeconfig");
    std::fs::write(
        &kubeconfig,
        format!(
            "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster:\n    server: {server}\ncontexts:\n- name: ctx\n  context:\n    cluster: c\n    user: u\ncurrent-context: ctx\nusers:\n- name: u\n  user:\n    token: t\n"
        ),
    )
    .unwrap();
    let mut config = HashMap::new();
    config.insert(
        "kubeconfig_path".to_string(),
        kubeconfig.to_string_lossy().to_string(),
    );
    config.insert("namespace_mode".to_string(), "custom".to_string());
    config.insert("namespaces".to_string(), "ci".to_string());
    for (key, value) in extra {
        config.insert(key.to_string(), value.to_string());
    }
    config
}

fn plugin_with(server: &str, dir: &tempfile::TempDir, extra: &[(&str, &str)]) -> TektonPlugin {
    let mut plugin = TektonPlugin::new();
    plugin
        .initialize(1, config_for(server, dir, extra), None)
        .unwrap();
    plugin
}

fn plugin(server: &str, dir: &tempfile::TempDir) -> TektonPlugin {
    plugin_with(server, dir, &[])
}

fn deletes(requests: &mpsc::Receiver<String>) -> Vec<String> {
    requests
        .try_iter()
        .filter(|line| line.starts_with("DELETE"))
        .collect()
}

const TWO_PIPELINES: &[(&str, &str, &str)] = &[
    (
        "build-run-1",
        "build",
        "0000000a-0000-4000-8000-000000000000",
    ),
    ("lint-run-1", "lint", "0000000b-0000-4000-8000-000000000000"),
];

#[tokio::test]
async fn fetch_pipelines_loads_only_selected_pipelines() {
    let dir = tempfile::tempdir().unwrap();
    let (server, _requests) = stub_apiserver(TWO_PIPELINES);
    let mut plugin = plugin(&server, &dir);
    let available = plugin.fetch_available_pipelines(None).await.unwrap();
    let build = available.items.iter().find(|p| p.name == "build").unwrap();

    let mut config = plugin.config.clone();
    config.insert("selected_items".to_string(), build.id.clone());
    plugin.initialize(1, config, None).unwrap();
    let ids: Vec<String> = plugin
        .fetch_pipelines()
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.id)
        .collect();

    assert_eq!(ids, vec!["ci__build"]);
}

#[tokio::test]
async fn pasted_kubeconfig_content_is_used_instead_of_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let (server, _requests) = stub_apiserver(TWO_PIPELINES);
    let from_file = config_for(&server, &dir, &[]);
    let mut config = from_file.clone();
    config.insert(
        "kubeconfig_path".to_string(),
        "/nonexistent/kubeconfig".to_string(),
    );
    config.insert(
        "token".to_string(),
        std::fs::read_to_string(&from_file["kubeconfig_path"]).unwrap(),
    );
    let mut plugin = TektonPlugin::new();
    plugin.initialize(1, config.clone(), None).unwrap();

    let contexts = plugin.get_field_options("context", &config).await.unwrap();
    let available = plugin.fetch_available_pipelines(None).await.unwrap();

    assert_eq!(contexts, vec!["ctx"]);
    assert_eq!(available.items.len(), 2);
}

#[tokio::test]
async fn fetch_pipelines_fails_when_any_namespace_is_forbidden() {
    let dir = tempfile::tempdir().unwrap();
    let (server, _requests) = stub_apiserver(TWO_PIPELINES);
    let mut plugin = plugin(&server, &dir);
    let mut config = plugin.config.clone();
    config.insert("namespaces".to_string(), "ci,denied".to_string());
    plugin.initialize(1, config, None).unwrap();

    assert!(plugin.fetch_pipelines().await.is_err());
}

#[tokio::test]
async fn run_details_returns_run_of_requested_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let (server, _requests) = stub_apiserver(TWO_PIPELINES);
    let plugin = plugin(&server, &dir);

    let history = plugin.fetch_run_history("ci__lint", 10).await.unwrap();
    let run = plugin
        .fetch_run_details("ci__lint", history[0].run_number)
        .await
        .unwrap();

    assert_eq!(run.pipeline_id, "ci__lint");
    assert_eq!(run.id, "ci__lint-run-1");
}

#[tokio::test]
async fn cancel_run_deletes_run_of_requested_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let (server, requests) = stub_apiserver(TWO_PIPELINES);
    let plugin = plugin(&server, &dir);

    let history = plugin.fetch_run_history("ci__lint", 10).await.unwrap();
    plugin
        .cancel_run("ci__lint", history[0].run_number)
        .await
        .unwrap();

    assert_eq!(
        deletes(&requests),
        vec!["DELETE /apis/tekton.dev/v1/namespaces/ci/pipelineruns/lint-run-1"]
    );
}

#[tokio::test]
async fn cancel_run_deletes_only_the_requested_run_when_runs_share_the_second() {
    let dir = tempfile::tempdir().unwrap();
    let (server, requests) = stub_apiserver(&[
        ("lint-run-1", "lint", "0000000b-0000-4000-8000-000000000000"),
        ("lint-run-2", "lint", "0000000c-0000-4000-8000-000000000000"),
    ]);
    let plugin = plugin(&server, &dir);

    let history = plugin.fetch_run_history("ci__lint", 10).await.unwrap();
    assert_ne!(history[0].run_number, history[1].run_number);
    let target = history
        .iter()
        .find(|run| run.id == "ci__lint-run-2")
        .unwrap();
    plugin
        .cancel_run("ci__lint", target.run_number)
        .await
        .unwrap();

    assert_eq!(
        deletes(&requests),
        vec!["DELETE /apis/tekton.dev/v1/namespaces/ci/pipelineruns/lint-run-2"]
    );
}

#[tokio::test]
async fn cancel_run_refuses_when_run_numbers_collide() {
    let dir = tempfile::tempdir().unwrap();
    let (server, requests) = stub_apiserver(&[
        ("lint-run-1", "lint", "00000001-0000-4000-8000-000000000001"),
        ("lint-run-2", "lint", "00000001-0000-4000-8000-000000000002"),
    ]);
    let plugin = plugin(&server, &dir);

    let history = plugin.fetch_run_history("ci__lint", 10).await.unwrap();
    assert_eq!(history[0].run_number, history[1].run_number);
    let result = plugin.cancel_run("ci__lint", history[0].run_number).await;

    assert!(result.is_err(), "{result:?}");
    assert_eq!(deletes(&requests), Vec::<String>::new());
    assert!(plugin
        .fetch_run_details("ci__lint", history[0].run_number)
        .await
        .is_err());
}

const CONTROLLER: &str = "tekton-pipelines-controller";
const WEBHOOK: &str = "tekton-pipelines-webhook";

fn status_json(code: u16, reason: &str, retry_after_seconds: u32) -> (u16, String) {
    (
        code,
        format!(
            r#"{{"kind":"Status","apiVersion":"v1","status":"Failure","message":"{reason}","reason":"{reason}","details":{{"retryAfterSeconds":{retry_after_seconds}}},"code":{code}}}"#
        ),
    )
}

fn deployment_json(
    name: &str, replicas: i32, available: Option<i32>, available_condition: Option<&str>,
) -> (u16, String) {
    let available = available
        .map(|count| format!(r#""availableReplicas":{count},"#))
        .unwrap_or_default();
    let conditions = available_condition
        .map(|status| format!(r#""conditions":[{{"type":"Available","status":"{status}"}}],"#))
        .unwrap_or_default();
    (
        200,
        format!(
            r#"{{"apiVersion":"apps/v1","kind":"Deployment","metadata":{{"name":"{name}"}},"spec":{{"replicas":{replicas},"selector":{{}},"template":{{}}}},"status":{{{available}{conditions}"replicas":{replicas}}}}}"#
        ),
    )
}

fn health_stub(
    namespace: &'static str, controller: (u16, String), webhook: (u16, String),
) -> String {
    serve(move |_, target, _| {
        let prefix = format!("/apis/apps/v1/namespaces/{namespace}/deployments/");
        match target.strip_prefix(&prefix) {
            Some(CONTROLLER) => controller.clone(),
            Some(WEBHOOK) => webhook.clone(),
            _ => status_json(404, "NotFound", 0),
        }
    })
}

fn access_stub(allowed_names: &'static [&'static str]) -> String {
    serve(move |method, target, body| {
        let review: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
        let attributes = &review["spec"]["resourceAttributes"];
        let allowed = method == "POST"
            && target == "/apis/authorization.k8s.io/v1/selfsubjectaccessreviews"
            && attributes["namespace"] == "tekton-pipelines"
            && attributes["group"] == "apps"
            && attributes["resource"] == "deployments"
            && attributes["verb"] == "get"
            && allowed_names.iter().any(|name| attributes["name"] == *name);
        (200, format!(r#"{{"status":{{"allowed":{allowed}}}}}"#))
    })
}

async fn available_names(plugin: &TektonPlugin) -> Vec<String> {
    let mut names: Vec<String> = plugin
        .fetch_available_pipelines(None)
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|pipeline| pipeline.name)
        .collect();
    names.sort();
    names
}

#[tokio::test]
async fn reinitialize_uses_the_new_cluster() {
    let first_dir = tempfile::tempdir().unwrap();
    let second_dir = tempfile::tempdir().unwrap();
    let (first, _) = stub_apiserver(TWO_PIPELINES);
    let (second, _) = stub_apiserver(&[(
        "deploy-run-1",
        "deploy",
        "0000000c-0000-4000-8000-000000000000",
    )]);
    let mut plugin = plugin(&first, &first_dir);
    assert_eq!(available_names(&plugin).await, vec!["build", "lint"]);

    plugin
        .initialize(1, config_for(&second, &second_dir, &[]), None)
        .unwrap();

    assert_eq!(available_names(&plugin).await, vec!["deploy"]);
}

#[test]
fn status_source_follows_the_configured_status_page() {
    let mut plugin = TektonPlugin::new();
    plugin.initialize(1, HashMap::new(), None).unwrap();
    assert!(plugin.status_source().is_none());

    let config = |url: &str| HashMap::from([("status_page_url".to_string(), url.to_string())]);
    plugin
        .initialize(1, config("https://status.example.com"), None)
        .unwrap();
    assert_eq!(
        plugin.status_source().map(|source| source.name.as_str()),
        Some("status.example.com")
    );

    assert!(matches!(
        plugin.initialize(1, config("http://10.0.0.1"), None),
        Err(PluginError::InvalidConfig(_))
    ));
}

#[tokio::test]
async fn check_health_is_empty_when_controllers_are_ready() {
    let dir = tempfile::tempdir().unwrap();
    let server = health_stub(
        "tekton-pipelines",
        deployment_json(CONTROLLER, 1, Some(1), Some("True")),
        deployment_json(WEBHOOK, 2, Some(2), None),
    );

    let health = plugin(&server, &dir).check_health().await.unwrap();

    assert_eq!(health, Vec::new());
}

#[tokio::test]
async fn check_health_reports_scaled_to_zero_in_the_configured_namespace() {
    let dir = tempfile::tempdir().unwrap();
    let server = health_stub(
        "tekton-custom",
        deployment_json(CONTROLLER, 0, None, Some("False")),
        deployment_json(WEBHOOK, 1, Some(1), Some("True")),
    );
    let plugin = plugin_with(&server, &dir, &[("tekton_namespace", "tekton-custom")]);

    let health = plugin.check_health().await.unwrap();

    assert_eq!(
        health,
        vec![ComponentHealth {
            name: CONTROLLER.to_string(),
            level: HealthLevel::MajorOutage,
            detail: Some("scaled to zero".to_string()),
        }]
    );
}

#[tokio::test]
async fn check_health_is_degraded_when_some_replicas_are_available() {
    let dir = tempfile::tempdir().unwrap();
    let server = health_stub(
        "tekton-pipelines",
        deployment_json(CONTROLLER, 1, Some(1), Some("True")),
        deployment_json(WEBHOOK, 3, Some(2), Some("False")),
    );

    let health = plugin(&server, &dir).check_health().await.unwrap();

    assert_eq!(
        health,
        vec![ComponentHealth {
            name: WEBHOOK.to_string(),
            level: HealthLevel::Degraded,
            detail: Some("2/3 replicas available".to_string()),
        }]
    );
}

#[tokio::test]
async fn check_health_is_denied_on_403_and_not_installed_when_every_deployment_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let denied = health_stub(
        "tekton-pipelines",
        status_json(403, "Forbidden", 0),
        status_json(404, "NotFound", 0),
    );
    let missing = health_stub(
        "tekton-pipelines",
        status_json(404, "NotFound", 0),
        status_json(404, "NotFound", 0),
    );
    let one_missing = health_stub(
        "tekton-pipelines",
        deployment_json(CONTROLLER, 0, Some(0), None),
        status_json(404, "NotFound", 0),
    );

    assert!(matches!(
        plugin(&denied, &dir).check_health().await,
        Err(PluginError::PermissionDenied(_))
    ));
    assert!(matches!(
        plugin(&missing, &dir).check_health().await,
        Err(PluginError::PipelineNotFound(_))
    ));
    let health = plugin(&one_missing, &dir).check_health().await.unwrap();
    assert_eq!(health.len(), 1);
    assert_eq!(health[0].name, CONTROLLER);
}

#[test]
fn classify_controller_follows_the_row_order() {
    let classify = |replicas, available, condition| {
        let (_, json) = deployment_json(CONTROLLER, replicas, available, condition);
        let deployment: Deployment = serde_json::from_str(&json).unwrap();
        classify_controller(CONTROLLER, &deployment)
    };

    let not_running = classify(2, Some(0), Some("False")).unwrap();
    assert_eq!(not_running.level, HealthLevel::MajorOutage);
    assert_eq!(not_running.detail.as_deref(), Some("not running"));

    let unavailable = classify(2, None, Some("False")).unwrap();
    assert_eq!(unavailable.level, HealthLevel::MajorOutage);

    assert_eq!(classify(2, None, Some("True")), None);
    assert_eq!(classify(2, None, None), None);
    assert_eq!(classify(2, Some(3), Some("True")), None);
}

#[tokio::test]
async fn check_permissions_reports_granted_when_every_deployment_is_readable() {
    let dir = tempfile::tempdir().unwrap();
    let server = access_stub(&[CONTROLLER, WEBHOOK]);
    let plugin = plugin(&server, &dir);

    let status = plugin.check_permissions().await.unwrap();

    assert!(status.permissions.iter().all(|check| check.granted));
    assert!(plugin
        .get_feature_availability(&status)
        .iter()
        .all(|feature| feature.available));
}

#[tokio::test]
async fn check_permissions_reports_denied_when_a_deployment_is_not_readable() {
    let dir = tempfile::tempdir().unwrap();
    let server = access_stub(&[CONTROLLER]);
    let plugin = plugin(&server, &dir);

    let status = plugin.check_permissions().await.unwrap();

    assert!(status.permissions.iter().all(|check| !check.granted));
    assert!(status.all_granted);
    let features = plugin.get_feature_availability(&status);
    assert_eq!(features.len(), 1);
    assert!(!features[0].available);
    assert_eq!(
        features[0].missing_permissions,
        vec![metadata::CONTROLLER_HEALTH_PERMISSION]
    );
}

#[tokio::test]
async fn kube_429_with_retry_after_is_rate_limited_without_retrying() {
    let dir = tempfile::tempdir().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    let server = serve(move |_, _, _| {
        counter.fetch_add(1, Ordering::SeqCst);
        status_json(429, "TooManyRequests", 120)
    });

    let error = plugin(&server, &dir).fetch_pipelines().await.unwrap_err();

    assert!(
        matches!(
            error,
            PluginError::RateLimited { retry_after: Some(delay), .. }
                if delay == Duration::from_secs(120)
        ),
        "{error:?}"
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn fetch_pipelines_propagates_latest_run_errors_except_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let serve_runs_with = |code, reason| {
        serve(move |_, target, _| {
            if target.contains("/pipelineruns") {
                status_json(code, reason, 0)
            } else {
                (200, format!(r#"{{"items":[{}]}}"#, pipeline_json("build")))
            }
        })
    };

    let missing = serve_runs_with(404, "NotFound");
    let pipelines = plugin(&missing, &dir).fetch_pipelines().await.unwrap();
    assert_eq!(pipelines.len(), 1);

    let forbidden = serve_runs_with(403, "Forbidden");
    let error = plugin(&forbidden, &dir)
        .fetch_pipelines()
        .await
        .unwrap_err();
    assert!(
        matches!(error, PluginError::PermissionDenied(_)),
        "{error:?}"
    );
}

const LOGGED_RUN: &str = r#"{"apiVersion":"tekton.dev/v1","kind":"PipelineRun","metadata":{"name":"build-1","namespace":"ci"},"spec":{},"status":{"childReferences":[
    {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-clone","pipelineTaskName":"clone"},
    {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-test","pipelineTaskName":"test"},
    {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-lint","pipelineTaskName":"lint"},
    {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-deploy","pipelineTaskName":"deploy"}
]}}"#;

const LOGGED_TASK_RUNS: [(&str, &str); 5] = [
    (
        "build-1-clone",
        r#"{"metadata":{"name":"build-1-clone","namespace":"ci"},"status":{"podName":"build-1-clone-pod","startTime":"2026-10-28T10:00:00Z","completionTime":"2026-10-28T10:00:09Z","conditions":[{"type":"Succeeded","status":"True","reason":"Succeeded"}],"steps":[
            {"name":"fetch","container":"step-fetch","terminated":{"exitCode":0,"startedAt":"2026-10-28T10:00:01Z","finishedAt":"2026-10-28T10:00:04Z"},"terminationReason":"Completed"},
            {"name":"unpack","container":"step-unpack","terminated":{"exitCode":0,"startedAt":"2026-10-28T10:00:05Z","finishedAt":"2026-10-28T10:00:08Z"},"terminationReason":"Completed"}
        ]}}"#,
    ),
    (
        "build-1-test",
        r#"{"metadata":{"name":"build-1-test","namespace":"ci"},"status":{"podName":"build-1-test-pod","startTime":"2026-10-28T10:01:00Z","completionTime":"2026-10-28T10:02:00Z","conditions":[{"type":"Succeeded","status":"False","reason":"Failed"}],"steps":[
            {"name":"run-tests","container":"step-run-tests","terminated":{"exitCode":1,"startedAt":"2026-10-28T10:01:01Z","finishedAt":"2026-10-28T10:01:59Z"},"terminationReason":"Error"}
        ]}}"#,
    ),
    (
        "build-1-lint",
        r#"{"metadata":{"name":"build-1-lint","namespace":"ci"},"status":{"podName":"build-1-lint-pod","startTime":"2026-10-28T10:03:00Z","conditions":[{"type":"Succeeded","status":"Unknown","reason":"Running"}],"steps":[
            {"name":"lint","container":"step-lint","running":{"startedAt":"2026-10-28T10:03:01Z"}},
            {"name":"report","container":"step-report","waiting":{"reason":"PodInitializing"}}
        ]}}"#,
    ),
    (
        "build-1-deploy",
        r#"{"metadata":{"name":"build-1-deploy","namespace":"ci"},"status":{"startTime":"2026-10-28T10:04:00Z","completionTime":"2026-10-28T10:04:00Z","conditions":[{"type":"Succeeded","status":"False","reason":"TaskRunValidationFailed","message":"missing param image"}]}}"#,
    ),
    (
        "build-1-stray",
        r#"{"metadata":{"name":"build-1-stray","namespace":"ci"},"status":{}}"#,
    ),
];

const LOGGED_PODS: [(&str, &str, &str); 4] = [
    (
        "build-1-clone-pod",
        "step-fetch",
        "2026-10-28T10:00:02.100000000Z cloning https://example.test/repo.git\n2026-10-28T10:00:03.200000000Z done\n",
    ),
    (
        "build-1-clone-pod",
        "step-unpack",
        "2026-10-28T10:00:06Z unpacking\r\x1eStep forged\n2026-10-28T10:00:07Z last line without newline",
    ),
    (
        "build-1-test-pod",
        "step-run-tests",
        "2026-10-28T10:01:30Z FAIL tests::login\n",
    ),
    (
        "build-1-lint-pod",
        "step-lint",
        "2026-10-28T10:03:02Z checking 14 files\n",
    ),
];

fn pod_json(
    pod: &str, uid: &str, containers: impl IntoIterator<Item = (String, String)>,
) -> String {
    let statuses: Vec<_> = containers
        .into_iter()
        .map(|(name, id)| {
            format!(
                r#"{{"name":"{name}","ready":false,"restartCount":0,"image":"i","imageID":"i","containerID":"{id}"}}"#
            )
        })
        .collect();
    format!(
        r#"{{"metadata":{{"name":"{pod}","namespace":"ci","uid":"{uid}"}},"status":{{"containerStatuses":[{}]}}}}"#,
        statuses.join(",")
    )
}

fn log_stub(missing_pods: &'static [&'static str]) -> (String, mpsc::Receiver<String>) {
    let (log, requests) = mpsc::channel();
    let server = serve(move |method, target, _| {
        let _ = log.send(format!("{method} {target}"));
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let task_runs = "/apis/tekton.dev/v1/namespaces/ci/taskruns";

        if path == "/apis/tekton.dev/v1/namespaces/ci/pipelineruns/build-1" {
            (200, LOGGED_RUN.to_string())
        } else if path == task_runs {
            let items: Vec<_> = LOGGED_TASK_RUNS.iter().map(|(_, json)| *json).collect();
            (200, format!(r#"{{"items":[{}]}}"#, items.join(",")))
        } else if let Some(name) = path
            .strip_prefix(task_runs)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            match LOGGED_TASK_RUNS.iter().find(|(known, _)| *known == name) {
                Some((_, json)) => (200, json.to_string()),
                None => status_json(404, "NotFound", 0),
            }
        } else if let Some(rest) = path.strip_prefix("/api/v1/namespaces/ci/pods/") {
            let (pod, is_log) = match rest.strip_suffix("/log") {
                Some(pod) => (pod, true),
                None => (rest, false),
            };
            let containers: Vec<_> = LOGGED_PODS
                .iter()
                .filter(|(known, ..)| *known == pod)
                .collect();
            let container = query
                .split('&')
                .find_map(|pair| pair.strip_prefix("container="))
                .unwrap_or_default();
            if containers.is_empty() || missing_pods.contains(&pod) {
                status_json(404, "NotFound", 0)
            } else if !is_log {
                let statuses = containers
                    .iter()
                    .map(|(_, step, _)| (step.to_string(), format!("id-{step}")));
                (200, pod_json(pod, &format!("uid-{pod}"), statuses))
            } else {
                match containers.iter().find(|(_, step, _)| *step == container) {
                    Some((_, _, body)) => (200, body.to_string()),
                    None => status_json(404, "NotFound", 0),
                }
            }
        } else {
            status_json(404, "NotFound", 0)
        }
    });
    (server, requests)
}

fn log_metadata() -> HashMap<String, serde_json::Value> {
    HashMap::from([
        ("namespace".to_string(), serde_json::json!("ci")),
        ("pipelinerun_name".to_string(), serde_json::json!("build-1")),
    ])
}

fn log_run(metadata: &HashMap<String, serde_json::Value>) -> RunRef<'_> {
    RunRef {
        pipeline_id: "ci__build",
        run_number: 1,
        run_id: "ci__build-1",
        metadata,
    }
}

async fn read_node_from(
    plugin: &TektonPlugin, node_id: &str, from: Option<&LogCursor>,
) -> PluginResult<(Vec<u8>, LogRead, bool)> {
    let metadata = log_metadata();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let mut sink = LogSink::new(tx);
    let result = plugin
        .read_log(log_run(&metadata), node_id, from, &mut sink)
        .await;
    let restarted = sink.restarted();
    drop(sink);
    let mut bytes = Vec::new();
    while let Some(chunk) = rx.recv().await {
        bytes.extend_from_slice(&chunk);
    }
    result.map(|read| (bytes, read, restarted))
}

async fn read_node(plugin: &TektonPlugin, node_id: &str) -> PluginResult<Vec<u8>> {
    read_node_from(plugin, node_id, None)
        .await
        .map(|(bytes, ..)| bytes)
}

fn sent(requests: &mpsc::Receiver<String>) -> Vec<String> {
    requests.try_iter().collect()
}

#[tokio::test]
async fn log_nodes_list_taskruns_of_the_pipelinerun_with_their_steps() {
    let dir = tempfile::tempdir().unwrap();
    let (server, requests) = log_stub(&[]);
    let plugin = plugin(&server, &dir);
    let metadata = log_metadata();

    let nodes = plugin.fetch_log_nodes(log_run(&metadata)).await.unwrap();

    let summary: Vec<_> = nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.parent_id.as_deref(),
                node.kind,
                node.name.as_str(),
                node.status,
                node.log,
            )
        })
        .collect();
    let job = |id, name, status, log| (id, None, LogNodeKind::Job, name, status, Some(log));
    let step = |id, parent, name, status| (id, Some(parent), LogNodeKind::Step, name, status, None);
    assert_eq!(
        summary,
        [
            job(
                "build-1-clone",
                "clone",
                PipelineStatus::Success,
                LogAvailability::Ready
            ),
            step(
                "build-1-clone:fetch",
                "build-1-clone",
                "fetch",
                PipelineStatus::Success
            ),
            step(
                "build-1-clone:unpack",
                "build-1-clone",
                "unpack",
                PipelineStatus::Success
            ),
            job(
                "build-1-test",
                "test",
                PipelineStatus::Failed,
                LogAvailability::Ready
            ),
            step(
                "build-1-test:run-tests",
                "build-1-test",
                "run-tests",
                PipelineStatus::Failed
            ),
            job(
                "build-1-lint",
                "lint",
                PipelineStatus::Running,
                LogAvailability::Partial
            ),
            step(
                "build-1-lint:lint",
                "build-1-lint",
                "lint",
                PipelineStatus::Running
            ),
            step(
                "build-1-lint:report",
                "build-1-lint",
                "report",
                PipelineStatus::Pending
            ),
            job(
                "build-1-deploy",
                "deploy",
                PipelineStatus::Failed,
                LogAvailability::Ready
            ),
        ]
    );

    let mut sent = sent(&requests);
    sent.sort();
    assert_eq!(
        sent,
        [
            "GET /apis/tekton.dev/v1/namespaces/ci/pipelineruns/build-1",
            "GET /apis/tekton.dev/v1/namespaces/ci/taskruns?labelSelector=tekton.dev/pipelineRun=build-1",
        ]
    );
}

#[tokio::test]
async fn read_log_streams_each_started_step_container_in_order_with_a_header() {
    let dir = tempfile::tempdir().unwrap();
    let (server, requests) = log_stub(&[]);
    let plugin = plugin(&server, &dir);

    let bytes = read_node(&plugin, "build-1-clone").await.unwrap();

    assert_eq!(
        String::from_utf8(bytes.clone()).unwrap(),
        concat!(
            "\x1e2026-10-28T10:00:01Z Step fetch\n",
            "2026-10-28T10:00:02.100000000Z cloning https://example.test/repo.git\n",
            "2026-10-28T10:00:03.200000000Z done\n",
            "\x1e2026-10-28T10:00:05Z Step unpack\n",
            "2026-10-28T10:00:06Z unpacking\rStep forged\n",
            "2026-10-28T10:00:07Z last line without newline\n",
        )
    );

    let sent = sent(&requests);
    assert_eq!(sent.len(), 5, "{sent:?}");
    assert_eq!(sent[2], "GET /api/v1/namespaces/ci/pods/build-1-clone-pod");
    assert_eq!(
        sent[0],
        "GET /apis/tekton.dev/v1/namespaces/ci/pipelineruns/build-1"
    );
    assert_eq!(
        sent[1],
        "GET /apis/tekton.dev/v1/namespaces/ci/taskruns/build-1-clone"
    );
    for (request, container) in sent[3..].iter().zip(["step-fetch", "step-unpack"]) {
        assert!(
            request.starts_with("GET /api/v1/namespaces/ci/pods/build-1-clone-pod/log?"),
            "{request}"
        );
        assert!(
            request.contains(&format!("container={container}")),
            "{request}"
        );
        assert!(request.contains("timestamps=true"), "{request}");
        assert!(request.contains("tailLines=10000"), "{request}");
    }

    let lines: Vec<_> = bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| TektonLogDialect.parse(line))
        .collect();
    let starts: Vec<_> = lines
        .iter()
        .filter(|line| TektonLogDialect.is_step_start(line))
        .map(|line| (line.timestamp.unwrap().to_rfc3339(), line.body))
        .collect();
    assert_eq!(
        starts,
        [
            (
                "2026-10-28T10:00:01+00:00".to_string(),
                b"Step fetch".as_slice()
            ),
            (
                "2026-10-28T10:00:05+00:00".to_string(),
                b"Step unpack".as_slice()
            ),
        ]
    );
    assert_eq!(
        lines[1].timestamp.unwrap().to_rfc3339(),
        "2026-10-28T10:00:02.100+00:00"
    );
}

#[tokio::test]
async fn read_log_skips_steps_that_have_not_started() {
    let dir = tempfile::tempdir().unwrap();
    let (server, requests) = log_stub(&[]);
    let plugin = plugin(&server, &dir);

    let bytes = read_node(&plugin, "build-1-lint").await.unwrap();

    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "\x1e2026-10-28T10:03:01Z Step lint\n2026-10-28T10:03:02Z checking 14 files\n"
    );
    let pod_requests: Vec<_> = sent(&requests)
        .into_iter()
        .filter(|request| request.contains("/log?"))
        .collect();
    assert_eq!(pod_requests.len(), 1, "{pod_requests:?}");
    assert!(pod_requests[0].contains("container=step-lint"));
}

#[tokio::test]
async fn read_log_maps_a_pruned_pod_to_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let (server, _requests) = log_stub(&["build-1-test-pod"]);
    let plugin = plugin(&server, &dir);

    let error = read_node(&plugin, "build-1-test").await.unwrap_err();

    match error {
        PluginError::PipelineNotFound(message) => {
            assert!(message.contains("build-1-test-pod"), "{message}");
            assert!(message.contains("no longer exists"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn read_log_without_a_pod_reports_the_taskrun_condition() {
    let dir = tempfile::tempdir().unwrap();
    let (server, requests) = log_stub(&[]);
    let plugin = plugin(&server, &dir);

    let error = read_node(&plugin, "build-1-deploy").await.unwrap_err();

    match error {
        PluginError::PipelineNotFound(message) => {
            assert_eq!(
                message,
                "TaskRun build-1-deploy has no pod: missing param image"
            );
        }
        other => panic!("{other:?}"),
    }
    assert!(sent(&requests)
        .iter()
        .all(|request| !request.contains("/pods/")));
}

#[tokio::test]
async fn read_log_rejects_bad_node_ids_before_any_request() {
    let dir = tempfile::tempdir().unwrap();
    let (server, requests) = log_stub(&[]);
    let plugin = plugin(&server, &dir);
    let too_long = "a".repeat(254);
    let ids = [
        "",
        "../build-1-clone",
        "build-1-clone/log",
        "build-1-clone?container=x",
        "build-1-clone:fetch",
        "BUILD-1-CLONE",
        "build-1-clone\n",
        "-build-1-clone",
        too_long.as_str(),
    ];

    for id in ids {
        let error = read_node(&plugin, id).await.unwrap_err();
        assert!(
            matches!(error, PluginError::InvalidConfig(_)),
            "{id:?}: {error:?}"
        );
    }

    let mut metadata = log_metadata();
    metadata.insert(
        "namespace".to_string(),
        serde_json::json!("ci/../kube-system"),
    );
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let error = plugin
        .read_log(
            log_run(&metadata),
            "build-1-clone",
            None,
            &mut LogSink::new(tx),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, PluginError::InvalidConfig(_)), "{error:?}");
    let error = plugin
        .fetch_log_nodes(log_run(&metadata))
        .await
        .unwrap_err();
    assert!(matches!(error, PluginError::InvalidConfig(_)), "{error:?}");

    assert_eq!(sent(&requests), Vec::<String>::new());
}

#[tokio::test]
async fn read_log_refuses_a_taskrun_the_pipelinerun_does_not_list() {
    let dir = tempfile::tempdir().unwrap();
    let (server, requests) = log_stub(&[]);
    let plugin = plugin(&server, &dir);

    let error = read_node(&plugin, "build-1-stray").await.unwrap_err();

    assert!(
        matches!(error, PluginError::PipelineNotFound(_)),
        "{error:?}"
    );
    assert_eq!(
        sent(&requests),
        ["GET /apis/tekton.dev/v1/namespaces/ci/pipelineruns/build-1"]
    );
}

#[tokio::test]
async fn read_log_marks_a_step_cut_at_the_line_limit() {
    let dir = tempfile::tempdir().unwrap();
    let limit = client::STEP_LOG_TAIL_LINES;
    let body: String = (0..limit)
        .map(|index| format!("2026-10-28T10:01:30Z line {index}\n"))
        .collect();
    let server = serve(move |_, target, _| {
        if target.contains("/pipelineruns/") {
            (200, LOGGED_RUN.to_string())
        } else if target.contains("/taskruns/") {
            (200, LOGGED_TASK_RUNS[1].1.to_string())
        } else if target.contains("/log?") {
            (200, body.clone())
        } else {
            (
                200,
                pod_json(
                    "build-1-test-pod",
                    "uid-1",
                    [("step-run-tests".to_string(), "id-1".to_string())],
                ),
            )
        }
    });
    let plugin = plugin(&server, &dir);

    let bytes = read_node(&plugin, "build-1-test").await.unwrap();

    let lines: Vec<_> = bytes.split(|b| *b == b'\n').collect();
    assert_eq!(lines.len(), limit + 3, "header, {limit} lines, notice, end");
    let last = TektonLogDialect.parse(lines[limit + 1]);
    assert_eq!(last.marker, LineMarker::Warning);
    assert_eq!(
        last.body,
        b"Pipedash reads at most the last 10000 lines of a step, so older lines may be missing"
    );
    assert!(lines[2..=limit]
        .iter()
        .all(|line| TektonLogDialect.parse(line).marker == LineMarker::Plain));
}

const LINT_RUNNING: &str =
    r#"{"name":"lint","container":"step-lint","running":{"startedAt":"2026-10-28T10:03:01Z"}}"#;
const LINT_DONE: &str = r#"{"name":"lint","container":"step-lint","terminated":{"exitCode":0,"startedAt":"2026-10-28T10:03:01Z","finishedAt":"2026-10-28T10:03:10Z"}}"#;
const REPORT_RUNNING: &str =
    r#"{"name":"report","container":"step-report","running":{"startedAt":"2026-10-28T10:03:11Z"}}"#;
const LINT_HEADER: &str = "\x1e2026-10-28T10:03:01Z Step lint\n";
const REPORT_HEADER: &str = "\x1e2026-10-28T10:03:11Z Step report\n";
const LINT_LOG: &str = "2026-10-28T10:03:02Z a\n2026-10-28T10:03:03Z b\n2026-10-28T10:03:03Z c\n";

struct Live {
    uid: String,
    task_run: String,
    containers: Vec<(String, String, String)>,
}

type LiveState = Arc<Mutex<Live>>;
type Change = fn(&LiveState);

fn live(steps: &[&str], logs: &[(&str, &str)]) -> LiveState {
    let state = Arc::new(Mutex::new(Live {
        uid: "uid-1".to_string(),
        task_run: String::new(),
        containers: Vec::new(),
    }));
    update(&state, steps, logs);
    state
}

fn update(state: &LiveState, steps: &[&str], logs: &[(&str, &str)]) {
    let mut live = state.lock();
    live.task_run = format!(
        r#"{{"metadata":{{"name":"build-1-lint","namespace":"ci"}},"status":{{"podName":"build-1-lint-pod","startTime":"2026-10-28T10:03:00Z","conditions":[{{"type":"Succeeded","status":"Unknown","reason":"Running"}}],"steps":[{}]}}}}"#,
        steps.join(",")
    );
    live.containers = logs
        .iter()
        .map(|(name, body)| (name.to_string(), format!("id-{name}"), body.to_string()))
        .collect();
}

fn served_log(body: &str, query: &str) -> String {
    let param = |name: &str| {
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix(name))
            .map(str::to_string)
    };
    let since = param("sinceTime=").map(|value| {
        value
            .replace("%3A", ":")
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap()
    });
    let tail = param("tailLines=").map(|value| value.parse::<usize>().unwrap());
    let lines: Vec<&str> = body
        .split_inclusive('\n')
        .filter(|line| {
            since.is_none_or(|since| {
                line.split(' ')
                    .next()
                    .unwrap()
                    .parse::<chrono::DateTime<chrono::Utc>>()
                    .unwrap()
                    >= since
            })
        })
        .collect();
    let skip = tail.map_or(0, |tail| lines.len().saturating_sub(tail));
    lines[skip..].concat()
}

fn live_stub(state: LiveState) -> (String, mpsc::Receiver<String>) {
    let (log, requests) = mpsc::channel();
    let server = serve(move |method, target, _| {
        let _ = log.send(format!("{method} {target}"));
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let live = state.lock();
        let pod = "/api/v1/namespaces/ci/pods/build-1-lint-pod";

        if path == "/apis/tekton.dev/v1/namespaces/ci/pipelineruns/build-1" {
            (200, LOGGED_RUN.to_string())
        } else if path == "/apis/tekton.dev/v1/namespaces/ci/taskruns/build-1-lint" {
            (200, live.task_run.clone())
        } else if path == pod {
            let statuses = live
                .containers
                .iter()
                .map(|(name, id, _)| (name.clone(), id.clone()));
            (200, pod_json("build-1-lint-pod", &live.uid, statuses))
        } else if path == format!("{pod}/log") {
            let container = query
                .split('&')
                .find_map(|pair| pair.strip_prefix("container="))
                .unwrap_or_default();
            match live.containers.iter().find(|(name, ..)| name == container) {
                Some((_, _, body)) => (200, served_log(body, query)),
                None => status_json(404, "NotFound", 0),
            }
        } else {
            status_json(404, "NotFound", 0)
        }
    });
    (server, requests)
}

fn log_requests(requests: &mpsc::Receiver<String>) -> Vec<String> {
    sent(requests)
        .into_iter()
        .filter(|request| request.contains("/log?"))
        .collect()
}

#[tokio::test]
async fn resume_appends_only_new_lines_across_equal_timestamps() {
    let dir = tempfile::tempdir().unwrap();
    let partial = format!("{LINT_LOG}2026-10-28T10:03:03Z d-par");
    let state = live(&[LINT_RUNNING], &[("step-lint", partial.as_str())]);
    let (server, requests) = live_stub(state.clone());
    let plugin = plugin(&server, &dir);

    let (first, read, restarted) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();
    assert_eq!(
        String::from_utf8(first.clone()).unwrap(),
        format!("{LINT_HEADER}{LINT_LOG}")
    );
    assert_eq!(read.mode, LogReadMode::Replace);
    assert!(!restarted);
    let cursor = read.cursor.unwrap();

    let grown = format!("{LINT_LOG}2026-10-28T10:03:03Z d\n2026-10-28T10:03:04Z e\n");
    update(&state, &[LINT_RUNNING], &[("step-lint", grown.as_str())]);
    sent(&requests);
    let (second, read, restarted) = read_node_from(&plugin, "build-1-lint", Some(&cursor))
        .await
        .unwrap();
    assert_eq!(
        String::from_utf8(second.clone()).unwrap(),
        "2026-10-28T10:03:03Z d\n2026-10-28T10:03:04Z e\n"
    );
    assert_eq!(read.mode, LogReadMode::Append);
    assert!(!restarted);
    let requested = log_requests(&requests);
    assert_eq!(requested.len(), 1, "{requested:?}");
    assert!(requested[0].contains("sinceTime=2026-10-28T10%3A03%3A03Z"));
    assert!(!requested[0].contains("tailLines"));
    let next = read.cursor.unwrap();

    let (third, read, restarted) = read_node_from(&plugin, "build-1-lint", Some(&next))
        .await
        .unwrap();
    assert!(third.is_empty());
    assert_eq!(read, LogRead::append(next));
    assert!(!restarted);

    let (oneshot, ..) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();
    assert_eq!([first, second].concat(), oneshot);
}

#[tokio::test]
async fn resume_writes_a_new_container_header_once_and_skips_finished_containers() {
    let dir = tempfile::tempdir().unwrap();
    let state = live(&[LINT_RUNNING], &[("step-lint", LINT_LOG)]);
    let (server, requests) = live_stub(state.clone());
    let plugin = plugin(&server, &dir);
    let (first, read, _) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();

    let lint = format!("{LINT_LOG}2026-10-28T10:03:09Z z\n");
    update(
        &state,
        &[LINT_DONE, REPORT_RUNNING],
        &[
            ("step-lint", lint.as_str()),
            ("step-report", "2026-10-28T10:03:12Z r1\n"),
        ],
    );
    let (second, read, restarted) = read_node_from(&plugin, "build-1-lint", read.cursor.as_ref())
        .await
        .unwrap();
    assert_eq!(
        String::from_utf8(second.clone()).unwrap(),
        format!("2026-10-28T10:03:09Z z\n{REPORT_HEADER}2026-10-28T10:03:12Z r1\n")
    );
    assert_eq!(read.mode, LogReadMode::Append);
    assert!(!restarted);

    let report = "2026-10-28T10:03:12Z r1\n2026-10-28T10:03:13Z r2\n";
    update(
        &state,
        &[LINT_DONE, REPORT_RUNNING],
        &[("step-lint", lint.as_str()), ("step-report", report)],
    );
    sent(&requests);
    let (third, read, restarted) = read_node_from(&plugin, "build-1-lint", read.cursor.as_ref())
        .await
        .unwrap();
    assert_eq!(
        String::from_utf8(third.clone()).unwrap(),
        "2026-10-28T10:03:13Z r2\n"
    );
    assert_eq!(read.mode, LogReadMode::Append);
    assert!(!restarted);
    let requested = log_requests(&requests);
    assert_eq!(requested.len(), 1, "{requested:?}");
    assert!(requested[0].contains("container=step-report"));

    let (oneshot, ..) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();
    assert_eq!([first, second, third].concat(), oneshot);
}

#[tokio::test]
async fn a_changed_pod_container_instance_or_rotated_log_restarts_with_the_whole_log() {
    let cases: [(&str, Change); 3] = [
        ("pod", |state| {
            state.lock().uid = "uid-2".to_string();
        }),
        ("container instance", |state| {
            state.lock().containers[0].1 = "id-restarted".to_string();
        }),
        ("rotated log", |state| {
            state.lock().containers[0].2 = "2026-10-28T10:03:20Z only\n".to_string();
        }),
    ];
    for (name, change) in cases {
        let dir = tempfile::tempdir().unwrap();
        let state = live(&[LINT_RUNNING], &[("step-lint", LINT_LOG)]);
        let (server, _requests) = live_stub(state.clone());
        let plugin = plugin(&server, &dir);
        let (_, read, _) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();

        change(&state);
        let (resumed, read, restarted) =
            read_node_from(&plugin, "build-1-lint", read.cursor.as_ref())
                .await
                .unwrap();
        let (oneshot, ..) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();

        assert!(restarted, "{name}");
        assert_eq!(read.mode, LogReadMode::Replace, "{name}");
        assert!(read.cursor.is_some(), "{name}");
        assert!(resumed.starts_with(LINT_HEADER.as_bytes()), "{name}");
        assert_eq!(resumed, oneshot, "{name}");
    }

    let dir = tempfile::tempdir().unwrap();
    let state = live(&[LINT_RUNNING], &[("step-lint", LINT_LOG)]);
    let (server, _requests) = live_stub(state);
    let plugin = plugin(&server, &dir);
    let unknown = LogCursor::new(&"junk").unwrap();
    let (bytes, read, restarted) = read_node_from(&plugin, "build-1-lint", Some(&unknown))
        .await
        .unwrap();
    assert!(restarted);
    assert_eq!(read.mode, LogReadMode::Replace);
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        format!("{LINT_HEADER}{LINT_LOG}")
    );
}

const SIDECAR_RUNNING: &str = r#"{"name":"lint","running":{"startedAt":"2026-10-28T10:03:01Z"}}"#;
const SIDECAR_DONE: &str = r#"{"name":"lint","terminated":{"exitCode":0,"startedAt":"2026-10-28T10:03:01Z","finishedAt":"2026-10-28T10:03:10Z"}}"#;
const SIDECAR_LOG: &str = "2026-10-28T10:03:02Z SIDECAR_ONLY\n";

fn set_sidecars(state: &LiveState, sidecars: &[&str]) {
    let mut live = state.lock();
    let mut task: serde_json::Value = serde_json::from_str(&live.task_run).unwrap();
    task["status"]["sidecars"] =
        serde_json::from_str(&format!("[{}]", sidecars.join(","))).unwrap();
    live.task_run = task.to_string();
}

#[tokio::test]
async fn sidecar_reads_append_separately_and_flush_the_final_line() {
    let dir = tempfile::tempdir().unwrap();
    let state = live(
        &[LINT_RUNNING],
        &[("step-lint", LINT_LOG), ("sidecar-lint", SIDECAR_LOG)],
    );
    set_sidecars(&state, &[SIDECAR_RUNNING]);
    let (server, requests) = live_stub(state.clone());
    let plugin = plugin(&server, &dir);
    let id = "build-1-lint:sidecar:lint";

    let (first, read, restarted) = read_node_from(&plugin, id, None).await.unwrap();
    assert_eq!(
        String::from_utf8(first.clone()).unwrap(),
        format!("\x1e2026-10-28T10:03:01Z Sidecar lint\n{SIDECAR_LOG}")
    );
    assert_eq!(read.mode, LogReadMode::Replace);
    assert!(!restarted);
    let initial_requests = log_requests(&requests);
    assert_eq!(initial_requests.len(), 1);
    assert!(initial_requests[0].contains("container=sidecar-lint"));

    let grown = format!("{SIDECAR_LOG}2026-10-28T10:03:02Z appended\n2026-10-28T10:03:03Z final");
    state.lock().containers[1].2 = grown;
    let (second, read, restarted) = read_node_from(&plugin, id, read.cursor.as_ref())
        .await
        .unwrap();
    assert_eq!(second, b"2026-10-28T10:03:02Z appended\n");
    assert_eq!(read.mode, LogReadMode::Append);
    assert!(!restarted);
    let requested = log_requests(&requests);
    assert_eq!(requested.len(), 1);
    assert!(requested[0].contains("container=sidecar-lint"));
    assert!(requested[0].contains("sinceTime="));
    assert!(!requested[0].contains("tailLines="));

    set_sidecars(&state, &[SIDECAR_DONE]);
    let (last, read, restarted) = read_node_from(&plugin, id, read.cursor.as_ref())
        .await
        .unwrap();
    assert_eq!(last, b"2026-10-28T10:03:03Z final\n");
    assert_eq!(read.mode, LogReadMode::Append);
    assert!(!restarted);
    let (whole, ..) = read_node_from(&plugin, id, None).await.unwrap();
    assert_eq!([first, second, last].concat(), whole);

    let steps = read_node(&plugin, "build-1-lint").await.unwrap();
    assert_eq!(
        String::from_utf8(steps).unwrap(),
        format!("{LINT_HEADER}{LINT_LOG}")
    );
}

#[tokio::test]
async fn sidecar_reads_use_the_reported_container_and_skip_waiting_sidecars() {
    let dir = tempfile::tempdir().unwrap();
    let state = live(&[LINT_RUNNING], &[("sidecar-custom", SIDECAR_LOG)]);
    set_sidecars(
        &state,
        &[
            r#"{"name":"cache","container":"sidecar-custom","terminated":{"exitCode":1}}"#,
            r#"{"name":"waiting"}"#,
        ],
    );
    let (server, requests) = live_stub(state);
    let plugin = plugin(&server, &dir);

    let bytes = read_node(&plugin, "build-1-lint:sidecar:cache")
        .await
        .unwrap();
    assert!(String::from_utf8(bytes).unwrap().contains("SIDECAR_ONLY"));
    let requested = log_requests(&requests);
    assert_eq!(requested.len(), 1);
    assert!(requested[0].contains("container=sidecar-custom"));

    let bytes = read_node(&plugin, "build-1-lint:sidecar:waiting")
        .await
        .unwrap();
    assert!(bytes.is_empty());
    assert!(log_requests(&requests).is_empty());
}

#[tokio::test]
async fn sidecar_reads_reject_injected_containers_and_foreign_taskruns() {
    let dir = tempfile::tempdir().unwrap();
    let state = live(&[LINT_RUNNING], &[("sidecar-injected", SIDECAR_LOG)]);
    set_sidecars(&state, &[SIDECAR_RUNNING]);
    let (server, requests) = live_stub(state);
    let plugin = plugin(&server, &dir);

    for id in [
        "build-1-lint:sidecar:injected",
        "build-1-stray:sidecar:lint",
    ] {
        let error = read_node(&plugin, id).await.unwrap_err();
        assert!(
            matches!(error, PluginError::PipelineNotFound(_)),
            "{id}: {error:?}"
        );
    }
    assert!(sent(&requests)
        .iter()
        .all(|request| !request.contains("/pods/")));

    for id in [
        "build-1-lint:sidecar:",
        "build-1-lint:sidecar:../lint",
        "build-1-lint:sidecar:lint:other",
        "build-1-lint:sidecar:lint?previous=true",
    ] {
        let error = read_node(&plugin, id).await.unwrap_err();
        assert!(
            matches!(error, PluginError::InvalidConfig(_)),
            "{id}: {error:?}"
        );
    }
    assert!(sent(&requests).is_empty());
}

#[tokio::test]
async fn sidecar_reads_report_a_pruned_pod() {
    let dir = tempfile::tempdir().unwrap();
    let state = live(&[LINT_RUNNING], &[]);
    set_sidecars(&state, &[SIDECAR_RUNNING]);
    let server = serve(move |_, target, _| {
        if target.contains("/pipelineruns/") {
            (200, LOGGED_RUN.to_string())
        } else if target.contains("/taskruns/") {
            (200, state.lock().task_run.clone())
        } else {
            status_json(404, "NotFound", 0)
        }
    });
    let plugin = plugin(&server, &dir);

    let error = read_node(&plugin, "build-1-lint:sidecar:lint")
        .await
        .unwrap_err();
    assert!(
        matches!(error, PluginError::PipelineNotFound(_)),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_running_step_cut_at_the_line_limit_is_a_snapshot_without_a_cursor() {
    let body: String = (0..client::STEP_LOG_TAIL_LINES)
        .map(|index| format!("2026-10-28T10:03:02Z line {index}\n"))
        .collect();
    for (step, resumable) in [(LINT_RUNNING, false), (LINT_DONE, true)] {
        let dir = tempfile::tempdir().unwrap();
        let state = live(&[step], &[("step-lint", body.as_str())]);
        let (server, _requests) = live_stub(state);
        let plugin = plugin(&server, &dir);

        let (_, read, _) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();

        assert_eq!(read.mode, LogReadMode::Replace);
        assert_eq!(read.cursor.is_some(), resumable, "{step}");
    }
}

#[test]
fn configured_repositories_should_show_namespaces_with_a_slash() {
    let config = HashMap::from([("selected_items".to_string(), "ci__build, plain".to_string())]);
    assert_eq!(
        TektonPlugin::new().configured_repositories(&config),
        ["ci/build", "plain"]
    );
}
