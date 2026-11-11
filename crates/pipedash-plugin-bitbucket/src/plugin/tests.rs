use std::net::SocketAddr;
use std::sync::{
    mpsc,
    Arc,
};

use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};

use super::*;

const STUB_COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const STUB_PARENT: &str = "fedcba9876543210fedcba9876543210fedcba98";
const STUB_DIFF: &str = "diff --git a/deploy/values.yaml b/deploy/values.yaml\n\
index 1..2 100644\n\
--- a/deploy/values.yaml\n\
+++ b/deploy/values.yaml\n\
@@ -1 +1 @@\n\
-tag: a\n\
+tag: b\n\
diff --git a/old.txt b/old.txt\n\
deleted file mode 100644\n\
--- a/old.txt\n\
+++ /dev/null\n\
@@ -1 +0,0 @@\n\
-gone\n\
diff --git a/logo.png b/logo.png\n\
Binary files differ\n";

async fn serve_runs(total: i64) -> (BitbucketPlugin, mpsc::Receiver<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (recorded, requests) = mpsc::channel();

    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buffer = [0; 4096];
            let count = stream.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..count]);
            let line = request.lines().next().unwrap().to_string();
            let target = line.split_whitespace().nth(1).unwrap();
            let url = reqwest::Url::parse(&format!("http://{address}{target}")).unwrap();
            let (status, body) = match (line.starts_with("GET "), url.path()) {
                (true, "/repositories/ws/repo/pipelines") => {
                    let page = url
                        .query_pairs()
                        .find(|(key, _)| key == "page")
                        .map(|(_, value)| value.parse::<i64>().unwrap())
                        .unwrap_or(1);
                    let high = total - (page - 1) * 100;
                    let low = (high - 99).max(1);
                    let values: Vec<_> = (low..=high)
                        .rev()
                        .map(|number| {
                            serde_json::json!({
                                "uuid": format!("{{{number}}}"),
                                "build_number": number,
                                "state": if number == 1 {
                                    serde_json::json!({"name": "IN_PROGRESS"})
                                } else {
                                    serde_json::json!({
                                        "name": "COMPLETED",
                                        "result": {"name": "SUCCESSFUL"}
                                    })
                                },
                                "target": {"type": "pipeline_ref_target", "ref_name": "main"},
                                "created_on": "2026-10-01T00:00:00Z",
                                "links": {}
                            })
                        })
                        .collect();
                    let next = (low > 1).then(|| format!(
                        "http://{address}/repositories/ws/repo/pipelines?pagelen=100&sort=-created_on&page={}",
                        page + 1
                    ));
                    (
                        "200 OK",
                        serde_json::json!({
                            "values": values, "next": next, "pagelen": 100
                        })
                        .to_string(),
                    )
                }
                (true, "/repositories") => {
                    let repo = |slug: &str| {
                        serde_json::json!({
                            "uuid": format!("{{{slug}}}"), "name": slug,
                            "full_name": format!("ws/{slug}"), "slug": slug,
                            "workspace": {"uuid": "{ws}", "slug": "ws", "name": "ws"},
                            "links": {"html": {"href": format!("http://{address}/ws/{slug}")}}
                        })
                    };
                    (
                        "200 OK",
                        serde_json::json!({
                            "values": [repo("repo"), repo("empty"), repo("configured")],
                            "size": 3, "pagelen": 100
                        })
                        .to_string(),
                    )
                }
                (true, "/repositories/ws/empty/pipelines")
                | (true, "/repositories/ws/configured/pipelines") => (
                    "200 OK",
                    serde_json::json!({"values": [], "pagelen": 100}).to_string(),
                ),
                (true, "/repositories/ws/configured/src/HEAD/bitbucket-pipelines.yml") => {
                    ("200 OK", "pipelines: {}".to_string())
                }
                (true, "/repositories/ws/repo/pipelines/%7B1%7D/steps") => (
                    "200 OK",
                    serde_json::json!({"values": [], "pagelen": 100}).to_string(),
                ),
                (false, "/repositories/ws/repo/pipelines/%7B1%7D/stopPipeline") => {
                    ("204 No Content", String::new())
                }
                (true, "/repositories/ws/repo/commit/main") => (
                    "200 OK",
                    serde_json::json!({
                        "hash": STUB_COMMIT, "date": "2026-10-01T10:00:00+02:00",
                        "message": "ship it", "parents": [{"hash": STUB_PARENT}]
                    })
                    .to_string(),
                ),
                (true, "/repositories/ws/repo/commits") => {
                    let include = url.query_pairs().find(|(key, _)| key == "include");
                    let ahead = include.is_some_and(|(_, value)| value == STUB_COMMIT);
                    let values = if ahead {
                        vec![serde_json::json!({"hash": STUB_COMMIT})]
                    } else {
                        Vec::new()
                    };
                    (
                        "200 OK",
                        serde_json::json!({"values": values, "pagelen": 100}).to_string(),
                    )
                }
                (true, path) if path == format!("/repositories/ws/repo/diffstat/{STUB_COMMIT}") => (
                    "200 OK",
                    serde_json::json!({
                        "values": [
                            {"old": {"path": "deploy/values.yaml"}, "new": {"path": "deploy/values.yaml"}},
                            {"old": {"path": "old.txt"}, "new": null},
                            {"old": null, "new": {"path": "logo.png"}}
                        ],
                        "pagelen": 500
                    })
                    .to_string(),
                ),
                (true, path) if path == format!("/repositories/ws/repo/diff/{STUB_COMMIT}") => {
                    ("200 OK", STUB_DIFF.to_string())
                }
                _ => ("404 Not Found", "{}".to_string()),
            };
            recorded.send(line).unwrap();
            let response = format!(
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut plugin = BitbucketPlugin::new();
    plugin.client = Some(client::BitbucketClient::new(
        Arc::new(reqwest::Client::builder().no_proxy().build().unwrap()),
        format!("http://{address}"),
        "Basic stub".to_string(),
    ));
    (plugin, requests)
}

