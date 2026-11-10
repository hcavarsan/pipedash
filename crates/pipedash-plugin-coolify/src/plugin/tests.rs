use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};

use pipedash_plugin_api::logs::rendered::RenderedCursor;
use serde_json::{
    json,
    Value,
};
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::sync::Mutex;

use super::*;

const TOKEN: &str = "T-stub";
const WEB: &str = "app-web";
const WEB_NEWEST: i64 = 1000;
const WEB_OLDEST: i64 = 941;
const API_NEWEST: i64 = 500;
const API_OLDEST: i64 = 489;

struct Recorded {
    method: String,
    path: String,
    query: String,
    headers: HashMap<String, String>,
}

type Requests = Arc<Mutex<Vec<Recorded>>>;

struct Stub {
    plugin: CoolifyPlugin,
    server_url: String,
    requests: Requests,
    growth: Arc<AtomicUsize>,
}

fn cfg(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn created(id: i64) -> String {
    chrono::DateTime::from_timestamp(1_762_000_000 + id * 60, 0)
        .unwrap()
        .to_rfc3339()
}

fn applications() -> Value {
    json!([
        {
            "uuid": "app-web", "name": "web", "status": "running:healthy",
            "description": null, "git_repository": "acme/web", "git_branch": "main",
            "build_pack": "nixpacks", "docker_registry_image_name": null,
            "docker_registry_image_tag": null, "fqdn": "https://web.example.com",
            "environment_id": 1, "destination": {"server": {"name": "prod-1"}}
        },
        {
            "uuid": "app-api", "name": "api", "status": "running:unhealthy",
            "git_repository": "acme/api", "git_branch": "main", "build_pack": "dockerfile",
            "fqdn": null, "environment_id": 2, "destination": {"server": {"name": "prod-1"}}
        },
        {
            "uuid": "app-img", "name": "img", "status": "exited:unhealthy",
            "git_repository": "coollabsio/coolify", "git_branch": "main",
            "build_pack": "dockerimage", "docker_registry_image_name": "ghcr.io/acme/img",
            "docker_registry_image_tag": "1.2", "environment_id": 3
        },
        {"uuid": "app-down", "name": "down", "status": "running:healthy", "environment_id": 3}
    ])
}

fn projects() -> Value {
    json!([
        {"id": 1, "uuid": "p-pipedash", "name": "pipedash", "description": null},
        {"id": 2, "uuid": "p-kftray", "name": "kftray", "description": "blog"}
    ])
}

fn project(uuid: &str) -> Option<Value> {
    match uuid {
        "p-pipedash" => Some(json!({
            "uuid": uuid, "name": "pipedash",
            "environments": [
                {"id": 1, "uuid": "e1", "name": "production"},
                {"id": 2, "uuid": "e2", "name": "staging"}
            ]
        })),
        "p-kftray" => Some(json!({
            "uuid": uuid, "name": "kftray",
            "environments": [{"id": 3, "uuid": "e3", "name": "production"}]
        })),
        _ => None,
    }
}

fn deployment_url(app: &str, uuid: &str) -> String {
    format!("/project/p/environment/e/application/{app}/deployment/{uuid}")
}

fn deployment(app: &str, id: i64, status: &str) -> Value {
    let uuid = format!("d-{id}");
    json!({
        "id": id, "deployment_uuid": uuid, "application_id": "1", "status": status,
        "created_at": created(id), "updated_at": created(id + 1), "finished_at": null,
        "commit": "HEAD", "commit_message": null,
        "deployment_url": deployment_url(app, &uuid), "server_name": "prod-1",
        "pull_request_id": 0, "is_webhook": true, "is_api": false, "rollback": false,
        "logs": "[]"
    })
}

fn deployments_of(app: &str) -> Option<Vec<Value>> {
    match app {
        "app-web" => Some(
            (WEB_OLDEST..=WEB_NEWEST)
                .rev()
                .map(|id| {
                    deployment(
                        app,
                        id,
                        if id == WEB_NEWEST {
                            "failed"
                        } else {
                            "finished"
                        },
                    )
                })
                .collect(),
        ),
        "app-api" => Some(
            (API_OLDEST..=API_NEWEST)
                .rev()
                .map(|id| deployment(app, id, "finished"))
                .collect(),
        ),
        "app-img" => Some(Vec::new()),
        _ => None,
    }
}

fn entry(command: Option<&str>, output: &str, stream: &str, batch: i64, order: i64) -> Value {
    json!({
        "command": command, "output": output, "type": stream,
        "timestamp": "2026-11-01T10:00:00Z", "hidden": false, "batch": batch, "order": order
    })
}

fn live_logs(grown: bool) -> String {
    let mut entries = vec![
        entry(None, "Starting deployment", "stdout", 1, 1),
        entry(Some("docker build ."), "step 1", "stdout", 2, 2),
    ];
    if grown {
        entries.push(entry(Some("docker build ."), "step 2", "stdout", 2, 3));
        entries.push(entry(Some("docker push"), "denied", "stderr", 3, 4));
    }
    Value::Array(entries).to_string()
}

fn detail(uuid: &str, growth: usize) -> (u16, Value) {
    let base = |id: i64, status: &str| {
        json!({
            "id": id, "deployment_uuid": uuid, "status": status, "created_at": created(id),
            "updated_at": created(id), "deployment_url": deployment_url("app-web", uuid),
            "application": {"uuid": "app-web", "name": "web"}
        })
    };
    let mut value = match uuid {
        "d-nologs" => return (200, base(5, "failed")),
        "d-queued" => base(6, "queued"),
        "d-live" => base(7, "in_progress"),
        "d-new" => base(1001, "queued"),
        "d-gone" => return (503, json!({"message": "Server Error"})),
        _ => return (404, json!({"message": "Deployment not found."})),
    };
    value["logs"] = match uuid {
        "d-live" => json!(live_logs(growth > 0)),
        _ => Value::Null,
    };
    (200, value)
}

fn query_number(query: &str, key: &str) -> Option<usize> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == key)
        .and_then(|(_, value)| value.parse().ok())
}

