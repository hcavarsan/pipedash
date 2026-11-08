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

async fn stub_argocd(requests: Arc<tokio::sync::Mutex<Vec<String>>>) -> String {
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
                let mut head = Vec::new();
                let mut buffer = [0; 4096];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buffer[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&head);
                let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (status, body) = argocd_reply(&target);
                requests.lock().await.push(target);
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
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
    let metadata = HashMap::new();
    let run = RunRef {
        pipeline_id: PIPELINE_ID,
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