#[tokio::test]
async fn run_details_finds_old_builds_without_a_page_cap() {
    let (plugin, requests) = serve_runs(1150).await;

    let run = plugin.fetch_run_details("ws__repo", 1).await.unwrap();
    assert_eq!(run.run_number, 1);
    assert_eq!(run.status, PipelineStatus::Running);
    assert_eq!(requests.try_iter().count(), 12);

    let latest = plugin.fetch_run_details("ws__repo", 1150).await.unwrap();
    assert_eq!(latest.run_number, 1150);
    assert_eq!(latest.status, PipelineStatus::Success);
    assert_eq!(requests.try_iter().count(), 1);
}

#[tokio::test]
async fn only_repositories_without_runs_and_pipelines_file_are_dropped_from_a_selection() {
    let (plugin, requests) = serve_runs(3).await;

    let kept = plugin
        .ids_with_ci(
            ["ws/repo", "ws/empty", "ws/configured"]
                .map(String::from)
                .to_vec(),
        )
        .await
        .unwrap();

    assert_eq!(kept, ["ws/repo", "ws/configured"]);
    assert!(requests
        .try_iter()
        .any(|line| line.contains("/ws/empty/src/HEAD/bitbucket-pipelines.yml")));
}

#[tokio::test]
async fn tracking_every_repository_lists_those_with_runs_or_a_pipelines_file() {
    let (plugin, _requests) = serve_runs(3).await;

    let pipelines = plugin.fetch_pipelines().await.unwrap();

    assert_eq!(
        pipelines
            .iter()
            .map(|pipeline| (pipeline.id.as_str(), pipeline.status))
            .collect::<Vec<_>>(),
        [
            ("ws__repo", PipelineStatus::Success),
            ("ws__configured", PipelineStatus::Pending)
        ]
    );
}

#[tokio::test]
async fn run_details_rejects_missing_builds_after_the_last_page() {
    for total in [0, 150] {
        let (plugin, requests) = serve_runs(total).await;

        let error = plugin
            .fetch_run_details("ws__repo", 9999)
            .await
            .unwrap_err();
        assert!(matches!(error, PluginError::PipelineNotFound(_)));
        assert_eq!(requests.try_iter().count(), if total == 0 { 1 } else { 2 });
    }
}

#[tokio::test]
async fn cancel_run_stops_an_old_running_build_by_its_uuid() {
    let (plugin, requests) = serve_runs(150).await;

    plugin.cancel_run("ws__repo", 1).await.unwrap();

    let requests: Vec<_> = requests.try_iter().collect();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests.last().unwrap(),
        "POST /repositories/ws/repo/pipelines/%7B1%7D/stopPipeline HTTP/1.1"
    );
}

