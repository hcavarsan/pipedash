use std::sync::Arc;

use super::*;

fn cfg(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn client_options_parse_origin_and_insecure() {
    let plugin = ArgocdPlugin::new();
    let options = plugin.client_options(&cfg(&[
        ("server_url", " https://Argo.example.com:8443/ "),
        ("insecure", "true"),
    ]));
    assert_eq!(
        options.origin.as_ref().map(|u| u.as_str()),
        Some("https://argo.example.com:8443/")
    );
    assert!(options.accept_invalid_certs);
}

#[test]
fn client_options_default_secure_for_missing_or_bad_values() {
    let plugin = ArgocdPlugin::new();
    for insecure in ["false", "yes", ""] {
        let options = plugin.client_options(&cfg(&[
            ("server_url", "https://argo.example.com"),
            ("insecure", insecure),
        ]));
        assert!(!options.accept_invalid_certs);
    }
    assert!(
        !plugin
            .client_options(&cfg(&[("server_url", "https://a.io")]))
            .accept_invalid_certs
    );
}

#[test]
fn client_options_origin_none_when_missing_or_invalid() {
    let plugin = ArgocdPlugin::new();
    assert_eq!(plugin.client_options(&cfg(&[])).origin, None);
    assert_eq!(
        plugin
            .client_options(&cfg(&[("server_url", "not a url")]))
            .origin,
        None
    );
}

#[test]
fn initialize_requires_client_and_validates_status_page() {
    let base = [("server_url", "https://a.io"), ("token", "t")];
    let mut plugin = ArgocdPlugin::new();

    assert!(matches!(
        plugin.initialize(1, cfg(&base), None),
        Err(PluginError::Internal(_))
    ));

    let mut bad = base.to_vec();
    bad.push(("status_page_url", "http://example.com"));
    assert!(matches!(
        plugin.initialize(1, cfg(&bad), None),
        Err(PluginError::InvalidConfig(_))
    ));
    assert!(plugin.status_source().is_none());
}

const APP: &str = r#"{"metadata":{"name":"web","namespace":"argocd"},
    "spec":{"source":{"repoURL":"https://github.com/acme/web","path":"deploy","targetRevision":"main"},
        "destination":{"server":"https://kubernetes.default.svc","namespace":"web"},"project":"default"},
    "status":{"sync":{"status":"OutOfSync","revision":"def456"},"health":{"status":"Degraded"},
        "history":[{"revision":"abc123","deployedAt":"2026-10-26T09:00:30Z","deployStartedAt":"2026-10-26T09:00:00Z","id":3,
            "source":{"repoURL":"https://ci:s3cret@github.com/acme/web?token=abc","path":"deploy"},"initiatedBy":{"username":"admin"}}],
        "operationState":{"operation":{"sync":{"revision":"def456","prune":true},"initiatedBy":{"automated":true}},
            "phase":"Failed","message":"one or more objects failed to apply","startedAt":"2026-10-27T10:00:00Z",
            "finishedAt":"2026-10-27T10:00:42Z","retryCount":2,
            "syncResult":{"revision":"def456","resources":[
                {"group":"batch","version":"v1","kind":"Job","namespace":"web","name":"db-migrate","hookType":"PreSync","hookPhase":"Succeeded","syncPhase":"PreSync","message":"job completed"},
                {"group":"apps","version":"v1","kind":"Deployment","namespace":"web","name":"web","status":"Synced","syncPhase":"Sync","message":"deployment.apps/web configured"},
                {"group":"","version":"v1","kind":"ConfigMap","namespace":"web","name":"web-config","status":"SyncFailed","syncPhase":"Sync","message":"ConfigMap \"web-config\" is invalid"}]}}}}"#;

const TREE: &str = r#"{"nodes":[
    {"group":"apps","version":"v1","kind":"Deployment","namespace":"web","name":"web","uid":"d1","health":{"status":"Degraded"}},
    {"version":"v1","kind":"Pod","namespace":"web","name":"web-7f9c-def","uid":"p2","health":{"status":"Healthy"}},
    {"version":"v1","kind":"Pod","namespace":"web","name":"web-7f9c-abc","uid":"p1","health":{"status":"Degraded"}}]}"#;

const CRASH_TREE: &str = r#"{"nodes":[
    {"group":"apps","version":"v1","kind":"Deployment","namespace":"web","name":"crash","uid":"d1","health":{"status":"Progressing"}},
    {"version":"v1","kind":"Pod","namespace":"web","name":"web-7f9c-def","uid":"p2","health":{"status":"Healthy"}},
    {"version":"v1","kind":"Pod","namespace":"web","name":"web-7f9c-abc","uid":"p1","health":{"status":"Degraded","message":"back-off 40s restarting failed container=app"}},
    {"version":"v1","kind":"Pod","namespace":"web","name":"web-7f9c-ghi","uid":"p3","health":{"status":"Degraded","message":"Back-off pulling image"}}]}"#;

const ROLLOUT_TREE: &str = r#"{"nodes":[
    {"group":"apps","version":"v1","kind":"Deployment","namespace":"web","name":"rollout","uid":"d1","health":{"status":"Progressing"}},
    {"version":"v1","kind":"Pod","namespace":"web","name":"rollout-1","uid":"p1","health":{"status":"Progressing"}}]}"#;