fn deploy(query: &str) -> (u16, Value) {
    let uuid = query.strip_prefix("uuid=").unwrap_or_default();
    let queued = |deployment_uuid: &str| {
        json!({"deployments": [{
            "message": format!("Application {uuid} deployment queued."),
            "resource_uuid": uuid, "deployment_uuid": deployment_uuid
        }]})
    };
    match uuid {
        "app-web" => (200, queued("d-new")),
        "app-slow" => (200, queued("d-gone")),
        "app-denied" => (
            200,
            json!({"deployments": [{
                "message": "Unauthorized to deploy this application.", "resource_uuid": uuid
            }]}),
        ),
        "boom" => (503, json!({"message": "Server Error"})),
        _ => (404, json!({"message": "No resources found."})),
    }
}

fn reply(request: &Recorded, growth: usize) -> (u16, Value) {
    if request.headers.get("authorization").map(String::as_str) != Some(&format!("Bearer {TOKEN}"))
    {
        return (401, json!({"message": "Unauthenticated."}));
    }
    let path = request.path.strip_prefix("/api/v1").unwrap_or_default();
    match (request.method.as_str(), path) {
        ("GET", "/applications") => (200, applications()),
        ("GET", "/projects") => (200, projects()),
        ("GET", path) if path.starts_with("/projects/") => {
            match project(&path["/projects/".len()..]) {
                Some(project) => (200, project),
                None => (404, json!({"message": "Project not found."})),
            }
        }
        ("GET", path) if path.starts_with("/deployments/applications/") => {
            let app = &path["/deployments/applications/".len()..];
            let Some(all) = deployments_of(app) else {
                return (503, json!({"message": "Server Error"}));
            };
            let skip = query_number(&request.query, "skip").unwrap_or(0);
            let take = query_number(&request.query, "take").unwrap_or(10);
            let count = all.len();
            let page: Vec<Value> = all.into_iter().skip(skip).take(take).collect();
            (200, json!({"count": count, "deployments": page}))
        }
        ("POST", "/deploy") => deploy(&request.query),
        ("POST", "/deployments/d-999/cancel") => (
            400,
            json!({"message": "Deployment cannot be cancelled. Current status: finished"}),
        ),
        ("POST", path) if path.starts_with("/deployments/") && path.ends_with("/cancel") => (
            200,
            json!({"message": "Deployment cancelled successfully."}),
        ),
        ("GET", path) if path.starts_with("/deployments/") => {
            detail(&path["/deployments/".len()..], growth)
        }
        (method, path) => (
            404,
            json!({"message": format!("unexpected {method} {path}")}),
        ),
    }
}