const PIPELINE_UUID: &str = "{3f9d6e5c-2b1a-4c7d-8e9f-0a1b2c3d4e5f}";
const STEP_BUILD: &str = "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}";
const STEP_TEST: &str = "{2c1f6d8b-1e22-4a7f-8b68-4d2e3f5a6b72}";
const STEP_DEPLOY: &str = "{3d207e9c-2f33-4b80-9c79-5e3f4a6b7c83}";
const STEP_RELEASE: &str = "{4e318fad-3044-4c91-8d8a-6f405b7c8d94}";
const STEP_SMOKE: &str = "{5f4290be-4155-4da2-9e9b-70516c8d9ea5}";
const STEP_PRUNED: &str = "{60503acf-5266-4eb3-8fac-81627d9eafb6}";
const STEP_EXPIRED: &str = "{71614bd0-6377-4fc4-90bd-92738eafb0c7}";
const BUILD_LOG: &str = "+ npm ci\nadded 214 packages\n+ npm run build\nbuilt in 9s\n";
const TEST_LOG: &str = "+ make test\nrunning 3 tests\ntest result: FAILED. 2 passed; 1 failed\n";

struct Reply {
    status: &'static str,
    headers: Vec<(&'static str, String)>,
    body: String,
}

fn reply(status: &'static str, body: &str) -> Reply {
    Reply {
        status,
        headers: Vec::new(),
        body: body.to_string(),
    }
}

type Recorded = Arc<tokio::sync::Mutex<Vec<(String, Option<String>)>>>;

async fn stub(handler: impl Fn(&str, &str) -> Reply + Send + 'static) -> (SocketAddr, Recorded) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let recorded = Recorded::default();
    let journal = recorded.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buffer = [0; 8192];
            let count = stream.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..count]);
            let mut lines = request.lines();
            let request_line = lines.next().unwrap().to_string();
            let header = |wanted: &str| {
                request
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
                    .map(|(_, value)| value.trim().to_string())
            };
            let target = request_line.split_whitespace().nth(1).unwrap();
            let reply = handler(target, &header("host").unwrap());
            journal
                .lock()
                .await
                .push((request_line.clone(), header("authorization")));
            let headers: String = reply
                .headers
                .iter()
                .map(|(name, value)| format!("{name}: {value}\r\n"))
                .collect();
            let response = format!(
                "HTTP/1.1 {}\r\n{headers}content-length: {}\r\nconnection: close\r\n\r\n{}",
                reply.status,
                reply.body.len(),
                reply.body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        }
    });
    (address, recorded)
}

fn steps_path() -> String {
    format!(
        "/repositories/ws/repo/pipelines/{}/steps",
        urlencoding::encode(PIPELINE_UUID)
    )
}

fn log_path(step_uuid: &str) -> String {
    format!("{}/{}/log", steps_path(), urlencoding::encode(step_uuid))
}

fn step(uuid: &str, name: &str, state: serde_json::Value, started: bool) -> serde_json::Value {
    let mut step = serde_json::json!({"uuid": uuid, "name": name, "state": state});
    if started {
        step["started_on"] = "2026-10-28T10:00:00.000Z".into();
    }
    if state["name"] == "COMPLETED" && started {
        step["completed_on"] = "2026-10-28T10:01:15.000Z".into();
    }
    step
}

fn completed(result: &str) -> serde_json::Value {
    serde_json::json!({"name": "COMPLETED", "result": {"name": result}})
}

fn steps_page(host: &str, second_page: bool) -> String {
    let (values, next) = if second_page {
        (
            vec![
                step(
                    STEP_DEPLOY,
                    "Deploy",
                    serde_json::json!({"name": "IN_PROGRESS"}),
                    true,
                ),
                step(
                    STEP_RELEASE,
                    "Release",
                    serde_json::json!({"name": "PENDING"}),
                    false,
                ),
                step(STEP_SMOKE, "Smoke", completed("NOT_RUN"), false),
            ],
            None,
        )
    } else {
        (
            vec![
                step(STEP_BUILD, "Build", completed("SUCCESSFUL"), true),
                step(STEP_TEST, "Test", completed("FAILED"), true),
            ],
            Some(format!("http://{host}{}?pagelen=100&page=2", steps_path())),
        )
    };
    serde_json::json!({"values": values, "next": next, "pagelen": 100}).to_string()
}

