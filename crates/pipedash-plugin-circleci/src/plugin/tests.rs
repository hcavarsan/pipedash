use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};

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
const WEB: &str = "gh/acme/web";
const APP: &str = "circleci/org1/proj1";

struct Recorded {
    method: String,
    path: String,
    query: String,
    headers: HashMap<String, String>,
    body: Value,
}

type Requests = Arc<Mutex<Vec<Recorded>>>;

struct Stub {
    plugin: CircleCIPlugin,
    requests: Requests,
    growth: Arc<AtomicUsize>,
}

fn cfg(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn project(slug: &str, id: &str, name: &str, branch: &str) -> Value {
    json!({"slug": slug, "id": id, "name": name, "organization_name": "acme",
        "vcs_info": {"vcs_url": format!("https://github.com/acme/{name}"), "default_branch": branch}})
}

fn pipeline(number: i64) -> Value {
    json!({"id": format!("pipe-{number}"), "number": number, "state": "created",
        "created_at": "2026-11-01T10:00:00Z", "updated_at": "2026-11-01T10:10:00Z",
        "vcs": {"branch": "main", "revision": "abc123", "commit": {"subject": "Ship it"}},
        "trigger": {"actor": {"login": "ada"}}})
}

fn workflow(id: &str, name: &str, status: &str, minute: u32) -> Value {
    json!({"id": id, "name": name, "status": status,
        "created_at": format!("2026-11-01T10:{minute:02}:00Z"),
        "stopped_at": format!("2026-11-01T10:{:02}:00Z", minute + 3)})
}

fn page(items: Value) -> Value {
    json!({"items": items, "next_page_token": null})
}

fn workflows(pipeline_id: &str) -> Option<Value> {
    Some(match pipeline_id {
        "pipe-42" => json!([
            workflow("wf-build-1", "build", "failed", 0),
            workflow("wf-build-2", "build", "running", 5),
            workflow("wf-deploy", "deploy", "on_hold", 1),
        ]),
        "pipe-41" => json!([
            workflow("wf-41-build", "build", "failed", 0),
            workflow("wf-41-lint", "lint", "success", 0),
        ]),
        "pipe-40" | "pipe-39" | "pipe-38" => {
            let status = if pipeline_id == "pipe-40" {
                "success"
            } else {
                "failed"
            };
            let mut setup = workflow("wf-setup", "configure", status, 0);
            setup["tag"] = json!("setup");
            let mut items = vec![setup];
            if pipeline_id != "pipe-38" {
                items.push(workflow("wf-continued", "setup", status, 5));
            }
            json!(items)
        }
        _ => return None,
    })
}

fn action(index: i64, status: &str, output: Option<String>) -> Value {
    json!({"index": index, "status": status, "failed": status == "failed",
        "output_url": output, "start_time": "2026-11-01T10:05:00Z"})
}

fn steps(server: &str, growth: usize) -> Value {
    let out = |name: &str| Some(format!("{server}/out/{name}?X-Amz-Signature=sig"));
    let second = if growth > 0 {
        action(1, "failed", out("t1"))
    } else {
        action(1, "running", None)
    };
    json!({"steps": [
        {"name": "Spin up environment", "actions": [action(0, "success", out("spin"))]},
        {"name": "Run tests", "actions": [action(0, "success", out("t0")), second]}
    ]})
}

fn output(name: &str) -> Value {
    let message = match name {
        "spin" => "Starting container\r\n",
        "t0" => "ok 1\n",
        _ => "not ok 2\nExited with code exit status 1\n",
    };
    json!([{"message": message, "time": "2026-11-01T10:05:01Z", "type": "out"}])
}

fn reply(request: &Recorded, server: &str, growth: usize) -> (u16, Value) {
    if let Some(name) = request.path.strip_prefix("/out/") {
        return (200, output(name));
    }
    if request.headers.get("circle-token").map(String::as_str) != Some(TOKEN) {
        return (401, json!({"message": "You must log in first."}));
    }
    let path = request.path.as_str();
    match (request.method.as_str(), path) {
        ("GET", "/api/v2/me") => (200, json!({"login": "ada"})),
        ("GET", "/api/v1.1/projects") => (
            200,
            json!([
                {"vcs_type": "github", "username": "acme", "reponame": "web",
                    "vcs_url": "https://github.com/acme/web", "default_branch": "main"},
                {"vcs_type": "bitbucket", "username": "acme", "reponame": "mobile",
                    "vcs_url": "https://bitbucket.org/acme/mobile", "default_branch": "main"},
                {"vcs_type": "github", "username": "tools", "reponame": "cli",
                    "vcs_url": "https://github.com/tools/cli", "default_branch": "main"}
            ]),
        ),
        ("GET", "/api/v2/project/gh/acme/web") => {
            (200, project("gh/acme/web", "p-web", "web", "main"))
        }
        ("GET", "/api/v2/project/gh/acme/api") => {
            (200, project("gh/acme/api", "p-api", "api", "trunk"))
        }
        ("GET", "/api/v2/project/circleci/org1/proj1") => (
            200,
            project("circleci/org1/proj1", "p-app", "app", "develop"),
        ),
        ("GET", "/api/v2/project/gh/acme/web/pipeline") => {
            (200, page(json!([pipeline(42), pipeline(41)])))
        }
        ("GET", "/api/v2/project/gh/acme/web/pipeline/42") => (200, pipeline(42)),
        ("GET", "/api/v2/project/gh/acme/web/pipeline/41") => (200, pipeline(41)),
        ("GET", "/api/v2/project/gh/acme/web/pipeline/40") => (200, pipeline(40)),
        ("GET", "/api/v2/project/gh/acme/web/pipeline/39") => (200, pipeline(39)),
        ("GET", "/api/v2/project/gh/acme/web/pipeline/38") => (200, pipeline(38)),
        ("GET", "/api/v2/project/gh/acme/api/pipeline") => (200, page(json!([]))),
        ("GET", "/api/v2/project/circleci/org1/proj1/pipeline") => (200, page(json!([]))),
        ("GET", "/api/v2/pipeline/pipe-42/config") => (
            200,
            json!({"source": "version: 2.1\nparameters:\n  deploy:\n    type: boolean\n    default: false\njobs: {}\n"}),
        ),
        ("GET", "/api/v2/projects/p-app/pipeline-definitions") => (
            200,
            page(json!([{"id": "def-a", "name": "build"}, {"id": "def-b", "name": "release"}])),
        ),
        ("GET", "/api/v2/workflow/wf-build-2/job") => (
            200,
            page(json!([
                {"id": "j1", "name": "checkout", "job_number": 101, "status": "success",
                    "type": "build", "started_at": "2026-11-01T10:05:00Z"},
                {"id": "j2", "name": "test", "job_number": 102, "status": "running",
                    "type": "build", "started_at": "2026-11-01T10:06:00Z"}
            ])),
        ),
        ("GET", "/api/v2/workflow/wf-deploy/job") => (
            200,
            page(json!([{"id": "j3", "name": "approve", "status": "on_hold", "type": "approval"}])),
        ),
        ("GET", "/api/v1.1/project/github/acme/web/102") => (200, steps(server, growth)),
        ("POST", "/api/v2/project/gh/acme/web/pipeline") => (
            201,
            json!({"id": "pipe-43", "number": 43, "state": "created"}),
        ),
        ("POST", "/api/v2/project/circleci/org1/proj1/pipeline/run") => (
            201,
            json!({"id": "pipe-7", "number": 7, "state": "created"}),
        ),
        ("POST", path) if path.ends_with("/cancel") => (202, json!({"message": "Accepted."})),
        ("POST", path) if path.ends_with("/rerun") => (202, json!({"workflow_id": "new"})),
        ("GET", path) if path.starts_with("/api/v2/pipeline/") => {
            let id = path
                .trim_start_matches("/api/v2/pipeline/")
                .trim_end_matches("/workflow");
            match workflows(id) {
                Some(items) => (200, page(items)),
                None => (503, json!({"message": "unavailable"})),
            }
        }
        ("GET", path) if path.starts_with("/api/v2/project/gh/acme/broken") => {
            (503, json!({"message": "unavailable"}))
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
    let (head_end, length) = loop {
        let count = stream.read(&mut buffer).await.ok()?;
        if count == 0 {
            return None;
        }
        raw.extend_from_slice(&buffer[..count]);
        if let Some(position) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..position]).to_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse().ok())
                .unwrap_or(0);
            break (position + 4, length);
        }
    };
    while raw.len() < head_end + length {
        let count = stream.read(&mut buffer).await.ok()?;
        if count == 0 {
            return None;
        }
        raw.extend_from_slice(&buffer[..count]);
    }
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
        body: serde_json::from_slice(&raw[head_end..head_end + length]).unwrap_or(Value::Null),
    })
}