async fn read_request(stream: &mut tokio::net::TcpStream) -> Option<Recorded> {
    let mut raw = Vec::new();
    let mut buffer = [0; 4096];
    let head_end = loop {
        let count = stream.read(&mut buffer).await.ok()?;
        if count == 0 {
            return None;
        }
        raw.extend_from_slice(&buffer[..count]);
        if let Some(position) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            break position;
        }
    };

    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_string();
    let target = request_line.next()?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_string()))
        .collect();
    Some(Recorded {
        method,
        path: path.to_string(),
        query: query.to_string(),
        headers,
    })
}

async fn stub(extra: &[(&str, &str)]) -> Stub {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Requests::default();
    let growth = Arc::new(AtomicUsize::new(0));
    let (recorded, grown) = (requests.clone(), growth.clone());
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let (recorded, grown) = (recorded.clone(), grown.clone());
            tokio::spawn(async move {
                let Some(request) = read_request(&mut stream).await else {
                    return;
                };
                let (status, body) = reply(&request, grown.load(Ordering::SeqCst));
                recorded.lock().await.push(request);
                let body = body.to_string();
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });

    let server_url = format!("http://{addr}");
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut pairs = vec![("server_url", server_url.as_str()), ("token", TOKEN)];
    pairs.extend_from_slice(extra);
    let mut plugin = CoolifyPlugin::new();
    plugin
        .initialize(1, cfg(&pairs), Some(Arc::new(reqwest::Client::new())))
        .unwrap();
    Stub {
        plugin,
        server_url,
        requests,
        growth,
    }
}

async fn calls(requests: &Requests, method: &str, prefix: &str) -> Vec<String> {
    requests
        .lock()
        .await
        .iter()
        .filter(|request| request.method == method && request.path.starts_with(prefix))
        .map(|request| format!("{}?{}", request.path, request.query))
        .collect()
}

fn run_ref<'a>(metadata: &'a HashMap<String, Value>, run_number: i64) -> RunRef<'a> {
    RunRef {
        pipeline_id: WEB,
        run_number,
        run_id: "app-web__run",
        metadata,
    }
}

fn uuid_metadata(uuid: &str) -> HashMap<String, Value> {
    HashMap::from([("deployment_uuid".to_string(), json!(uuid))])
}

async fn read(
    plugin: &CoolifyPlugin, run: RunRef<'_>, from: Option<&LogCursor>,
) -> PluginResult<(String, LogRead, bool)> {
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let read = async {
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(run, mapper::LOG_NODE_ID, from, &mut sink)
            .await;
        (result, sink.restarted())
    };
    let collect = async {
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        bytes
    };
    let ((result, restarted), bytes) = tokio::join!(read, collect);
    result.map(|read| (String::from_utf8(bytes).unwrap(), read, restarted))
}