fn pipelines_page() -> String {
    serde_json::json!({
        "values": [{
            "uuid": PIPELINE_UUID,
            "build_number": 7,
            "state": {"name": "IN_PROGRESS"},
            "target": {"type": "pipeline_ref_target", "ref_name": "main"},
            "created_on": "2026-10-28T10:00:00Z",
            "links": {}
        }],
        "pagelen": 100
    })
    .to_string()
}

async fn serve_pipeline_logs() -> (BitbucketPlugin, Recorded, Recorded) {
    let (storage, stored) = stub(|target, _| match target {
        "/storage/test.log" => reply("200 OK", TEST_LOG),
        _ => reply("404 Not Found", "{}"),
    })
    .await;
    let (api, requests) = stub(move |target, host| {
        let url = reqwest::Url::parse(&format!("http://{host}{target}")).unwrap();
        let path = url.path();
        if path == "/repositories/ws/repo/pipelines" {
            reply("200 OK", &pipelines_page())
        } else if path == steps_path() {
            let second_page = url.query_pairs().any(|(key, _)| key == "page");
            reply("200 OK", &steps_page(host, second_page))
        } else if path == log_path(STEP_BUILD) {
            reply("200 OK", BUILD_LOG)
        } else if path == log_path(STEP_TEST) {
            Reply {
                status: "307 Temporary Redirect",
                headers: vec![("location", format!("http://{storage}/storage/test.log"))],
                body: String::new(),
            }
        } else if path == log_path(STEP_EXPIRED) {
            reply("410 Gone", "{}")
        } else {
            reply("404 Not Found", "{}")
        }
    })
    .await;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut plugin = BitbucketPlugin::new();
    plugin.client = Some(client::BitbucketClient::new(
        Arc::new(reqwest::Client::builder().no_proxy().build().unwrap()),
        format!("http://{api}"),
        "Basic stub".to_string(),
    ));
    (plugin, requests, stored)
}

fn run_7(metadata: &HashMap<String, serde_json::Value>) -> RunRef<'_> {
    RunRef {
        pipeline_id: "ws__repo",
        run_number: 7,
        run_id: "ws__repo__7",
        metadata,
    }
}

async fn read_step(plugin: &BitbucketPlugin, node_id: &str) -> PluginResult<Vec<u8>> {
    let metadata = HashMap::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut sink = LogSink::new(tx);
    let result = plugin
        .read_log(run_7(&metadata), node_id, None, &mut sink)
        .await;
    drop(sink);
    let mut bytes = Vec::new();
    while let Some(chunk) = rx.recv().await {
        bytes.extend_from_slice(&chunk);
    }
    result.map(|_| bytes)
}

#[tokio::test]
async fn steps_on_two_pages_become_job_nodes() {
    let (plugin, requests, _) = serve_pipeline_logs().await;
    let metadata = HashMap::new();

    let nodes = plugin.fetch_log_nodes(run_7(&metadata)).await.unwrap();

    let summary: Vec<_> = nodes
        .iter()
        .map(|node| (node.id.as_str(), node.name.as_str(), node.status, node.log))
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                STEP_BUILD,
                "Build",
                PipelineStatus::Success,
                Some(LogAvailability::Ready)
            ),
            (
                STEP_TEST,
                "Test",
                PipelineStatus::Failed,
                Some(LogAvailability::Ready)
            ),
            (
                STEP_DEPLOY,
                "Deploy",
                PipelineStatus::Running,
                Some(LogAvailability::Partial)
            ),
            (
                STEP_RELEASE,
                "Release",
                PipelineStatus::Pending,
                Some(LogAvailability::Pending)
            ),
            (STEP_SMOKE, "Smoke", PipelineStatus::Skipped, None),
        ]
    );
    let lines: Vec<String> = requests
        .lock()
        .await
        .iter()
        .map(|(line, _)| line.clone())
        .collect();
    let steps = steps_path();
    assert_eq!(
        &lines[1..],
        [
            format!("GET {steps}?pagelen=100 HTTP/1.1"),
            format!("GET {steps}?pagelen=100&page=2 HTTP/1.1"),
        ]
    );
}

