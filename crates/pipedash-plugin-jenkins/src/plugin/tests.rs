use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::test_support::{
    stub_server,
    stub_server_with_head,
    Reply,
};

fn mixed(path: &str) -> Reply {
    if path.contains("/job/limited/") {
        Reply {
            status: "429 Too Many Requests",
            headers: "Retry-After: 120\r\n",
            body: "slow down",
        }
    } else if path.contains("/job/missing/") {
        Reply {
            status: "404 Not Found",
            headers: "",
            body: "not found",
        }
    } else {
        Reply {
            status: "200 OK",
            headers: "Content-Type: application/json\r\n",
            body: r#"{"name":"ok"}"#,
        }
    }
}

async fn initialized(selected: &str, extra: &[(&str, &str)]) -> PluginResult<JenkinsPlugin> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let server_url = stub_server(Arc::new(AtomicUsize::new(0)), Duration::ZERO, mixed).await;
    let mut config = HashMap::from([
        ("server_url".to_string(), server_url),
        ("username".to_string(), "alice".to_string()),
        ("token".to_string(), "token".to_string()),
        ("selected_items".to_string(), selected.to_string()),
    ]);
    config.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    let mut plugin = JenkinsPlugin::new();
    plugin.initialize(1, config, Some(Arc::new(reqwest::Client::new())))?;
    Ok(plugin)
}