async fn stub(extra: &[(&str, &str)]) -> Stub {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Requests::default();
    let growth = Arc::new(AtomicUsize::new(0));
    let (recorded, grown, server) = (requests.clone(), growth.clone(), server_url.clone());
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let (recorded, grown, server) = (recorded.clone(), grown.clone(), server.clone());
            tokio::spawn(async move {
                let Some(request) = read_request(&mut stream).await else {
                    return;
                };
                let (status, body) = reply(&request, &server, grown.load(Ordering::SeqCst));
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

    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut pairs = vec![("base_url", server_url.as_str()), ("token", TOKEN)];
    pairs.extend_from_slice(extra);
    let mut plugin = CircleCIPlugin::new();
    plugin
        .initialize(1, cfg(&pairs), Some(Arc::new(reqwest::Client::new())))
        .unwrap();
    Stub {
        plugin,
        requests,
        growth,
    }
}

async fn calls(requests: &Requests, method: &str) -> Vec<String> {
    requests
        .lock()
        .await
        .iter()
        .filter(|request| request.method == method)
        .map(|request| match request.query.as_str() {
            "" => request.path.clone(),
            query => format!("{}?{query}", request.path),
        })
        .collect()
}

async fn bodies(requests: &Requests, method: &str) -> Vec<Value> {
    requests
        .lock()
        .await
        .iter()
        .filter(|request| request.method == method)
        .map(|request| request.body.clone())
        .collect()
}

fn run_ref<'a>(metadata: &'a HashMap<String, Value>, run_number: i64) -> RunRef<'a> {
    RunRef {
        pipeline_id: WEB,
        run_number,
        run_id: "gh/acme/web__pipe-42",
        metadata,
    }
}