fn listed_app(name: &str, health: &str) -> serde_json::Value {
    serde_json::json!({
        "metadata": {"name": name, "namespace": "argocd"},
        "spec": {"source": {"repoURL": "https://github.com/acme/web"}, "destination": {"namespace": "web"}, "project": "default"},
        "status": {"sync": {"status": "Synced"}, "health": {"status": health, "message": format!("{name} is {health}")}},
    })
}

const APP_EVENTS: &str = r#"{"metadata":{},"items":[
    {"type":"Warning","reason":"OperationCompleted","message":"Sync operation to def456 failed","count":1,"firstTimestamp":"2026-10-27T10:00:42Z","lastTimestamp":"2026-10-27T10:00:42Z"},
    {"type":"Normal","reason":"OperationStarted","message":"Initiated automated sync to 'def456'","firstTimestamp":"2026-10-27T10:00:00Z","lastTimestamp":"2026-10-27T10:00:00Z","eventTime":null}]}"#;

const POD_EVENTS: &str = r#"{"metadata":{},"items":[
    {"type":"Warning","reason":"BackOff","message":"Back-off restarting failed container app","count":12,"firstTimestamp":"2026-10-27T10:01:00Z","lastTimestamp":"2026-10-27T10:06:00Z"}]}"#;

const CRASHING_POD: &str = r#"{"metadata":{"name":"web-7f9c-abc","namespace":"web","uid":"p1"},
    "spec":{"nodeName":"node-1","initContainers":[{"name":"migrate","image":"migrate:1"}],"containers":[{"name":"app","image":"web:2"}]},
    "status":{"phase":"Running",
        "initContainerStatuses":[{"name":"migrate","restartCount":0,"state":{"terminated":{"exitCode":0,"reason":"Completed"}}}],
        "containerStatuses":[{"name":"app","restartCount":3,
            "state":{"waiting":{"reason":"CrashLoopBackOff","message":"back-off 40s restarting failed container=app"}},
            "lastState":{"terminated":{"exitCode":1,"reason":"Error","finishedAt":"2026-10-27T10:05:02Z"}}}]}}"#;

const HEALTHY_POD: &str = r#"{"metadata":{"name":"web-7f9c-def","namespace":"web","uid":"p2"},
    "spec":{"containers":[{"name":"app"}]},
    "status":{"phase":"Running","containerStatuses":[{"name":"app","restartCount":0,"state":{"running":{}}}]}}"#;

const LAST: &str =
    r#"{"result":{"content":"","timeStampStr":"2026-10-27T10:06:00Z","podName":"","last":true}}"#;

fn log_stream(lines: &[&str]) -> String {
    lines
        .iter()
        .map(|content| {
            format!(
                "{}\n",
                serde_json::json!({"result": {
                    "content": content,
                    "timeStampStr": "2026-10-27T10:05:01.5Z",
                    "podName": "web-7f9c-abc",
                    "last": false,
                }})
            )
        })
        .chain([format!("{LAST}\n")])
        .collect()
}