#[tokio::test]
async fn step_log_streams_into_the_sink() {
    let (plugin, requests, _) = serve_pipeline_logs().await;

    let log = read_step(&plugin, STEP_BUILD).await.unwrap();

    assert_eq!(log, BUILD_LOG.as_bytes());
    let requests = requests.lock().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1],
        (
            format!("GET {} HTTP/1.1", log_path(STEP_BUILD)),
            Some("Basic stub".to_string())
        )
    );
}

#[tokio::test]
async fn step_log_redirect_is_followed_without_the_api_credentials() {
    let (plugin, requests, stored) = serve_pipeline_logs().await;

    let log = read_step(&plugin, STEP_TEST).await.unwrap();

    assert_eq!(log, TEST_LOG.as_bytes());
    let requests = requests.lock().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].1.as_deref(), Some("Basic stub"));
    assert_eq!(
        *stored.lock().await,
        [("GET /storage/test.log HTTP/1.1".to_string(), None)]
    );
}

#[tokio::test]
async fn step_log_that_is_gone_is_not_found() {
    let (plugin, _, _) = serve_pipeline_logs().await;

    for step in [STEP_PRUNED, STEP_EXPIRED] {
        let error = read_step(&plugin, step).await.unwrap_err();
        assert!(
            matches!(&error, PluginError::PipelineNotFound(message) if message.contains("no longer available")),
            "{step}: {error:?}"
        );
    }
}

#[tokio::test]
async fn step_ids_outside_the_braced_uuid_shape_are_rejected_without_a_request() {
    let (plugin, requests, stored) = serve_pipeline_logs().await;

    let rejected = [
        "",
        "{}",
        "1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61",
        "%7B1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61%7D",
        "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a6}",
        "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a611}",
        "{1b0e5c7a0d11-4f6e-9a57-3c1d2e4f5a61--}",
        "{1g0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}",
        "{../../../pipelines/%7Bx%7D/stopPipeline}",
        "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}/log",
        "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}?x=1",
        "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}\n",
        " {1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}",
    ];
    for node_id in rejected {
        let error = read_step(&plugin, node_id).await.unwrap_err();
        assert!(
            matches!(error, PluginError::InvalidConfig(_)),
            "{node_id:?}: {error:?}"
        );
    }

    assert!(requests.lock().await.is_empty());
    assert!(stored.lock().await.is_empty());
}

struct StepLog {
    body: Vec<u8>,
    honors_range: bool,
    requests: Vec<(Option<String>, Option<String>)>,
}

type SharedStepLog = Arc<std::sync::Mutex<StepLog>>;

async fn serve_ranged_storage(log: SharedStepLog) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut buffer = [0; 8192];
            let count = stream.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..count]);
            let header = |wanted: &str| {
                request
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
                    .map(|(_, value)| value.trim().to_string())
            };
            let range = header("range");
            let (status, content_range, payload) = {
                let mut log = log.lock().unwrap();
                log.requests.push((range.clone(), header("authorization")));
                let total = log.body.len();
                let start = range
                    .as_deref()
                    .and_then(|value| {
                        value
                            .strip_prefix("bytes=")?
                            .strip_suffix('-')?
                            .parse()
                            .ok()
                    })
                    .filter(|_| log.honors_range);
                match start {
                    Some(start) if start >= total => (
                        "416 Range Not Satisfiable",
                        format!("content-range: bytes */{total}\r\n"),
                        Vec::new(),
                    ),
                    Some(start) => (
                        "206 Partial Content",
                        format!("content-range: bytes {start}-{}/{total}\r\n", total - 1),
                        log.body[start..].to_vec(),
                    ),
                    None => ("200 OK", String::new(), log.body.clone()),
                }
            };
            let head = format!(
                "HTTP/1.1 {status}\r\n{content_range}content-length: {}\r\nconnection: close\r\n\r\n",
                payload.len()
            );
            stream.write_all(head.as_bytes()).await.unwrap();
            stream.write_all(&payload).await.unwrap();
        }
    });
    address
}