#[tokio::test]
async fn transient_failure_on_any_job_fails_the_fetch() {
    let plugin = initialized("ok,limited", &[]).await.unwrap();

    let error = plugin.fetch_pipelines().await.unwrap_err();

    assert!(
        matches!(error, PluginError::RateLimited { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn permanent_failure_on_one_job_keeps_the_rest() {
    let plugin = initialized("ok,missing", &[]).await.unwrap();

    let pipelines = plugin.fetch_pipelines().await.unwrap();

    assert_eq!(pipelines.len(), 1);
}

#[tokio::test]
async fn status_source_follows_the_optional_status_page_url() {
    let none = initialized("ok", &[]).await.unwrap();
    assert!(none.status_source().is_none());

    let custom = initialized("ok", &[("status_page_url", "https://status.example.com")])
        .await
        .unwrap();
    let source = custom.status_source().unwrap();
    assert_eq!(source.name, "status.example.com");
    assert_eq!(
        source.feed_url.as_str(),
        "https://status.example.com/api/v2/summary.json"
    );

    let rejected = initialized("ok", &[("status_page_url", "javascript:alert(1)")]).await;
    assert!(matches!(rejected, Err(PluginError::InvalidConfig(_))));
}

fn catalog_reply(path: &str, phase: usize) -> Reply {
    let path = path.split('?').next().unwrap_or(path);
    if phase == 2 && path == "/job/team/api/json" {
        return Reply {
            status: "503 Service Unavailable",
            headers: "",
            body: "catalog unavailable",
        };
    }
    let body = match path {
        "/api/json" => {
            r#"{"jobs":[{"name":"manual","_class":"hudson.model.FreeStyleProject"},{"name":"team","_class":"com.cloudbees.hudson.plugins.folder.Folder"},{"name":"branches","_class":"org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject"},{"name":"unsupported","_class":"hudson.matrix.MatrixProject"}]}"#
        }
        "/job/team/api/json" if phase > 0 => {
            r#"{"jobs":[{"name":"existing","_class":"hudson.model.FreeStyleProject"},{"name":"new-service","_class":"org.jenkinsci.plugins.workflow.job.WorkflowJob"}]}"#
        }
        "/job/team/api/json" => {
            r#"{"jobs":[{"name":"existing","_class":"hudson.model.FreeStyleProject"}]}"#
        }
        "/job/branches/api/json" if phase > 0 => {
            r#"{"jobs":[{"name":"main","_class":"org.jenkinsci.plugins.workflow.job.WorkflowJob"},{"name":"feature%2Fnext","_class":"org.jenkinsci.plugins.workflow.job.WorkflowJob"}]}"#
        }
        "/job/branches/api/json" => {
            r#"{"jobs":[{"name":"main","_class":"org.jenkinsci.plugins.workflow.job.WorkflowJob"}]}"#
        }
        "/job/manual/api/json" => r#"{"name":"manual"}"#,
        "/job/team/job/existing/api/json" => r#"{"name":"existing"}"#,
        "/job/team/job/new-service/api/json" => r#"{"name":"new-service"}"#,
        "/job/branches/job/main/api/json" => r#"{"name":"main"}"#,
        "/job/branches/job/feature%2Fnext/api/json" => r#"{"name":"feature%2Fnext"}"#,
        _ => {
            return Reply {
                status: "404 Not Found",
                headers: "",
                body: "unexpected job",
            };
        }
    };
    Reply {
        status: "200 OK",
        headers: "Content-Type: application/json\r\n",
        body,
    }
}

async fn catalog_plugin(phase: Arc<AtomicUsize>, selected: &str, follow: bool) -> JenkinsPlugin {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let server_url = stub_server_with_head(Arc::default(), Duration::ZERO, move |path, _| {
        catalog_reply(path, phase.load(Ordering::SeqCst))
    })
    .await;
    let mut config = HashMap::from([
        ("server_url".to_string(), server_url),
        ("username".to_string(), "alice".to_string()),
        ("token".to_string(), "token".to_string()),
        ("selected_items".to_string(), selected.to_string()),
    ]);
    if follow {
        config.insert("follow_all_jobs".to_string(), "true".to_string());
    }
    let mut plugin = JenkinsPlugin::new();
    plugin
        .initialize(1, config, Some(Arc::new(reqwest::Client::new())))
        .unwrap();
    plugin
}

async fn monitored_jobs(plugin: &JenkinsPlugin) -> Vec<String> {
    let mut ids: Vec<_> = plugin
        .fetch_pipelines()
        .await
        .unwrap()
        .into_iter()
        .map(|pipeline| pipeline.id)
        .collect();
    ids.sort();
    ids
}

#[tokio::test]
async fn following_all_jobs_refreshes_nested_jobs_and_restores_manual_selection() {
    let phase = Arc::new(AtomicUsize::new(0));
    let mut plugin = catalog_plugin(phase.clone(), "manual", false).await;
    assert_eq!(monitored_jobs(&plugin).await, ["manual"]);

    plugin
        .config
        .insert("follow_all_jobs".into(), "true".into());
    assert_eq!(
        monitored_jobs(&plugin).await,
        ["branches/main", "manual", "team/existing"]
    );
    phase.store(1, Ordering::SeqCst);
    assert_eq!(
        monitored_jobs(&plugin).await,
        [
            "branches/feature%2Fnext",
            "branches/main",
            "manual",
            "team/existing",
            "team/new-service",
        ]
    );
    phase.store(2, Ordering::SeqCst);
    assert!(matches!(
        plugin.fetch_pipelines().await,
        Err(PluginError::ServiceUnavailable { .. })
    ));

    plugin
        .config
        .insert("follow_all_jobs".into(), "false".into());
    assert_eq!(monitored_jobs(&plugin).await, ["manual"]);
}

#[tokio::test]
async fn following_all_jobs_does_not_require_a_saved_selection() {
    let plugin = catalog_plugin(Arc::default(), "", true).await;

    assert_eq!(
        monitored_jobs(&plugin).await,
        ["branches/main", "manual", "team/existing"]
    );
}

fn jenkins_core(path: &str) -> Reply {
    let json = "Content-Type: application/json\r\n";
    if path.starts_with("/job/plain/api/json") {
        Reply {
            status: "200 OK",
            headers: json,
            body: r#"{"property":[{"_class":"jenkins.model.BuildDiscarderProperty"}]}"#,
        }
    } else if path.starts_with("/job/release/api/json") {
        Reply {
            status: "200 OK",
            headers: json,
            body: r#"{"property":[{"_class":"hudson.model.ParametersDefinitionProperty","parameterDefinitions":[{"_class":"hudson.model.StringParameterDefinition","name":"BRANCH","defaultParameterValue":{"value":"main"}}]}]}"#,
        }
    } else if path == "/job/plain/build" || path == "/job/release/buildWithParameters" {
        Reply {
            status: "201 Created",
            headers: "",
            body: "",
        }
    } else {
        Reply {
            status: "400 Bad Request",
            headers: "",
            body: "job is not parameterized",
        }
    }
}

async fn trigger_without_inputs(job: &str) -> (PluginResult<TriggerResult>, usize) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let requests = Arc::new(AtomicUsize::new(0));
    let server_url = stub_server(requests.clone(), Duration::ZERO, jenkins_core).await;
    let mut plugin = JenkinsPlugin::new();
    plugin
        .initialize(
            1,
            HashMap::from([
                ("server_url".to_string(), server_url),
                ("username".to_string(), "alice".to_string()),
                ("token".to_string(), "secret".to_string()),
            ]),
            Some(Arc::new(reqwest::Client::new())),
        )
        .unwrap();

    let result = plugin
        .trigger_pipeline(TriggerParams {
            workflow_id: job.to_string(),
            inputs: None,
        })
        .await;

    (result, requests.load(Ordering::SeqCst))
}

#[tokio::test]
async fn trigger_posts_to_build_for_a_job_without_parameters() {
    let (result, requests) = trigger_without_inputs("plain").await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(requests, 2);
}

#[tokio::test]
async fn trigger_posts_to_build_with_parameters_for_a_parameterized_job() {
    let (result, requests) = trigger_without_inputs("release").await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(requests, 2);
}

const BUILD_41: &str = r#"{"number":41,"url":"http://jenkins.example/job/team/job/app/41/","result":"SUCCESS","building":false,"timestamp":1761646800000,"duration":90000,"fullDisplayName":"team » app #41"}"#;
const BUILD_42: &str = r#"{"number":42,"url":"http://jenkins.example/job/team/job/app/42/","result":null,"building":true,"timestamp":1761647000000,"duration":0,"fullDisplayName":"team » app #42"}"#;
const CONSOLE_42: &str = "Started by user alice\n[Pipeline] Start of Pipeline\n[Pipeline] stage\n[Pipeline] { (Build)\n[Pipeline] sh\n+ make\nbuilding\n";

fn folder_job(path: &str) -> Reply {
    let json = |body: &'static str| Reply {
        status: "200 OK",
        headers: "Content-Type: application/json\r\n",
        body,
    };
    let missing = |status: &'static str| Reply {
        status,
        headers: "",
        body: "gone",
    };
    match path.strip_prefix("/job/team/job/app/") {
        Some(rest) if rest.starts_with("41/api/json") => json(BUILD_41),
        Some(rest) if rest.starts_with("42/api/json") => json(BUILD_42),
        Some("42/logText/progressiveText?start=0") => Reply {
            status: "200 OK",
            headers: "Content-Type: text/plain;charset=UTF-8\r\nX-Text-Size: 131\r\nX-More-Data: true\r\n",
            body: CONSOLE_42,
        },
        Some("42/logText/progressiveText?start=131") => Reply {
            status: "200 OK",
            headers: "Content-Type: text/plain;charset=UTF-8\r\nX-Text-Size: 136\r\nX-More-Data: true\r\n",
            body: "done\n",
        },
        Some("42/logText/progressiveText?start=136") => Reply {
            status: "200 OK",
            headers: "Content-Type: text/plain;charset=UTF-8\r\nX-Text-Size: 136\r\nX-More-Data: true\r\n",
            body: "",
        },
        Some("42/logText/progressiveText?start=900") => Reply {
            status: "200 OK",
            headers: "Content-Type: text/plain;charset=UTF-8\r\nX-Text-Size: 131\r\nX-More-Data: true\r\n",
            body: CONSOLE_42,
        },
        Some("42/logText/progressiveText?start=777") => Reply {
            status: "200 OK",
            headers: "Content-Type: text/plain;charset=UTF-8\r\n",
            body: CONSOLE_42,
        },
        Some("8/logText/progressiveText?start=0") => missing("410 Gone"),
        _ => missing("404 Not Found"),
    }
}

async fn folder_plugin(requests: Arc<AtomicUsize>) -> JenkinsPlugin {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let expected = format!(
        "Basic {}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, "alice:secret")
    );
    let server_url = stub_server_with_head(requests, Duration::ZERO, move |path, head| {
        let authorized = head.lines().any(|line| {
            line.to_ascii_lowercase().starts_with("authorization:") && line.ends_with(&expected)
        });
        if authorized {
            folder_job(path)
        } else {
            Reply {
                status: "401 Unauthorized",
                headers: "",
                body: "denied",
            }
        }
    })
    .await;
    let mut plugin = JenkinsPlugin::new();
    plugin
        .initialize(
            1,
            HashMap::from([
                ("server_url".to_string(), server_url),
                ("username".to_string(), "alice".to_string()),
                ("token".to_string(), "secret".to_string()),
            ]),
            Some(Arc::new(reqwest::Client::new())),
        )
        .unwrap();
    plugin
}

fn build_of_folder_job(
    run_number: i64, metadata: &HashMap<String, serde_json::Value>,
) -> RunRef<'_> {
    RunRef {
        pipeline_id: "team/app",
        run_number,
        run_id: "jenkins-build-42",
        metadata,
    }
}