async fn read(
    plugin: &CircleCIPlugin, node_id: &str, from: Option<&LogCursor>,
) -> PluginResult<(String, LogRead, bool)> {
    let metadata = HashMap::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let read = async {
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(run_ref(&metadata, 42), node_id, from, &mut sink)
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

fn trigger(workflow_id: &str, inputs: Value) -> TriggerParams {
    TriggerParams {
        workflow_id: workflow_id.to_string(),
        inputs: Some(inputs),
    }
}

#[tokio::test]
async fn selected_projects_show_the_status_of_their_newest_pipeline() {
    let stub = stub(&[("selected_items", "gh/acme/web, gh/acme/api")]).await;

    let pipelines = stub.plugin.fetch_pipelines().await.unwrap();

    let summary: Vec<_> = pipelines
        .iter()
        .map(|p| {
            (
                p.id.as_str(),
                p.repository.as_str(),
                p.status,
                p.branch.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (WEB, "acme/web", PipelineStatus::Running, Some("main")),
            (
                "gh/acme/api",
                "acme/api",
                PipelineStatus::Pending,
                Some("trunk")
            ),
        ]
    );
    assert_eq!(pipelines[1].last_run, None);
    let gets = calls(&stub.requests, "GET").await;
    let position = |path: &str| gets.iter().position(|get| get == path).unwrap();
    let mut sorted = gets.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        [
            "/api/v2/pipeline/pipe-42/workflow",
            "/api/v2/project/gh/acme/api",
            "/api/v2/project/gh/acme/api/pipeline",
            "/api/v2/project/gh/acme/web",
            "/api/v2/project/gh/acme/web/pipeline",
        ]
    );
    assert!(
        position("/api/v2/project/gh/acme/web/pipeline")
            < position("/api/v2/pipeline/pipe-42/workflow")
    );
}

#[tokio::test]
async fn an_empty_selection_reads_nothing_and_one_failing_project_fails_the_fetch() {
    let stub = stub(&[("selected_items", "")]).await;
    assert!(stub.plugin.fetch_pipelines().await.unwrap().is_empty());
    assert!(stub.requests.lock().await.is_empty());

    let stub = self::stub(&[("selected_items", "gh/acme/web,gh/acme/broken")]).await;
    let error = stub.plugin.fetch_pipelines().await.unwrap_err();
    assert!(
        matches!(error, PluginError::ServiceUnavailable { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_deleted_project_keeps_the_other_projects_pipelines() {
    let stub = stub(&[("selected_items", "gh/acme/web,gh/acme/gone")]).await;
    let pipelines = stub.plugin.fetch_pipelines().await.unwrap();
    assert_eq!(
        pipelines.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        [WEB]
    );

    let stub = self::stub(&[("selected_items", "gh/acme/gone")]).await;
    let only_deleted = stub.plugin.fetch_pipelines().await;
    assert!(
        matches!(only_deleted, Err(PluginError::PipelineNotFound(_))),
        "{only_deleted:?}"
    );
}

#[tokio::test]
async fn discovery_groups_followed_projects_by_organization() {
    let stub = stub(&[]).await;

    let organizations: Vec<String> = stub
        .plugin
        .fetch_organizations()
        .await
        .unwrap()
        .into_iter()
        .map(|organization| organization.id)
        .collect();
    assert_eq!(organizations, ["acme", "tools"]);

    let acme: Vec<String> = stub
        .plugin
        .fetch_available_pipelines_filtered(Some("acme".into()), None, None)
        .await
        .unwrap()
        .items
        .into_iter()
        .map(|pipeline| pipeline.id)
        .collect();
    assert_eq!(acme, ["gh/acme/web", "bb/acme/mobile"]);
}

#[tokio::test]
async fn history_runs_take_their_status_from_the_latest_attempt_of_each_workflow() {
    let stub = stub(&[]).await;

    let runs = stub.plugin.fetch_run_history(WEB, 2).await.unwrap();

    let summary: Vec<_> = runs
        .iter()
        .map(|run| (run.run_number, run.status))
        .collect();
    assert_eq!(
        summary,
        [(42, PipelineStatus::Running), (41, PipelineStatus::Failed)]
    );
    assert_eq!(runs[1].metadata["can_rerun_failed_jobs"], json!(true));
    assert_eq!(
        runs[0].logs_url,
        format!("{}/pipelines/github/acme/web/42", stub.plugin.base_url)
    );
}

#[tokio::test]
async fn log_nodes_group_jobs_under_their_workflows() {
    let stub = stub(&[]).await;
    let metadata = HashMap::from([("circleci_pipeline_id".to_string(), json!("pipe-42"))]);

    let nodes = stub
        .plugin
        .fetch_log_nodes(run_ref(&metadata, 42))
        .await
        .unwrap();

    let summary: Vec<_> = nodes
        .iter()
        .map(|node| (node.id.as_str(), node.parent_id.as_deref(), node.log))
        .collect();
    assert_eq!(
        summary,
        [
            ("workflow:wf-build-2", None, None),
            (
                "101",
                Some("workflow:wf-build-2"),
                Some(LogAvailability::Ready)
            ),
            (
                "102",
                Some("workflow:wf-build-2"),
                Some(LogAvailability::Partial)
            ),
            ("workflow:wf-deploy", None, None),
            ("job:j3", Some("workflow:wf-deploy"), None),
        ]
    );
    assert!(!calls(&stub.requests, "GET")
        .await
        .iter()
        .any(|path| path.contains("/pipeline/42")));
}

#[tokio::test]
async fn job_logs_render_steps_as_sections_and_append_finished_steps() {
    let stub = stub(&[]).await;
    let first = "#2026-11-01T10:05:00Z Spin up environment\n.2026-11-01T10:05:01Z Starting container\n#2026-11-01T10:05:00Z Run tests (container 0)\n.2026-11-01T10:05:01Z ok 1\n#2026-11-01T10:05:00Z Run tests (container 1)\n";

    let (output, read_1, restarted) = read(&stub.plugin, "102", None).await.unwrap();
    assert_eq!(output, first);
    assert_eq!(read_1.mode, LogReadMode::Replace);
    assert!(!restarted);
    let outputs: Vec<_> = stub
        .requests
        .lock()
        .await
        .iter()
        .filter(|request| request.path.starts_with("/out/"))
        .map(|request| request.headers.contains_key("circle-token"))
        .collect();
    assert_eq!(outputs, [false, false]);

    stub.growth.store(1, Ordering::SeqCst);
    let (output, read_2, restarted) = read(&stub.plugin, "102", read_1.cursor.as_ref())
        .await
        .unwrap();
    assert_eq!(read_2.mode, LogReadMode::Replace);
    assert!(restarted);
    assert!(output.ends_with(
        "+2026-11-01T10:05:00Z Run tests (container 1)\n.2026-11-01T10:05:01Z not ok 2\n.2026-11-01T10:05:01Z Exited with code exit status 1\n"
    ));

    let (output, read_3, restarted) = read(&stub.plugin, "102", read_2.cursor.as_ref())
        .await
        .unwrap();
    assert_eq!(
        (output.as_str(), read_3.mode, restarted),
        ("", LogReadMode::Append, false)
    );

    assert!(matches!(
        read(&stub.plugin, "workflow:wf-build-2", None).await,
        Err(PluginError::InvalidConfig(_))
    ));
}

#[tokio::test]
async fn job_output_past_the_budget_is_left_out_in_step_order() {
    let mut stub = stub(&[]).await;
    stub.plugin.job_output_limit = "Starting container\r\n".len();

    let (output, _, _) = read(&stub.plugin, "102", None).await.unwrap();

    let omitted = format!(".- {}\n", log_dialect::OMITTED);
    assert_eq!(
        output,
        format!(
            "#2026-11-01T10:05:00Z Spin up environment\n.2026-11-01T10:05:01Z Starting container\n#2026-11-01T10:05:00Z Run tests (container 0)\n{omitted}#2026-11-01T10:05:00Z Run tests (container 1)\n{omitted}"
        )
    );
}

#[tokio::test]
async fn trigger_parameters_list_the_branch_and_the_declared_pipeline_parameters() {
    let stub = stub(&[]).await;

    let parameters = stub.plugin.fetch_workflow_parameters(WEB).await.unwrap();
    let names: Vec<_> = parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["branch", "parameters.deploy"]);
    assert!(matches!(
        &parameters[0].param_type,
        WorkflowParameterType::String { default: Some(branch) } if branch == "main"
    ));

    let app = stub.plugin.fetch_workflow_parameters(APP).await.unwrap();
    let names: Vec<_> = app.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["branch", "pipeline"]);
}

#[tokio::test]
async fn triggering_posts_once_with_the_branch_and_typed_parameters() {
    let stub = stub(&[]).await;

    let result = stub
        .plugin
        .trigger_pipeline(trigger(
            WEB,
            json!({"branch": "release", "parameters.deploy": true, "parameters.count": 3.0}),
        ))
        .await
        .unwrap();

    assert_eq!(
        result,
        TriggerResult {
            run_id: None,
            run_number: Some(43),
        }
    );
    assert_eq!(
        calls(&stub.requests, "POST").await,
        ["/api/v2/project/gh/acme/web/pipeline"]
    );
    assert_eq!(
        bodies(&stub.requests, "POST").await,
        [json!({"branch": "release", "parameters": {"deploy": true, "count": 3}})]
    );
    assert!(calls(&stub.requests, "GET").await.is_empty());
}

#[tokio::test]
async fn triggering_preserves_cleared_string_defaults_and_omits_absent_inputs() {
    let stub = stub(&[]).await;

    for inputs in [
        json!({"branch": "main", "parameters.message": "custom"}),
        json!({"branch": "main", "parameters.message": ""}),
        json!({"branch": "main"}),
    ] {
        stub.plugin
            .trigger_pipeline(trigger(WEB, inputs))
            .await
            .unwrap();
    }

    assert_eq!(
        bodies(&stub.requests, "POST").await,
        [
            json!({"branch": "main", "parameters": {"message": "custom"}}),
            json!({"branch": "main", "parameters": {"message": ""}}),
            json!({"branch": "main"}),
        ]
    );
}

#[tokio::test]
async fn app_projects_run_the_chosen_pipeline_definition() {
    let stub = stub(&[]).await;

    let error = stub
        .plugin
        .trigger_pipeline(trigger(APP, json!({})))
        .await
        .unwrap_err();
    assert!(matches!(error, PluginError::InvalidConfig(_)), "{error:?}");
    assert!(calls(&stub.requests, "POST").await.is_empty());

    stub.plugin
        .trigger_pipeline(trigger(APP, json!({"pipeline": "release"})))
        .await
        .unwrap();
    assert_eq!(
        calls(&stub.requests, "POST").await,
        ["/api/v2/project/circleci/org1/proj1/pipeline/run"]
    );
    assert_eq!(
        bodies(&stub.requests, "POST").await,
        [
            json!({"definition_id": "def-b", "config": {"branch": "develop"},
            "checkout": {"branch": "develop"}})
        ]
    );
}

#[tokio::test]
async fn cancel_stops_only_the_active_latest_workflows() {
    let stub = stub(&[]).await;
    stub.plugin.cancel_run(WEB, 42).await.unwrap();
    assert_eq!(
        calls(&stub.requests, "POST").await,
        [
            "/api/v2/workflow/wf-build-2/cancel",
            "/api/v2/workflow/wf-deploy/cancel"
        ]
    );

    let stub = self::stub(&[]).await;
    assert!(stub.plugin.cancel_run(WEB, 41).await.is_err());
    assert!(calls(&stub.requests, "POST").await.is_empty());
}

#[tokio::test]
async fn reruns_target_finished_workflows_and_failed_only_reruns_skip_passing_ones() {
    let stub = stub(&[]).await;
    stub.plugin.rerun_failed_jobs(WEB, 41).await.unwrap();
    stub.plugin.rerun_run(WEB, 41).await.unwrap();
    assert_eq!(
        calls(&stub.requests, "POST").await,
        [
            "/api/v2/workflow/wf-41-build/rerun",
            "/api/v2/workflow/wf-41-build/rerun",
            "/api/v2/workflow/wf-41-lint/rerun"
        ]
    );
    assert_eq!(
        bodies(&stub.requests, "POST").await,
        [
            json!({"from_failed": true}),
            json!({"from_failed": false}),
            json!({"from_failed": false})
        ]
    );

    let stub = self::stub(&[]).await;
    assert!(stub.plugin.rerun_run(WEB, 42).await.is_err());
    assert!(calls(&stub.requests, "POST").await.is_empty());
}

#[tokio::test]
async fn continued_pipeline_reruns_skip_tagged_setup_workflows() {
    for number in [40, 39] {
        let stub = stub(&[]).await;

        stub.plugin.rerun_run(WEB, number).await.unwrap();

        assert_eq!(
            calls(&stub.requests, "POST").await,
            ["/api/v2/workflow/wf-continued/rerun"]
        );
        assert_eq!(
            bodies(&stub.requests, "POST").await,
            [json!({"from_failed": false})]
        );
    }
}

#[tokio::test]
async fn failed_only_reruns_skip_setup_after_continuation() {
    let stub = stub(&[]).await;

    stub.plugin.rerun_failed_jobs(WEB, 39).await.unwrap();

    assert_eq!(
        calls(&stub.requests, "POST").await,
        ["/api/v2/workflow/wf-continued/rerun"]
    );
    assert_eq!(
        bodies(&stub.requests, "POST").await,
        [json!({"from_failed": true})]
    );
}

#[tokio::test]
async fn setup_can_rerun_before_pipeline_continuation() {
    for failed_only in [false, true] {
        let stub = stub(&[]).await;

        stub.plugin.rerun(WEB, 38, failed_only).await.unwrap();

        assert_eq!(
            calls(&stub.requests, "POST").await,
            ["/api/v2/workflow/wf-setup/rerun"]
        );
        assert_eq!(
            bodies(&stub.requests, "POST").await,
            [json!({"from_failed": failed_only})]
        );
    }
}

#[test]
fn initialize_requires_a_token_and_a_plain_base_url() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let client = || Some(Arc::new(reqwest::Client::new()));
    let mut plugin = CircleCIPlugin::new();
    assert!(matches!(
        plugin.initialize(1, cfg(&[]), client()),
        Err(PluginError::InvalidConfig(message)) if message == "Missing token in config"
    ));
    assert!(plugin
        .initialize(
            1,
            cfg(&[
                ("token", "t"),
                ("base_url", "https://circleci.acme.dev/api?x=1")
            ]),
            client()
        )
        .is_err());
    plugin
        .initialize(1, cfg(&[("token", "t")]), client())
        .unwrap();
    assert_eq!(plugin.base_url, config::DEFAULT_BASE_URL);
}

#[test]
fn pipeline_actions_should_rerun_in_place_and_hide_configured_repositories() {
    let plugin = CircleCIPlugin::new();
    let metadata = HashMap::new();
    let facts = PipelineFacts {
        id: "gh/a/b",
        status: PipelineStatus::Success,
        branch: None,
        workflow_file: None,
        resource_state: None,
        metadata: &metadata,
    };
    assert_eq!(
        plugin.pipeline_actions(&facts).rerun_strategy,
        RerunStrategy::InPlace
    );
    let config = HashMap::from([("selected_items".to_string(), "a".to_string())]);
    assert!(plugin.configured_repositories(&config).is_empty());
}