fn argocd_reply(target: &str) -> (&'static str, String) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let has = |pair: &str| query.split('&').any(|part| part == pair);
    let ok = |body: &str| ("200 OK", body.to_string());
    match path {
        "/api/v1/settings" => ok(r#"{"url":"","dexConfig":{"connectors":[{"name":"GitHub","type":"github","id":"github"}]}}"#),
        "/api/v1/applications/web" => ok(APP),
        "/api/v1/applications/web/resource-tree" => ok(TREE),
        "/api/v1/applications" => ok(&serde_json::json!({"items": [
            listed_app("crash", "Progressing"),
            listed_app("rollout", "Progressing"),
            listed_app("ready", "Healthy"),
            listed_app("forbidden", "Progressing"),
            listed_app("broken", "Degraded"),
        ]})
        .to_string()),
        "/api/v1/applications/crash/resource-tree" => ok(CRASH_TREE),
        "/api/v1/applications/rollout/resource-tree" => ok(ROLLOUT_TREE),
        "/api/v1/applications/forbidden/resource-tree" => (
            "403 Forbidden",
            r#"{"error":"permission denied: applications, get, default/forbidden","code":7,"message":"permission denied: applications, get, default/forbidden"}"#
                .to_string(),
        ),
        "/api/v1/applications/broken/resource-tree" => (
            "500 Internal Server Error",
            r#"{"message":"boom"}"#.to_string(),
        ),
        "/api/v1/applications/web/events" if query.is_empty() => ok(APP_EVENTS),
        "/api/v1/applications/web/events" if has("resourceName=web-7f9c-abc") => {
            ok(POD_EVENTS)
        }
        "/api/v1/applications/web/events" => ok(r#"{"metadata":{},"items":null}"#),
        "/api/v1/applications/web/resource" if has("kind=Pod") && has("version=v1") => {
            let pod = if has("resourceName=web-7f9c-abc") {
                CRASHING_POD
            } else {
                HEALTHY_POD
            };
            ok(&serde_json::json!({ "manifest": pod }).to_string())
        }
        "/api/v1/applications/web/logs" if has("podName=web-7f9c-def") => (
            "403 Forbidden",
            r#"{"error":"permission denied: logs, get, default/web","code":7,"message":"permission denied: logs, get, default/web"}"#
                .to_string(),
        ),
        "/api/v1/applications/web/logs" if has("tailLines=1000") && has("follow=false") => {
            if has("container=migrate") {
                ok(&log_stream(&["migrations applied"]))
            } else if has("previous=true") {
                ok(&log_stream(&["panic: missing DATABASE_URL"]))
            } else {
                ok(&format!("{LAST}\n"))
            }
        }
        _ => ("404 Not Found", r#"{"message":"not found"}"#.to_string()),
    }
}

type Requests = Arc<tokio::sync::Mutex<Vec<String>>>;
type Reply = fn(&str, &str) -> (&'static str, String);

async fn serve(requests: Requests, reply: Reply) -> String {
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let requests = requests.clone();
            tokio::spawn(async move {
                let mut data = Vec::new();
                let mut buffer = [0; 4096];
                let split = loop {
                    if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        break at + 4;
                    }
                    match stream.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => data.extend_from_slice(&buffer[..n]),
                    }
                };
                let head = String::from_utf8_lossy(&data[..split]).to_string();
                let length = head
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                while data.len() < split + length {
                    match stream.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => data.extend_from_slice(&buffer[..n]),
                    }
                }
                let body = String::from_utf8_lossy(&data[split..split + length]).to_string();
                let mut parts = head.split_whitespace();
                let method = parts.next().unwrap_or("GET").to_string();
                let target = parts.next().unwrap_or("/").to_string();
                let (status, payload) = reply(&method, &target);
                requests.lock().await.push(if method == "GET" {
                    target
                } else {
                    format!("{method} {target} {body}")
                });
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    payload.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
}

async fn stub_argocd(requests: Requests) -> String {
    serve(requests, |_, target| argocd_reply(target)).await
}

async fn stub_plugin() -> (ArgocdPlugin, Arc<tokio::sync::Mutex<Vec<String>>>) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let requests = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let server_url = stub_argocd(requests.clone()).await;
    let mut plugin = ArgocdPlugin::new();
    plugin
        .initialize(
            1,
            cfg(&[("server_url", &server_url), ("token", "t")]),
            Some(Arc::new(reqwest::Client::new())),
        )
        .unwrap();
    (plugin, requests)
}

#[tokio::test]
async fn sign_in_client_reads_unauthenticated_settings() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let requests = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let server_url = stub_argocd(requests.clone()).await;

    let client = ArgocdPlugin::new()
        .sign_in_client(
            &cfg(&[("server_url", &server_url)]),
            &reqwest::Client::new(),
        )
        .await
        .unwrap();

    assert_eq!(client.issuer, format!("{server_url}/api/dex"));
    assert_eq!(client.label, "GitHub");
    assert_eq!(*requests.lock().await, ["/api/v1/settings"]);
}

async fn fetch_states() -> (HashMap<String, Pipeline>, Vec<String>) {
    let (plugin, requests) = stub_plugin().await;
    let pipelines = plugin.fetch_pipelines().await.unwrap();
    let requests = requests.lock().await.clone();
    (
        pipelines
            .into_iter()
            .map(|pipeline| (pipeline.name.clone(), pipeline))
            .collect(),
        requests,
    )
}

fn state_of(pipeline: &Pipeline) -> (PipelineStatus, &str, Option<&str>) {
    let state = pipeline.resource_state.as_ref().unwrap();
    (
        pipeline.status,
        state.label.as_str(),
        state.message.as_deref(),
    )
}

#[tokio::test]
async fn progressing_app_with_crash_looping_pod_is_degraded() {
    let (pipelines, _) = fetch_states().await;

    assert_eq!(
        state_of(&pipelines["crash"]),
        (
            PipelineStatus::Degraded,
            "Degraded",
            Some("Pod web/web-7f9c-abc: back-off 40s restarting failed container=app (+1 more)")
        )
    );
}

#[tokio::test]
async fn progressing_app_with_healthy_tree_stays_progressing() {
    let (pipelines, _) = fetch_states().await;

    assert_eq!(
        state_of(&pipelines["rollout"]),
        (
            PipelineStatus::Running,
            "Progressing",
            Some("rollout is Progressing")
        )
    );
}

#[tokio::test]
async fn healthy_app_costs_no_resource_tree_request() {
    let (pipelines, requests) = fetch_states().await;

    assert_eq!(
        state_of(&pipelines["ready"]),
        (PipelineStatus::Success, "Synced", Some("ready is Healthy"))
    );
    assert!(requests.iter().all(|target| !target.contains("/ready")));
    for unhealthy in ["crash", "rollout", "forbidden", "broken"] {
        let tree = format!("/api/v1/applications/{unhealthy}/resource-tree");
        assert!(requests.contains(&tree), "{unhealthy}");
    }
}

#[tokio::test]
async fn failing_resource_tree_keeps_the_provider_state() {
    let (pipelines, requests) = fetch_states().await;

    assert_eq!(
        state_of(&pipelines["forbidden"]),
        (
            PipelineStatus::Running,
            "Progressing",
            Some("forbidden is Progressing")
        )
    );
    assert_eq!(
        state_of(&pipelines["broken"]),
        (
            PipelineStatus::Degraded,
            "Degraded",
            Some("broken is Degraded")
        )
    );
    let forbidden = "/api/v1/applications/forbidden/resource-tree";
    assert_eq!(requests.iter().filter(|t| *t == forbidden).count(), 1);
}

const PIPELINE_ID: &str = "argocd__web";

fn run_at(stamp: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(stamp)
        .unwrap()
        .timestamp()
}

async fn read(
    plugin: &ArgocdPlugin, run_number: i64, node_id: &str,
) -> PluginResult<Vec<(LineMarker, String)>> {
    read_in(plugin, PIPELINE_ID, run_number, node_id).await
}

async fn read_in(
    plugin: &ArgocdPlugin, pipeline_id: &str, run_number: i64, node_id: &str,
) -> PluginResult<Vec<(LineMarker, String)>> {
    let metadata = HashMap::new();
    let run = RunRef {
        pipeline_id,
        run_number,
        run_id: "argocd-sync-web-current",
        metadata: &metadata,
    };
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let mut sink = LogSink::new(tx);
    let result = plugin.read_log(run, node_id, None, &mut sink).await;
    assert!(!sink.restarted());
    drop(sink);
    let mut bytes = Vec::new();
    while let Some(chunk) = rx.recv().await {
        bytes.extend_from_slice(&chunk);
    }
    assert_eq!(result?, LogRead::replace(None));
    Ok(bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let parsed = plugin.log_dialect().parse(line);
            (
                parsed.marker,
                String::from_utf8_lossy(parsed.body).into_owned(),
            )
        })
        .collect())
}