async fn resume_console(
    plugin: &JenkinsPlugin, run_number: i64, node_id: &str, from: Option<&LogCursor>,
) -> PluginResult<(Vec<u8>, LogRead, bool)> {
    let metadata = HashMap::new();
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut sink = LogSink::new(tx);
    let result = plugin
        .read_log(
            build_of_folder_job(run_number, &metadata),
            node_id,
            from,
            &mut sink,
        )
        .await;
    let restarted = sink.restarted();
    drop(sink);
    let mut bytes = Vec::new();
    while let Some(chunk) = rx.recv().await {
        bytes.extend_from_slice(&chunk);
    }
    result.map(|read| (bytes, read, restarted))
}

async fn read_console(
    plugin: &JenkinsPlugin, run_number: i64, node_id: &str,
) -> PluginResult<Vec<u8>> {
    resume_console(plugin, run_number, node_id, None)
        .await
        .map(|(bytes, _, _)| bytes)
}

fn console_cursor(build: i64, offset: u64) -> LogCursor {
    LogCursor::new(&serde_json::json!({ "build": build, "offset": offset })).unwrap()
}

#[tokio::test]
async fn finished_and_running_builds_become_one_job_node_each() {
    let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;
    let metadata = HashMap::new();

    let finished = plugin
        .fetch_log_nodes(build_of_folder_job(41, &metadata))
        .await
        .unwrap();
    let running = plugin
        .fetch_log_nodes(build_of_folder_job(42, &metadata))
        .await
        .unwrap();

    assert_eq!(finished.len(), 1);
    let node = &finished[0];
    assert_eq!(node.id, "41");
    assert_eq!(node.parent_id, None);
    assert_eq!(node.kind, LogNodeKind::Job);
    assert_eq!(node.name, "team » app #41");
    assert_eq!(node.status, PipelineStatus::Success);
    assert_eq!(node.log, Some(LogAvailability::Ready));
    assert_eq!(
        node.web_url.as_deref(),
        Some("http://jenkins.example/job/team/job/app/41/")
    );
    assert_eq!(
        node.started_at.unwrap().timestamp_millis(),
        1_761_646_800_000
    );
    assert_eq!(
        node.finished_at.unwrap().timestamp_millis(),
        1_761_646_890_000
    );
    assert_eq!(running.len(), 1);
    let node = &running[0];
    assert_eq!(node.id, "42");
    assert_eq!(node.status, PipelineStatus::Running);
    assert_eq!(node.log, Some(LogAvailability::Partial));
    assert_eq!(node.finished_at, None);
}