async fn serve_running_step(body: &[u8], honors_range: bool) -> (BitbucketPlugin, SharedStepLog) {
    let log = SharedStepLog::new(std::sync::Mutex::new(StepLog {
        body: body.to_vec(),
        honors_range,
        requests: Vec::new(),
    }));
    let storage = serve_ranged_storage(log.clone()).await;
    let (api, _) = stub(move |target, _| {
        let path = reqwest::Url::parse(&format!("http://api{target}"))
            .unwrap()
            .path()
            .to_string();
        if path == "/repositories/ws/repo/pipelines" {
            reply("200 OK", &pipelines_page())
        } else if path == log_path(STEP_DEPLOY) {
            Reply {
                status: "307 Temporary Redirect",
                headers: vec![("location", format!("http://{storage}/storage/deploy.log"))],
                body: String::new(),
            }
        } else {
            reply("404 Not Found", "{}")
        }
    })
    .await;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut plugin = BitbucketPlugin::new();
    plugin.client = Some(client::BitbucketClient::new(
        Arc::new(reqwest::Client::builder().no_proxy().build().unwrap()),
        format!("http://{api}"),
        "Basic stub".to_string(),
    ));
    (plugin, log)
}

fn cursor(step: &str, offset: u64) -> LogCursor {
    LogCursor::new(&serde_json::json!({"pipeline": PIPELINE_UUID, "step": step, "offset": offset}))
        .unwrap()
}

async fn read_running_step(
    plugin: &BitbucketPlugin, from: Option<&LogCursor>,
) -> (PluginResult<LogRead>, Vec<u8>, bool) {
    let metadata = HashMap::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut sink = LogSink::new(tx);
    let result = plugin
        .read_log(run_7(&metadata), STEP_DEPLOY, from, &mut sink)
        .await;
    let restarted = sink.restarted();
    drop(sink);
    let mut bytes = Vec::new();
    while let Some(chunk) = rx.recv().await {
        bytes.extend_from_slice(&chunk);
    }
    (result, bytes, restarted)
}

fn storage_requests(log: &SharedStepLog) -> Vec<(Option<String>, Option<String>)> {
    log.lock().unwrap().requests.clone()
}

#[tokio::test]
async fn resumed_step_log_appends_only_the_new_bytes_through_the_redirect() {
    let (plugin, log) = serve_running_step(b"a\nb\n", true).await;

    let (first, bytes, restarted) = read_running_step(&plugin, None).await;
    assert_eq!(
        first.unwrap(),
        LogRead::replace(Some(cursor(STEP_DEPLOY, 4)))
    );
    assert_eq!(bytes, b"a\nb\n");
    assert!(!restarted);

    log.lock().unwrap().body.extend_from_slice(b"c\n");
    let (second, bytes, restarted) =
        read_running_step(&plugin, Some(&cursor(STEP_DEPLOY, 4))).await;
    assert_eq!(second.unwrap(), LogRead::append(cursor(STEP_DEPLOY, 6)));
    assert_eq!(bytes, b"c\n");
    assert!(!restarted);

    let (idle, bytes, restarted) = read_running_step(&plugin, Some(&cursor(STEP_DEPLOY, 6))).await;
    assert_eq!(idle.unwrap(), LogRead::append(cursor(STEP_DEPLOY, 6)));
    assert!(bytes.is_empty() && !restarted);
    assert_eq!(
        storage_requests(&log),
        [
            (None, None),
            (Some("bytes=4-".to_string()), None),
            (Some("bytes=6-".to_string()), None)
        ]
    );
}

#[tokio::test]
async fn storage_ignoring_range_restarts_with_the_whole_step_log() {
    let (plugin, log) = serve_running_step(b"a\nb\nc\n", false).await;

    let (read, bytes, restarted) = read_running_step(&plugin, Some(&cursor(STEP_DEPLOY, 4))).await;

    assert_eq!(
        read.unwrap(),
        LogRead::replace(Some(cursor(STEP_DEPLOY, 6)))
    );
    assert_eq!(bytes, b"a\nb\nc\n");
    assert!(restarted);
    assert_eq!(
        storage_requests(&log),
        [(Some("bytes=4-".to_string()), None)]
    );
}

#[tokio::test]
async fn shrunk_step_log_restarts_with_the_whole_step_log() {
    let (plugin, log) = serve_running_step(b"a\n", true).await;

    let (read, bytes, restarted) = read_running_step(&plugin, Some(&cursor(STEP_DEPLOY, 6))).await;

    assert_eq!(
        read.unwrap(),
        LogRead::replace(Some(cursor(STEP_DEPLOY, 2)))
    );
    assert_eq!(bytes, b"a\n");
    assert!(restarted);
    assert_eq!(
        storage_requests(&log),
        [(Some("bytes=6-".to_string()), None), (None, None)]
    );
}