fn line(marker: LineMarker, text: &str) -> (LineMarker, String) {
    (marker, text.to_string())
}

const OPEN: LineMarker = LineMarker::SectionStart { open: true };
const CLOSED: LineMarker = LineMarker::SectionStart { open: false };

#[tokio::test]
async fn log_nodes_list_sync_result_events_and_pods() {
    let (plugin, _) = stub_plugin().await;
    let metadata = HashMap::new();
    let run = RunRef {
        pipeline_id: PIPELINE_ID,
        run_number: run_at("2026-10-27T10:00:00Z"),
        run_id: "argocd-sync-web-current",
        metadata: &metadata,
    };

    let nodes = plugin.fetch_log_nodes(run).await.unwrap();

    let summary: Vec<_> = nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.parent_id.as_deref(),
                node.status,
                node.log,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "sync",
                None,
                PipelineStatus::Failed,
                Some(LogAvailability::Ready)
            ),
            (
                "events",
                None,
                PipelineStatus::Success,
                Some(LogAvailability::Partial)
            ),
            ("pods", None, PipelineStatus::Failed, None),
            (
                "pod:web:web-7f9c-abc",
                Some("pods"),
                PipelineStatus::Failed,
                Some(LogAvailability::Partial)
            ),
            (
                "pod:web:web-7f9c-def",
                Some("pods"),
                PipelineStatus::Success,
                Some(LogAvailability::Partial)
            ),
        ]
    );
    assert_eq!(nodes[0].name, "Sync result");
    assert_eq!(nodes[3].name, "web-7f9c-abc");
}

#[tokio::test]
async fn failed_sync_lists_resource_results_by_phase() {
    let (plugin, _) = stub_plugin().await;

    let lines = read(&plugin, run_at("2026-10-27T10:00:00Z"), "sync")
        .await
        .unwrap();

    assert_eq!(
        lines,
        [
            line(OPEN, "Sync operation"),
            line(LineMarker::Plain, "Started by the automated sync policy"),
            line(LineMarker::Plain, "Revision def456"),
            line(LineMarker::Plain, "Options: prune"),
            line(LineMarker::Plain, "Retried 2 times"),
            line(LineMarker::Error, "one or more objects failed to apply"),
            line(LineMarker::Plain, "Failed after 42s"),
            line(CLOSED, "PreSync phase, 1 resource"),
            line(
                LineMarker::Plain,
                "Succeeded    batch/Job web/db-migrate (PreSync hook): job completed"
            ),
            line(OPEN, "Sync phase, 2 resources"),
            line(
                LineMarker::Plain,
                "Synced       apps/Deployment web/web: deployment.apps/web configured"
            ),
            line(
                LineMarker::Error,
                "SyncFailed   ConfigMap web/web-config: ConfigMap \"web-config\" is invalid"
            ),
        ]
    );
}

#[tokio::test]
async fn older_sync_shows_its_history_record() {
    let (plugin, _) = stub_plugin().await;

    let lines = read(&plugin, run_at("2026-10-26T09:00:30Z"), "sync")
        .await
        .unwrap();

    assert_eq!(
        lines,
        [
            line(OPEN, "Sync"),
            line(LineMarker::Plain, "Started by admin"),
            line(LineMarker::Plain, "Revision abc123"),
            line(
                LineMarker::Plain,
                "Source https://github.com/acme/web deploy"
            ),
            line(LineMarker::Plain, "Deployed after 30s"),
            line(
                LineMarker::Plain,
                "Argo CD keeps per-resource results only for the most recent operation of an app."
            ),
        ]
    );
}

#[tokio::test]
async fn application_events_come_in_time_order() {
    let (plugin, _) = stub_plugin().await;

    let lines = read(&plugin, run_at("2026-10-27T10:00:00Z"), "events")
        .await
        .unwrap();

    assert_eq!(
        lines,
        [
            line(
                LineMarker::Plain,
                "Normal OperationStarted: Initiated automated sync to 'def456'"
            ),
            line(
                LineMarker::Warning,
                "Warning OperationCompleted: Sync operation to def456 failed"
            ),
        ]
    );
}