#[tokio::test]
async fn pipelines_combine_the_latest_deployment_with_live_health() {
    let stub = stub(&[("selected_items", "app-web, api,img")]).await;

    let pipelines = stub.plugin.fetch_pipelines().await.unwrap();

    let summary: Vec<_> = pipelines
        .iter()
        .map(|pipeline| {
            (
                pipeline.name.as_str(),
                pipeline.status,
                pipeline
                    .resource_state
                    .as_ref()
                    .map(|state| state.label.as_str()),
                pipeline.metadata["state"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("web", PipelineStatus::Failed, None, Some("Up")),
            (
                "api",
                PipelineStatus::Degraded,
                Some("Unhealthy"),
                Some("Unhealthy")
            ),
            (
                "img",
                PipelineStatus::Cancelled,
                Some("Exited"),
                Some("Exited")
            ),
        ]
    );
    assert_eq!(pipelines[0].id, WEB);
    assert_eq!(pipelines[0].metadata["server"], json!("prod-1"));
    assert_eq!(pipelines[0].metadata["deployment_status"], json!("failed"));
    assert_eq!(
        pipelines[0].last_run.map(|at| at.to_rfc3339()),
        Some(created(WEB_NEWEST))
    );
    assert_eq!(pipelines[2].last_run, None);
    assert_eq!(pipelines[2].repository, "ghcr.io/acme/img:1.2");
    assert_eq!(
        calls(&stub.requests, "GET", "/api/v1/deployments/").await,
        [
            "/api/v1/deployments/applications/app-web?skip=0&take=1",
            "/api/v1/deployments/applications/app-api?skip=0&take=1",
            "/api/v1/deployments/applications/app-img?skip=0&take=1",
        ]
    );
}

#[tokio::test]
async fn an_empty_selection_lists_nothing_and_reads_no_deployments() {
    let stub = stub(&[("selected_items", "")]).await;

    assert!(stub.plugin.fetch_pipelines().await.unwrap().is_empty());
    assert!(calls(&stub.requests, "GET", "/api/v1/deployments/")
        .await
        .is_empty());
}

#[tokio::test]
async fn one_failing_application_fails_the_whole_fetch() {
    let stub = stub(&[("selected_items", "web,down")]).await;

    let error = stub.plugin.fetch_pipelines().await.unwrap_err();

    assert!(
        matches!(error, PluginError::ServiceUnavailable { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn run_history_pages_until_the_limit_or_a_short_page() {
    let stub = stub(&[]).await;

    let runs = stub.plugin.fetch_run_history(WEB, 45).await.unwrap();
    let numbers: Vec<i64> = runs.iter().map(|run| run.run_number).collect();
    assert_eq!(numbers, (956..=WEB_NEWEST).rev().collect::<Vec<_>>());
    assert_eq!(runs[0].status, PipelineStatus::Failed);
    assert_eq!(runs[0].metadata["deployment_uuid"], json!("d-1000"));
    assert_eq!(runs[0].metadata["source"], json!("Webhook"));
    assert_eq!(
        calls(&stub.requests, "GET", "/api/v1/deployments/").await,
        [
            "/api/v1/deployments/applications/app-web?skip=0&take=20",
            "/api/v1/deployments/applications/app-web?skip=20&take=20",
            "/api/v1/deployments/applications/app-web?skip=40&take=5",
        ]
    );

    let stub = self::stub(&[]).await;
    let runs = stub.plugin.fetch_run_history("app-api", 45).await.unwrap();
    assert_eq!(runs.len(), 12);
    assert_eq!(
        calls(&stub.requests, "GET", "/api/v1/deployments/").await,
        ["/api/v1/deployments/applications/app-api?skip=0&take=20"]
    );
}

#[tokio::test]
async fn the_newest_finished_deployment_is_degraded_while_the_application_is_unhealthy() {
    let stub = stub(&[]).await;

    let runs = stub.plugin.fetch_run_history("app-api", 2).await.unwrap();
    let newest = stub.plugin.fetch_run_details("app-api", API_NEWEST).await;
    let older = stub
        .plugin
        .fetch_run_details("app-api", API_NEWEST - 1)
        .await;

    let statuses: Vec<_> = runs.iter().map(|run| run.status).collect();
    assert_eq!(
        statuses,
        [PipelineStatus::Degraded, PipelineStatus::Success]
    );
    assert_eq!(newest.unwrap().status, PipelineStatus::Degraded);
    assert_eq!(older.unwrap().status, PipelineStatus::Success);
}

#[tokio::test]
async fn run_details_page_until_the_id_and_stop_once_ids_are_older() {
    let stub = stub(&[]).await;

    let run = stub.plugin.fetch_run_details(WEB, 970).await.unwrap();
    assert_eq!(run.run_number, 970);
    assert_eq!(run.id, "app-web__d-970");
    assert_eq!(
        calls(&stub.requests, "GET", "/api/v1/deployments/")
            .await
            .len(),
        2
    );

    let stub = self::stub(&[]).await;
    let error = stub
        .plugin
        .fetch_run_details(WEB, WEB_NEWEST + 5)
        .await
        .unwrap_err();
    assert!(
        matches!(error, PluginError::PipelineNotFound(_)),
        "{error:?}"
    );
    assert_eq!(
        calls(&stub.requests, "GET", "/api/v1/deployments/")
            .await
            .len(),
        1
    );
}

#[tokio::test]
async fn logs_hidden_from_the_token_ask_for_read_sensitive() {
    let stub = stub(&[]).await;
    let metadata = uuid_metadata("d-nologs");

    let error = stub
        .plugin
        .fetch_log_nodes(run_ref(&metadata, 5))
        .await
        .unwrap_err();

    match error {
        PluginError::PermissionDenied(message) => assert!(message.contains("read:sensitive")),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_queued_deployment_has_a_pending_node_and_no_output() {
    let stub = stub(&[]).await;
    let metadata = uuid_metadata("d-queued");

    let nodes = stub
        .plugin
        .fetch_log_nodes(run_ref(&metadata, 6))
        .await
        .unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].log, Some(LogAvailability::Pending));
    assert_eq!(
        nodes[0].web_url.as_deref(),
        Some(
            format!(
                "{}{}",
                stub.server_url,
                deployment_url("app-web", "d-queued")
            )
            .as_str()
        )
    );

    let (output, _, _) = read(&stub.plugin, run_ref(&metadata, 6), None)
        .await
        .unwrap();
    assert_eq!(output, "");
}

#[tokio::test]
async fn live_logs_append_only_new_lines_and_restart_on_foreign_cursors() {
    let stub = stub(&[]).await;
    let metadata = uuid_metadata("d-live");
    let first = ". Starting deployment\n$ docker build .\n. step 1\n";
    let added = ". step 2\n$ docker push\n! denied\n";

    let (output, read_1, restarted) = read(&stub.plugin, run_ref(&metadata, 7), None)
        .await
        .unwrap();
    assert_eq!(output, first);
    assert_eq!(read_1.mode, LogReadMode::Replace);
    assert!(!restarted);

    stub.growth.store(1, Ordering::SeqCst);
    let (output, read_2, restarted) =
        read(&stub.plugin, run_ref(&metadata, 7), read_1.cursor.as_ref())
            .await
            .unwrap();
    assert_eq!(output, added);
    assert_eq!(read_2.mode, LogReadMode::Append);
    assert!(!restarted);

    let foreign = LogCursor::new(&RenderedCursor::of(b"! something else\n")).unwrap();
    let (output, read_3, restarted) = read(&stub.plugin, run_ref(&metadata, 7), Some(&foreign))
        .await
        .unwrap();
    assert_eq!(output, format!("{first}{added}"));
    assert_eq!(read_3.mode, LogReadMode::Replace);
    assert!(restarted);
}

fn trigger(workflow_id: &str) -> TriggerParams {
    TriggerParams {
        workflow_id: workflow_id.to_string(),
        inputs: None,
    }
}

#[tokio::test]
async fn trigger_queues_one_deploy_and_reports_the_new_run() {
    let stub = stub(&[]).await;

    let result = stub.plugin.trigger_pipeline(trigger(WEB)).await.unwrap();

    assert_eq!(
        result,
        TriggerResult {
            run_id: None,
            run_number: Some(1001),
        }
    );
    let requests = stub.requests.lock().await;
    let posts: Vec<&Recorded> = requests.iter().filter(|r| r.method == "POST").collect();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].path, "/api/v1/deploy");
    assert_eq!(posts[0].query, "uuid=app-web");
    assert_eq!(
        posts[0].headers.get("authorization").map(String::as_str),
        Some("Bearer T-stub")
    );
}

#[tokio::test]
async fn a_failed_deploy_request_is_never_resent() {
    let stub = stub(&[]).await;

    let error = stub
        .plugin
        .trigger_pipeline(trigger("boom"))
        .await
        .unwrap_err();

    assert!(
        matches!(error, PluginError::ServiceUnavailable { .. }),
        "{error:?}"
    );
    assert_eq!(calls(&stub.requests, "POST", "/").await.len(), 1);
}

#[tokio::test]
async fn a_deploy_coolify_did_not_queue_is_an_error_with_its_reason() {
    let stub = stub(&[]).await;

    let error = stub
        .plugin
        .trigger_pipeline(trigger("app-denied"))
        .await
        .unwrap_err();

    match error {
        PluginError::ApiError(message) => {
            assert!(message.contains("Unauthorized to deploy this application."))
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_queued_deploy_succeeds_even_when_its_details_cannot_be_read() {
    let stub = stub(&[]).await;

    let result = stub
        .plugin
        .trigger_pipeline(trigger("app-slow"))
        .await
        .unwrap();

    assert_eq!(result, TriggerResult::default());
    assert_eq!(calls(&stub.requests, "POST", "/").await.len(), 1);
}

#[tokio::test]
async fn cancel_resolves_the_deployment_and_posts_once() {
    let stub = stub(&[]).await;

    stub.plugin.cancel_run(WEB, WEB_NEWEST).await.unwrap();
    assert_eq!(
        calls(&stub.requests, "POST", "/").await,
        ["/api/v1/deployments/d-1000/cancel?"]
    );

    let stub = self::stub(&[]).await;
    let error = stub.plugin.cancel_run(WEB, 999).await.unwrap_err();
    assert!(matches!(error, PluginError::ApiError(_)), "{error:?}");
    assert_eq!(calls(&stub.requests, "POST", "/").await.len(), 1);
}

#[tokio::test]
async fn discovery_groups_applications_by_project() {
    let stub = stub(&[]).await;

    let organizations: Vec<String> = stub
        .plugin
        .fetch_organizations()
        .await
        .unwrap()
        .into_iter()
        .map(|organization| organization.id)
        .collect();
    assert_eq!(organizations, ["kftray", "pipedash"]);

    let available = stub.plugin.fetch_available_pipelines(None).await.unwrap();
    let web = &available.items[0];
    assert_eq!(web.id, "app-web");
    assert_eq!(web.organization.as_deref(), Some("pipedash"));
    assert_eq!(
        web.description.as_deref(),
        Some("State: running:healthy | Environment: production | Source: acme/web@main")
    );

    let kftray: Vec<String> = stub
        .plugin
        .fetch_available_pipelines_filtered(Some("kftray".to_string()), None, None)
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|pipeline| pipeline.id)
        .collect();
    assert_eq!(kftray, ["app-img", "app-down"]);
}

#[tokio::test]
async fn initialize_requires_a_token() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut plugin = CoolifyPlugin::new();

    let error = plugin
        .initialize(
            1,
            cfg(&[("server_url", "https://coolify.example.com")]),
            Some(Arc::new(reqwest::Client::new())),
        )
        .unwrap_err();

    assert!(
        matches!(&error, PluginError::InvalidConfig(message) if message == "Missing token in config"),
        "{error:?}"
    );
}

#[test]
fn configured_repositories_should_be_empty() {
    let config = HashMap::from([("selected_items".to_string(), "a".to_string())]);
    assert!(CoolifyPlugin::new()
        .configured_repositories(&config)
        .is_empty());
}