#[tokio::test]
async fn build_that_does_not_exist_has_no_log_nodes() {
    let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;
    let metadata = HashMap::new();

    let error = plugin
        .fetch_log_nodes(build_of_folder_job(99, &metadata))
        .await
        .unwrap_err();

    assert!(
        matches!(error, PluginError::PipelineNotFound(_)),
        "{error:?}"
    );
}

#[tokio::test]
async fn first_console_read_is_the_whole_log_and_resumes_at_the_server_text_size() {
    let requests = Arc::new(AtomicUsize::new(0));
    let plugin = folder_plugin(requests.clone()).await;

    let (log, read, restarted) = resume_console(&plugin, 42, "42", None).await.unwrap();

    assert_eq!(log, CONSOLE_42.as_bytes());
    assert_eq!(read, LogRead::replace(Some(console_cursor(42, 131))));
    assert!(!restarted);
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn resumed_console_read_appends_only_the_new_text() {
    let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;

    let (log, read, restarted) = resume_console(&plugin, 42, "42", Some(&console_cursor(42, 131)))
        .await
        .unwrap();

    assert_eq!(log, b"done\n");
    assert_eq!(read, LogRead::append(console_cursor(42, 136)));
    assert!(!restarted);
}

#[tokio::test]
async fn resumed_console_read_with_nothing_new_writes_nothing() {
    let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;

    let (log, read, restarted) = resume_console(&plugin, 42, "42", Some(&console_cursor(42, 136)))
        .await
        .unwrap();

    assert!(log.is_empty());
    assert_eq!(read, LogRead::append(console_cursor(42, 136)));
    assert!(!restarted);
}

#[tokio::test]
async fn console_that_rolled_over_restarts_with_the_whole_log() {
    let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;

    let (log, read, restarted) = resume_console(&plugin, 42, "42", Some(&console_cursor(42, 900)))
        .await
        .unwrap();

    assert_eq!(log, CONSOLE_42.as_bytes());
    assert_eq!(read, LogRead::replace(Some(console_cursor(42, 131))));
    assert!(restarted);
}

#[tokio::test]
async fn cursor_of_another_build_or_unknown_shape_restarts_from_zero() {
    let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;
    let foreign = console_cursor(41, 136);
    let malformed = LogCursor::new(&"junk").unwrap();

    for cursor in [foreign, malformed] {
        let (log, read, restarted) = resume_console(&plugin, 42, "42", Some(&cursor))
            .await
            .unwrap();

        assert_eq!(log, CONSOLE_42.as_bytes(), "{cursor:?}");
        assert_eq!(
            read,
            LogRead::replace(Some(console_cursor(42, 131))),
            "{cursor:?}"
        );
        assert!(restarted, "{cursor:?}");
    }
}

#[tokio::test]
async fn resumed_console_read_without_a_text_size_fails_instead_of_guessing() {
    let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;

    let error = resume_console(&plugin, 42, "42", Some(&console_cursor(42, 777)))
        .await
        .unwrap_err();

    assert!(matches!(error, PluginError::ApiError(_)), "{error:?}");
}

#[tokio::test]
async fn pruned_console_log_is_not_found() {
    let plugin = folder_plugin(Arc::new(AtomicUsize::new(0))).await;

    for build in ["7", "8"] {
        let error = read_console(&plugin, 42, build).await.unwrap_err();
        assert!(
            matches!(&error, PluginError::PipelineNotFound(message) if message.contains("no longer available")),
            "{build}: {error:?}"
        );
    }
}

#[tokio::test]
async fn build_numbers_outside_the_digit_alphabet_are_rejected_without_a_request() {
    let requests = Arc::new(AtomicUsize::new(0));
    let plugin = folder_plugin(requests.clone()).await;

    let rejected = [
        "",
        "../../api/json",
        "42/consoleText",
        "42?tree=x",
        "4 2",
        "-1",
        "+1",
        "٤٢",
        "99999999999999999999",
    ];
    for node_id in rejected {
        let error = read_console(&plugin, 42, node_id).await.unwrap_err();
        assert!(
            matches!(error, PluginError::InvalidConfig(_)),
            "{node_id}: {error:?}"
        );
    }
    assert_eq!(requests.load(Ordering::SeqCst), 0);
}

#[test]
fn configured_repositories_should_prefix_root_jobs() {
    let config = HashMap::from([(
        "selected_items".to_string(),
        "deploy, team/build ,".to_string(),
    )]);
    assert_eq!(
        JenkinsPlugin::new().configured_repositories(&config),
        ["(root)/deploy", "team/build"]
    );
}