#[tokio::test]
async fn crashing_pod_shows_status_events_and_every_container_attempt() {
    let (plugin, requests) = stub_plugin().await;

    let lines = read(
        &plugin,
        run_at("2026-10-27T10:00:00Z"),
        "pod:web:web-7f9c-abc",
    )
    .await
    .unwrap();

    assert_eq!(
        lines,
        [
            line(OPEN, "Pod web/web-7f9c-abc"),
            line(LineMarker::Plain, "Phase Running on node node-1"),
            line(
                LineMarker::Plain,
                "Init container migrate: exited with code 0 (Completed)"
            ),
            line(
                LineMarker::Error,
                "Container app: waiting (CrashLoopBackOff), 3 restarts, last exit code 1 (Error): back-off 40s restarting failed container=app"
            ),
            line(OPEN, "Events"),
            line(
                LineMarker::Warning,
                "Warning BackOff: Back-off restarting failed container app (x12)"
            ),
            line(CLOSED, "Init container migrate"),
            line(LineMarker::Plain, "migrations applied"),
            line(OPEN, "Container app, previous attempt"),
            line(LineMarker::Plain, "panic: missing DATABASE_URL"),
            line(OPEN, "Container app"),
            line(LineMarker::Plain, "No log output."),
        ]
    );
    let requests = requests.lock().await;
    assert!(
        requests
            .iter()
            .any(|r| r.contains("resourceUID=p1") && r.contains("resourceNamespace=web")),
        "{requests:?}"
    );
}

#[tokio::test]
async fn pod_log_without_the_logs_permission_is_permission_denied() {
    let (plugin, _) = stub_plugin().await;

    let error = read(
        &plugin,
        run_at("2026-10-27T10:00:00Z"),
        "pod:web:web-7f9c-def",
    )
    .await
    .unwrap_err();

    assert!(
        matches!(&error, PluginError::PermissionDenied(message) if message.contains("logs, get")),
        "{error:?}"
    );
}

#[tokio::test]
async fn unknown_node_ids_are_rejected_without_a_request() {
    let (plugin, requests) = stub_plugin().await;

    for node_id in [
        "",
        "pods",
        "pod:web",
        "pod:web:a?b=1",
        "pod:..:x",
        "pod/web/a",
    ] {
        let error = read(&plugin, run_at("2026-10-27T10:00:00Z"), node_id)
            .await
            .unwrap_err();
        assert!(
            matches!(error, PluginError::InvalidConfig(_)),
            "{node_id}: {error:?}"
        );
    }
    assert!(requests.lock().await.is_empty());
}

#[test]
fn pipeline_actions_should_target_the_destination_cluster_and_namespace() {
    let plugin = ArgocdPlugin::new();
    let build = |entries: &[(&str, &str)]| {
        let metadata: HashMap<String, serde_json::Value> = entries
            .iter()
            .map(|(key, value)| (key.to_string(), serde_json::json!(value)))
            .collect();
        let facts = PipelineFacts {
            id: "app",
            status: PipelineStatus::Success,
            branch: None,
            workflow_file: None,
            resource_state: None,
            metadata: &metadata,
        };
        plugin.pipeline_actions(&facts).target
    };

    let both = build(&[
        ("destination_cluster", " in-cluster "),
        ("destination_namespace", "prod"),
    ]);
    assert_eq!(
        both,
        Some(DisplayField {
            label: "Destination".to_string(),
            value: "in-cluster / prod".to_string()
        })
    );
    let only = build(&[("destination_namespace", "prod")]);
    assert_eq!(only.map(|field| field.value), Some("prod".to_string()));
    assert_eq!(build(&[]), None);
    assert!(plugin
        .configured_repositories(&HashMap::from([(
            "selected_items".to_string(),
            "a".to_string()
        )]))
        .is_empty());
}

const APPS_API: &str = "/apis/argoproj.io/v1alpha1";

fn cluster_app(name: &str, server: &str) -> serde_json::Value {
    let mut app: serde_json::Value = serde_json::from_str(APP).unwrap();
    app["metadata"]["name"] = serde_json::json!(name);
    app["metadata"]["resourceVersion"] = serde_json::json!("7");
    app["spec"]["destination"]["server"] = serde_json::json!(server);
    app["status"]["resources"] = serde_json::json!([
        {"group": "apps", "kind": "Deployment", "namespace": "web", "name": "api"}
    ]);
    app
}

fn plain_app(name: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut app = serde_json::json!({
        "metadata": {"name": name, "namespace": "argocd", "resourceVersion": "7"},
        "spec": {"source": {"repoURL": "https://github.com/acme/web"},
            "destination": {"name": "in-cluster"}, "project": "default"},
        "status": {"sync": {"status": "Synced"}, "health": {"status": "Healthy"},
            "operationState": {"phase": "Succeeded", "startedAt": "2026-10-27T10:00:00Z"}},
    });
    if let Some(extra) = extra.as_object() {
        for (key, value) in extra {
            app[key] = value.clone();
        }
    }
    app
}

fn progressing_app(name: &str, namespace: &str) -> serde_json::Value {
    plain_app(
        name,
        serde_json::json!({"status": {
            "sync": {"status": "Synced"}, "health": {"status": "Progressing"},
            "operationState": {"phase": "Succeeded", "startedAt": "2026-10-27T10:00:00Z"},
            "history": [{"deployedAt": "2026-10-26T09:00:30Z", "deployStartedAt": "2026-10-26T09:00:00Z", "id": 1, "revision": "abc"}],
            "resources": [{"group": "apps", "kind": "Deployment", "namespace": namespace, "name": "cart"}],
        }}),
    )
}