#[tokio::test]
async fn cursors_of_another_step_or_shape_restart_without_a_range_request() {
    let (plugin, log) = serve_running_step(b"a\nb\n", true).await;
    let other_pipeline = LogCursor::new(
        &serde_json::json!({"pipeline": "{other}", "step": STEP_DEPLOY, "offset": 4}),
    )
    .unwrap();

    for stale in [
        LogCursor::new(&"garbage").unwrap(),
        cursor(STEP_TEST, 4),
        other_pipeline,
    ] {
        let (read, bytes, restarted) = read_running_step(&plugin, Some(&stale)).await;
        assert_eq!(
            read.unwrap(),
            LogRead::replace(Some(cursor(STEP_DEPLOY, 4)))
        );
        assert_eq!(bytes, b"a\nb\n");
        assert!(restarted);
    }

    assert_eq!(
        storage_requests(&log),
        [(None, None), (None, None), (None, None)]
    );
}

#[tokio::test]
async fn commit_hooks_read_the_repository_through_the_cloud_api() {
    let (plugin, _requests) = serve_runs(1).await;
    let repo = RepoId::from_host_path("bitbucket.org", "ws/repo").unwrap();
    let commit = CommitSha::parse(STUB_COMMIT).unwrap();
    let parent = CommitSha::parse(STUB_PARENT).unwrap();

    let info = plugin.resolve_commit(&repo, "main").await.unwrap();
    let comparison = plugin
        .compare_commits(&repo, &parent, &commit)
        .await
        .unwrap();
    let changes = plugin.commit_changes(&repo, &commit).await.unwrap();

    assert_eq!(info.sha, commit);
    assert_eq!(info.parents, [parent]);
    assert_eq!(info.authored_at, info.committed_at);
    assert_eq!(info.authored_at.to_rfc3339(), "2026-10-01T08:00:00+00:00");
    assert_eq!(
        (comparison.ahead, comparison.behind, comparison.commits),
        (1, 0, vec![commit])
    );
    let patches: Vec<_> = changes
        .iter()
        .map(|file| (file.path.as_str(), file.patch.as_deref()))
        .collect();
    assert_eq!(
        patches,
        [
            (
                "deploy/values.yaml",
                Some("@@ -1 +1 @@\n-tag: a\n+tag: b\n")
            ),
            ("old.txt", Some("@@ -1 +0,0 @@\n-gone\n")),
            ("logo.png", None),
        ]
    );
}

#[tokio::test]
async fn commit_hooks_reject_repositories_outside_bitbucket_cloud() {
    let (plugin, _requests) = serve_runs(1).await;
    let sha = CommitSha::parse(STUB_COMMIT).unwrap();

    for repo in [
        RepoId::from_host_path("git.example.test", "ws/repo").unwrap(),
        RepoId::from_host_path("bitbucket.org", "ws/repo/extra").unwrap(),
    ] {
        let error = plugin.commit_changes(&repo, &sha).await.unwrap_err();
        assert!(matches!(error, PluginError::InvalidConfig(_)), "{error}");
    }
}

#[tokio::test]
async fn hooks_on_an_uninitialized_plugin_report_it_instead_of_unsupported() {
    let plugin = BitbucketPlugin::new();
    let repo = RepoId::from_host_path("bitbucket.org", "ws/repo").unwrap();
    let sha = CommitSha::parse(STUB_COMMIT).unwrap();

    let errors = [
        plugin.resolve_commit(&repo, "main").await.unwrap_err(),
        plugin.compare_commits(&repo, &sha, &sha).await.unwrap_err(),
        plugin.commit_changes(&repo, &sha).await.unwrap_err(),
    ];

    for error in errors {
        assert!(matches!(error, PluginError::Internal(_)), "{error}");
    }
}

#[test]
fn source_hosts_are_empty_without_configuration_and_bitbucket_cloud_otherwise() {
    let plugin = BitbucketPlugin::new();
    let configured = HashMap::from([("email".to_string(), "a@b.test".to_string())]);

    assert!(plugin.source_hosts(&HashMap::new()).is_empty());
    assert_eq!(plugin.source_hosts(&configured), ["bitbucket.org"]);
}
