use std::io::{
    BufRead,
    BufReader,
    Write,
};
use std::net::TcpListener;
use std::sync::mpsc;

use super::*;

const TEMPLATES: &str = r#"{"items":[{"metadata":{"name":"build","namespace":"ci"}},{"metadata":{"name":"release","namespace":"ops"}}]}"#;
const CRONS: &str = r#"{"items":[{"metadata":{"name":"nightly","namespace":"ci"},"spec":{"schedule":"0 2 * * *","suspend":true}}]}"#;
const BUILD_RUNS: &str = r#"{"items":[
    {"metadata":{"name":"build-old","namespace":"ci","uid":"00000001-0","creationTimestamp":"2026-11-19T10:00:00Z"},"status":{"phase":"Failed","startedAt":"2026-11-19T10:00:00Z","finishedAt":"2026-11-19T10:01:00Z"}},
    {"metadata":{"name":"build-new","namespace":"ci","uid":"00000002-0","creationTimestamp":"2026-11-21T10:00:00Z"},"status":{"phase":"Running","startedAt":"2026-11-21T10:00:00Z"}}
]}"#;

fn respond(target: &str) -> (u16, &'static str) {
    let path = target.split('?').next().unwrap_or_default();
    match path {
        "/apis/argoproj.io/v1alpha1/workflowtemplates"
        | "/apis/argoproj.io/v1alpha1/namespaces/ci/workflowtemplates" => (200, TEMPLATES),
        "/apis/argoproj.io/v1alpha1/cronworkflows"
        | "/apis/argoproj.io/v1alpha1/namespaces/ci/cronworkflows" => (200, CRONS),
        "/apis/argoproj.io/v1alpha1/namespaces/ci/workflows"
            if target.ends_with("workflow-template%3Dbuild") =>
        {
            (200, BUILD_RUNS)
        }
        "/apis/argoproj.io/v1alpha1/namespaces/ci/workflows" => (200, r#"{"items":[]}"#),
        _ => (404, r#"{"kind":"Status","code":404,"message":"not found"}"#),
    }
}

fn stub_apiserver() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (log, requests) = mpsc::channel();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut lines = BufReader::new(&stream).lines().map_while(Result::ok);
            let request_line = lines.next().unwrap_or_default();
            lines.take_while(|line| !line.is_empty()).for_each(drop);
            let target = request_line
                .split(' ')
                .nth(1)
                .unwrap_or_default()
                .to_string();
            let (status, body) = respond(&target);
            let _ = log.send(target);
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Stub\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (format!("http://{addr}"), requests)
}

fn plugin(server: &str, dir: &tempfile::TempDir, extra: &[(&str, &str)]) -> ArgoWorkflowsPlugin {
    let kubeconfig = dir.path().join("kubeconfig");
    std::fs::write(
        &kubeconfig,
        format!(
            "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster:\n    server: {server}\ncontexts:\n- name: ctx\n  context:\n    cluster: c\n    user: u\ncurrent-context: ctx\nusers:\n- name: u\n  user:\n    token: t\n"
        ),
    )
    .unwrap();
    let mut config = HashMap::from([(
        config::KUBECONFIG_PATH_KEY.to_string(),
        kubeconfig.to_string_lossy().to_string(),
    )]);
    config.extend(
        extra
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string())),
    );
    let mut plugin = ArgoWorkflowsPlugin::new();
    plugin.initialize(1, config, None).unwrap();
    plugin
}

#[tokio::test]
async fn available_pipelines_list_templates_and_cron_workflows_from_every_namespace() {
    let (server, _) = stub_apiserver();
    let dir = tempfile::tempdir().unwrap();
    let plugin = plugin(&server, &dir, &[]);

    let available = plugin
        .fetch_available_pipelines_filtered(None, None, None)
        .await
        .unwrap();
    let mut ids: Vec<&str> = available
        .items
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    ids.sort();

    assert_eq!(
        ids,
        [
            "ci__cron__nightly",
            "ci__template__build",
            "ops__template__release"
        ]
    );
    assert!(plugin.validate_credentials().await.unwrap());
}

#[tokio::test]
async fn selected_pipelines_read_only_their_namespaces_and_latest_workflow() {
    let (server, requests) = stub_apiserver();
    let dir = tempfile::tempdir().unwrap();
    let plugin = plugin(
        &server,
        &dir,
        &[("selected_items", "ci__template__build,ci__cron__nightly")],
    );

    let mut pipelines = plugin.fetch_pipelines().await.unwrap();
    pipelines.sort_by(|a, b| a.id.cmp(&b.id));

    assert_eq!(pipelines.len(), 2);
    assert_eq!(pipelines[0].id, "ci__cron__nightly");
    assert_eq!(pipelines[0].status, PipelineStatus::Pending);
    assert_eq!(
        pipelines[0].resource_state.as_ref().unwrap().label,
        "Suspended"
    );
    assert_eq!(pipelines[1].id, "ci__template__build");
    assert_eq!(pipelines[1].status, PipelineStatus::Running);
    let targets: Vec<String> = requests.try_iter().collect();
    assert!(
        targets
            .iter()
            .all(|target| target.contains("/namespaces/ci/")),
        "{targets:?}"
    );
    assert!(targets
        .iter()
        .any(|target| target.ends_with("cron-workflow%3Dnightly")));
}

#[tokio::test]
async fn run_history_is_newest_first_and_details_find_a_run_by_number() {
    let (server, _) = stub_apiserver();
    let dir = tempfile::tempdir().unwrap();
    let plugin = plugin(&server, &dir, &[]);

    let history = plugin
        .fetch_run_history("ci__template__build", 10)
        .await
        .unwrap();
    let names: Vec<&serde_json::Value> = history
        .iter()
        .map(|run| &run.metadata["workflow"])
        .collect();
    assert_eq!(names, ["build-new", "build-old"]);
    assert_eq!(
        plugin
            .fetch_run_history("ci__template__build", 1)
            .await
            .unwrap()
            .len(),
        1
    );

    let details = plugin
        .fetch_run_details("ci__template__build", history[1].run_number)
        .await
        .unwrap();
    assert_eq!(details.status, PipelineStatus::Failed);
    assert_eq!(details.duration_seconds, Some(60));
    assert!(matches!(
        plugin.fetch_run_details("ci__template__build", 1).await,
        Err(PluginError::PipelineNotFound(_))
    ));

    let facts = RunFacts {
        run_number: details.run_number,
        status: details.status,
        branch: None,
        actor: None,
        inputs: None,
        metadata: &details.metadata,
    };
    let pipeline = PipelineFacts {
        id: "ci__template__build",
        status: details.status,
        branch: None,
        workflow_file: None,
        resource_state: None,
        metadata: &HashMap::new(),
    };
    assert_eq!(plugin.run_display(&pipeline, &facts).title, "build-old");
}

#[tokio::test]
async fn custom_namespaces_are_read_one_by_one() {
    let (server, requests) = stub_apiserver();
    let dir = tempfile::tempdir().unwrap();
    let plugin = plugin(&server, &dir, &[(config::NAMESPACES_KEY, "ci")]);

    plugin.validate_credentials().await.unwrap();

    let mut paths: Vec<String> = requests
        .try_iter()
        .map(|target| target.split('?').next().unwrap_or_default().to_string())
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            "/apis/argoproj.io/v1alpha1/namespaces/ci/cronworkflows",
            "/apis/argoproj.io/v1alpha1/namespaces/ci/workflowtemplates",
        ]
    );
}