fn cluster_reply(method: &str, target: &str) -> (&'static str, String) {
    let path = target.split('?').next().unwrap_or(target);
    let ok = |body: serde_json::Value| ("200 OK", body.to_string());
    if method == "PATCH" {
        return ok(serde_json::json!({}));
    }
    if let Some(name) = path.strip_prefix(&format!("{APPS_API}/namespaces/argocd/applications/")) {
        return match name {
            "web" => ok(cluster_app("web", "https://kubernetes.default.svc")),
            "remote" => {
                let mut app = cluster_app("remote", "https://prod.example.com");
                app["status"]["resources"] = serde_json::json!([
                    {"group": "apps", "kind": "Deployment", "namespace": "web", "name": "api"},
                    {"group": "", "kind": "Pod", "namespace": "web", "name": "bare"}
                ]);
                ok(app)
            }
            "idle" => ok(plain_app("idle", serde_json::json!({}))),
            "busy" => ok(plain_app(
                "busy",
                serde_json::json!({"operation": {"sync": {}}}),
            )),
            "stuck" => ok(progressing_app("stuck", "shop")),
            "throttled" | "restricted" => ok(progressing_app(name, name)),
            _ => cluster_missing(),
        };
    }
    if path.contains("/namespaces/throttled/") {
        return cluster_refusal("429 Too Many Requests", 429);
    }
    if path.contains("/namespaces/restricted/") {
        return cluster_refusal("403 Forbidden", 403);
    }
    match path {
        "/apis/argoproj.io/v1alpha1/namespaces/ops/applications" => {
            ok(serde_json::json!({"items": [progressing_app("throttled", "throttled")]}))
        }
        "/apis/argoproj.io/v1alpha1/namespaces/argocd/applications" => {
            let mut fresh = plain_app("fresh", serde_json::json!({}));
            fresh.as_object_mut().unwrap().remove("status");
            ok(serde_json::json!({"items": [fresh]}))
        }
        "/apis/argoproj.io/v1alpha1/namespaces/team/applications" => {
            ok(serde_json::json!({"items": []}))
        }
        "/api/v1/namespaces/web/pods" => ok(serde_json::json!({"items": [
            {"metadata": {"name": "api-7f-a", "namespace": "web", "uid": "u1",
                "ownerReferences": [{"kind": "ReplicaSet", "name": "api-7f"}]},
                "spec": {"containers": [{"name": "app"}]},
                "status": {"phase": "Running", "containerStatuses": [
                    {"name": "app", "ready": true, "restartCount": 0, "state": {"running": {}}}]}},
            {"metadata": {"name": "stranger", "namespace": "web", "uid": "u2"},
                "spec": {"containers": [{"name": "app"}]}, "status": {"phase": "Running"}},
        ]})),
        "/apis/apps/v1/namespaces/web/replicasets" => ok(serde_json::json!({"items": [
            {"metadata": {"name": "api-7f", "namespace": "web",
                "ownerReferences": [{"kind": "Deployment", "name": "api"}]}},
        ]})),
        "/apis/batch/v1/namespaces/web/jobs" => ok(serde_json::json!({"items": []})),
        "/api/v1/namespaces/shop/pods" => ok(serde_json::json!({"items": [
            {"metadata": {"name": "cart-5c-x", "namespace": "shop", "uid": "u3",
                "ownerReferences": [{"kind": "ReplicaSet", "name": "cart-5c"}]},
                "spec": {"containers": [{"name": "app"}]},
                "status": {"phase": "Pending", "containerStatuses": [
                    {"name": "app", "ready": false, "restartCount": 0,
                        "state": {"waiting": {"reason": "ImagePullBackOff"}}}]}},
        ]})),
        "/apis/apps/v1/namespaces/shop/replicasets" => ok(serde_json::json!({"items": [
            {"metadata": {"name": "cart-5c", "namespace": "shop",
                "ownerReferences": [{"kind": "Deployment", "name": "cart"}]}},
        ]})),
        "/apis/batch/v1/namespaces/shop/jobs" => ok(serde_json::json!({"items": []})),
        "/api/v1/namespaces/web/pods/api-7f-a/log" => (
            "200 OK",
            "2026-10-27T10:05:01.5Z first line\n2026-10-27T10:05:02Z second line\n".to_string(),
        ),
        "/api/v1/namespaces/web/events" | "/api/v1/namespaces/argocd/events" => {
            ok(serde_json::json!({"items": []}))
        }
        _ => cluster_missing(),
    }
}

fn cluster_refusal(status: &'static str, code: u16) -> (&'static str, String) {
    let body = serde_json::json!({
        "kind": "Status", "apiVersion": "v1", "status": "Failure",
        "message": "refused", "reason": "Refused", "code": code,
    });
    (status, body.to_string())
}

fn cluster_missing() -> (&'static str, String) {
    (
        "404 Not Found",
        r#"{"kind":"Status","apiVersion":"v1","status":"Failure","message":"not found","reason":"NotFound","code":404}"#
            .to_string(),
    )
}

async fn cluster_plugin(extra: &[(&str, &str)]) -> (ArgocdPlugin, Requests) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let requests = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let server = serve(requests.clone(), cluster_reply).await;
    let kubeconfig = format!(
        "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster:\n    server: {server}\ncontexts:\n- name: ctx\n  context:\n    cluster: c\n    user: u\ncurrent-context: ctx\nusers:\n- name: u\n  user:\n    token: t\n"
    );
    let mut config = cfg(&[("auth_mode", "kubeconfig"), ("token", &kubeconfig)]);
    config.extend(cfg(extra));
    let mut plugin = ArgocdPlugin::new();
    plugin.initialize(1, config, None).unwrap();
    (plugin, requests)
}

fn patches(requests: &[String]) -> Vec<(String, serde_json::Value)> {
    requests
        .iter()
        .filter_map(|request| request.strip_prefix("PATCH "))
        .map(|rest| {
            let (target, body) = rest.split_once(' ').unwrap();
            (target.to_string(), serde_json::from_str(body).unwrap())
        })
        .collect()
}

fn sync_of(pipeline: &str, inputs: serde_json::Value) -> TriggerParams {
    TriggerParams {
        workflow_id: pipeline.to_string(),
        inputs: Some(inputs),
    }
}

#[tokio::test]
async fn kubernetes_mode_needs_neither_http_client_nor_server_url() {
    let (plugin, requests) = cluster_plugin(&[]).await;

    assert!(plugin.server_url.is_none());
    assert!(plugin.client().is_ok());
    let runs = plugin.fetch_run_history("argocd__web", 5).await.unwrap();
    assert!(!runs.is_empty());
    assert!(runs.iter().all(|run| run.logs_url.is_empty()));
    assert_eq!(requests.lock().await.len(), 1);
}

#[tokio::test]
async fn kubernetes_mode_lists_each_configured_namespace_and_never_cluster_wide() {
    let (plugin, requests) = cluster_plugin(&[("namespaces", "argocd,team")]).await;

    let pipelines = plugin.fetch_pipelines().await.unwrap();

    assert_eq!(pipelines.len(), 1);
    assert_eq!(
        state_of(&pipelines[0]),
        (PipelineStatus::Pending, "Unknown", None)
    );
    let paths: Vec<_> = requests
        .lock()
        .await
        .iter()
        .map(|request| request.split('?').next().unwrap().to_string())
        .collect();
    assert_eq!(
        paths,
        [
            format!("{APPS_API}/namespaces/argocd/applications"),
            format!("{APPS_API}/namespaces/team/applications"),
        ]
    );
}

#[tokio::test]
async fn kubernetes_mode_without_namespaces_lists_cluster_wide() {
    let (plugin, requests) = cluster_plugin(&[("namespaces", "")]).await;

    let _ = plugin.fetch_pipelines().await;

    let requests = requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with(&format!("{APPS_API}/applications")));
}

#[tokio::test]
async fn kubernetes_sync_sends_one_merge_patch_guarded_by_the_resource_version() {
    let (plugin, requests) = cluster_plugin(&[]).await;

    plugin
        .trigger_pipeline(sync_of(
            "argocd__idle",
            serde_json::json!({"prune": true, "dry_run": true}),
        ))
        .await
        .unwrap();

    let patches = patches(&requests.lock().await);
    assert_eq!(patches.len(), 1);
    let (target, body) = &patches[0];
    assert!(target.starts_with(&format!("{APPS_API}/namespaces/argocd/applications/idle")));
    assert_eq!(body["metadata"]["resourceVersion"], "7");
    assert_eq!(body["operation"]["initiatedBy"]["username"], "pipedash");
    assert_eq!(body["operation"]["sync"]["prune"], true);
    assert_eq!(body["operation"]["sync"]["dryRun"], true);
}

#[tokio::test]
async fn kubernetes_sync_is_refused_while_an_operation_is_pending() {
    let (plugin, requests) = cluster_plugin(&[]).await;

    let error = plugin
        .trigger_pipeline(sync_of("argocd__busy", serde_json::json!({})))
        .await
        .unwrap_err();

    assert!(matches!(error, PluginError::ApiError(_)), "{error:?}");
    assert!(patches(&requests.lock().await).is_empty());
}

#[tokio::test]
async fn kubernetes_cancel_marks_the_running_operation_as_terminating() {
    let (plugin, requests) = cluster_plugin(&[]).await;

    plugin.cancel_run("argocd__busy", 0).await.unwrap();

    let patches = patches(&requests.lock().await);
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].1["metadata"]["resourceVersion"], "7");
    assert_eq!(
        patches[0].1["status"]["operationState"]["phase"],
        "Terminating"
    );
}

#[tokio::test]
async fn kubernetes_cancel_without_an_operation_sends_no_patch() {
    let (plugin, requests) = cluster_plugin(&[]).await;

    let error = plugin.cancel_run("argocd__idle", 0).await.unwrap_err();

    assert!(matches!(error, PluginError::InvalidConfig(_)), "{error:?}");
    assert!(patches(&requests.lock().await).is_empty());
}

#[tokio::test]
async fn kubernetes_mode_rejects_pipelines_outside_the_configured_namespaces() {
    let (plugin, requests) = cluster_plugin(&[("namespaces", "argocd")]).await;
    let run = run_at("2026-10-27T10:00:00Z");

    let errors = [
        plugin.fetch_run_history("other__web", 5).await.unwrap_err(),
        plugin.cancel_run("other__web", 0).await.unwrap_err(),
        plugin
            .trigger_pipeline(sync_of("other__web", serde_json::json!({})))
            .await
            .unwrap_err(),
        read_in(&plugin, "other__web", run, "events")
            .await
            .unwrap_err(),
    ];

    for error in errors {
        assert!(
            matches!(&error, PluginError::InvalidConfig(message) if message.contains("outside the configured namespaces")),
            "{error:?}"
        );
    }
    assert!(requests.lock().await.is_empty());
}

async fn node_ids(plugin: &ArgocdPlugin, pipeline_id: &str) -> Vec<String> {
    let metadata = HashMap::new();
    let run = RunRef {
        pipeline_id,
        run_number: run_at("2026-10-27T10:00:00Z"),
        run_id: "argocd-sync-current",
        metadata: &metadata,
    };
    let nodes = plugin.fetch_log_nodes(run).await.unwrap();
    assert!(nodes.iter().all(|node| node.web_url.is_none()));
    nodes.into_iter().map(|node| node.id).collect()
}

#[tokio::test]
async fn kubernetes_log_nodes_list_only_pods_the_application_owns() {
    let (plugin, _) = cluster_plugin(&[]).await;

    assert_eq!(
        node_ids(&plugin, "argocd__web").await,
        ["sync", "events", "pods", "pod:web:api-7f-a"]
    );
}

#[tokio::test]
async fn kubernetes_application_in_another_cluster_lists_no_pods() {
    let (plugin, requests) = cluster_plugin(&[]).await;

    assert_eq!(
        node_ids(&plugin, "argocd__remote").await,
        ["sync", "events"]
    );

    let requests = requests.lock().await;
    for kind in ["/pods", "/replicasets", "/jobs"] {
        assert!(
            requests.iter().all(|request| !request.contains(kind)),
            "{kind}"
        );
    }
}

#[tokio::test]
async fn kubernetes_pod_outside_the_application_cannot_be_read() {
    let (plugin, requests) = cluster_plugin(&[]).await;

    let error = read(&plugin, run_at("2026-10-27T10:00:00Z"), "pod:web:stranger")
        .await
        .unwrap_err();

    assert!(
        matches!(error, PluginError::PipelineNotFound(_)),
        "{error:?}"
    );
    assert!(requests
        .lock()
        .await
        .iter()
        .all(|request| !request.contains("/log")));
}

#[tokio::test]
async fn kubernetes_pod_log_is_written_one_line_per_log_line() {
    let (plugin, _) = cluster_plugin(&[]).await;

    let lines = read(&plugin, run_at("2026-10-27T10:00:00Z"), "pod:web:api-7f-a")
        .await
        .unwrap();

    assert_eq!(
        lines[lines.len() - 3..],
        [
            line(OPEN, "Container app"),
            line(LineMarker::Plain, "first line"),
            line(LineMarker::Plain, "second line"),
        ]
    );
}

#[tokio::test]
async fn kubernetes_sync_that_left_a_pod_unable_to_start_is_degraded() {
    let (plugin, _) = cluster_plugin(&[]).await;

    let runs = plugin.fetch_run_history("argocd__stuck", 5).await.unwrap();
    let details = plugin
        .fetch_run_details("argocd__stuck", runs[0].run_number)
        .await
        .unwrap();

    assert_eq!(runs[0].status, PipelineStatus::Degraded);
    assert_eq!(details.status, PipelineStatus::Degraded);
}

#[tokio::test]
async fn kubernetes_throttled_pod_check_fails_instead_of_reporting_a_partial_result() {
    let (listing, _) = cluster_plugin(&[("namespaces", "ops")]).await;
    let (plugin, _) = cluster_plugin(&[]).await;
    let metadata = HashMap::new();
    let run_number = run_at("2026-10-27T10:00:00Z");
    let run = RunRef {
        pipeline_id: "argocd__throttled",
        run_number,
        run_id: "argocd-sync-current",
        metadata: &metadata,
    };

    let failures = [
        listing.fetch_pipelines().await.map(drop),
        plugin
            .fetch_run_history("argocd__throttled", 5)
            .await
            .map(drop),
        plugin
            .fetch_run_details("argocd__throttled", run_number)
            .await
            .map(drop),
        plugin.fetch_log_nodes(run).await.map(drop),
    ];

    for failure in failures {
        assert!(
            matches!(failure, Err(PluginError::RateLimited { .. })),
            "{failure:?}"
        );
    }
}

#[tokio::test]
async fn kubernetes_older_run_opens_while_the_pod_check_is_failing() {
    let (plugin, requests) = cluster_plugin(&[]).await;

    let older = plugin
        .fetch_run_details("argocd__throttled", run_at("2026-10-26T09:00:30Z"))
        .await
        .unwrap();

    assert_eq!(older.status, PipelineStatus::Success);
    let requests = requests.lock().await;
    assert!(requests.iter().all(|request| !request.contains("/pods")));
}

#[tokio::test]
async fn kubernetes_without_pod_permissions_keeps_the_state_argocd_reports() {
    let (plugin, _) = cluster_plugin(&[]).await;

    let runs = plugin
        .fetch_run_history("argocd__restricted", 5)
        .await
        .unwrap();

    assert_eq!(runs[0].status, PipelineStatus::Success);
    assert_eq!(
        node_ids(&plugin, "argocd__restricted").await,
        ["sync", "events"]
    );
}
