use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::sync::Arc;

use pipedash_plugin_api::PluginError;

use super::*;
use crate::test_support::{
    stub_server,
    Reply,
};

fn unauthorized(_: &str) -> Reply {
    Reply {
        status: "401 Unauthorized",
        headers: "Content-Type: text/html\r\n",
        body: "<html><body><h2>HTTP ERROR 401 Unauthorized</h2></body></html>",
    }
}

fn not_found(_: &str) -> Reply {
    Reply {
        status: "404 Not Found",
        headers: "",
        body: "<html><body><h2>HTTP ERROR 404 Not Found</h2></body></html>",
    }
}

fn rate_limited(_: &str) -> Reply {
    Reply {
        status: "429 Too Many Requests",
        headers: "Retry-After: 120\r\n",
        body: "slow down",
    }
}

fn unavailable(_: &str) -> Reply {
    Reply {
        status: "503 Service Unavailable",
        headers: "",
        body: "maintenance",
    }
}

fn created(_: &str) -> Reply {
    Reply {
        status: "201 Created",
        headers: "",
        body: "",
    }
}

async fn client_for(
    reply: fn(&str) -> Reply, delay: Duration,
) -> (JenkinsClient, Arc<AtomicUsize>) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let requests = Arc::new(AtomicUsize::new(0));
    let server_url = stub_server(requests.clone(), delay, reply).await;
    let http_client = Client::builder()
        .timeout(Duration::from_millis(300))
        .build()
        .unwrap();
    let client = JenkinsClient::new(
        Arc::new(http_client),
        server_url,
        "Basic YWxpY2U6d3Jvbmc=".to_string(),
    );
    (client, requests)
}

#[tokio::test]
async fn rejected_credentials_fail_as_auth_error_without_retry() {
    let (client, requests) = client_for(unauthorized, Duration::ZERO).await;

    let results = [
        client.discover_all_jobs().await.err(),
        client.fetch_job_details("build").await.err(),
        client.fetch_build_details("build", 1).await.err(),
        client.fetch_build_history("build", 10).await.err(),
        client.fetch_job_parameters("build").await.err(),
    ];

    for error in results {
        assert!(
            matches!(error, Some(PluginError::AuthenticationFailed(_))),
            "{error:?}"
        );
    }
    assert_eq!(requests.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn missing_job_is_pipeline_not_found() {
    let (client, requests) = client_for(not_found, Duration::ZERO).await;

    let error = client.fetch_job_details("gone").await.unwrap_err();

    assert!(
        matches!(error, PluginError::PipelineNotFound(_)),
        "{error:?}"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn rate_limit_carries_retry_after_and_is_not_retried() {
    let (client, requests) = client_for(rate_limited, Duration::ZERO).await;

    let error = client.fetch_job_details("build").await.unwrap_err();

    assert!(
        matches!(error, PluginError::RateLimited { .. }),
        "{error:?}"
    );
    assert_eq!(error.retry_after(), Some(Duration::from_secs(120)));
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn server_error_is_service_unavailable_and_retried() {
    let (client, requests) = client_for(unavailable, Duration::ZERO).await;

    let error = client.fetch_job_details("build").await.unwrap_err();

    assert!(
        matches!(error, PluginError::ServiceUnavailable { .. }),
        "{error:?}"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn timed_out_mutations_are_not_sent_again() {
    let (client, requests) = client_for(created, Duration::from_secs(2)).await;

    let trigger = client
        .trigger_build("build", vec![("name".to_string(), "value".to_string())])
        .await
        .unwrap_err();
    let cancel = client.cancel_build("build", 7).await.unwrap_err();

    assert!(
        matches!(trigger, PluginError::NetworkError(_)),
        "{trigger:?}"
    );
    assert!(matches!(cancel, PluginError::NetworkError(_)), "{cancel:?}");
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn failed_trigger_error_has_no_parameter_values() {
    let (client, requests) = client_for(unavailable, Duration::ZERO).await;
    let secret = "s3cr3t-".repeat(20);

    let error = client
        .trigger_build(
            "build",
            vec![
                ("DEPLOY_TOKEN".to_string(), secret.clone()),
                ("short".to_string(), "hunter2".to_string()),
            ],
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, PluginError::ServiceUnavailable { .. }),
        "{error:?}"
    );
    let text = error.to_string();
    assert!(
        !text.contains("s3cr3t") && !text.contains("hunter2"),
        "{text}"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

fn multibranch(path: &str) -> Reply {
    let body = if path.starts_with("/job/app/") {
        r#"{"_class":"org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject","jobs":[{"_class":"org.jenkinsci.plugins.workflow.job.WorkflowJob","name":"main","url":"x"},{"_class":"org.jenkinsci.plugins.workflow.job.WorkflowJob","name":"feature-x","url":"x"}]}"#
    } else {
        r#"{"_class":"hudson.model.Hudson","jobs":[{"_class":"org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject","name":"app","url":"x"}]}"#
    };
    Reply {
        status: "200 OK",
        headers: "Content-Type: application/json\r\n",
        body,
    }
}

#[tokio::test]
async fn multibranch_pipeline_lists_its_branch_jobs() {
    let (client, requests) = client_for(multibranch, Duration::ZERO).await;

    let jobs = client.discover_all_jobs().await.unwrap();

    let mut paths: Vec<&str> = jobs.iter().map(|job| job.full_path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(paths, ["app/feature-x", "app/main"]);
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

fn org_folder_of_eight_projects(path: &str) -> Reply {
    let body = if path.starts_with("/job/org/job/") {
        r#"{"jobs":[{"_class":"org.jenkinsci.plugins.workflow.job.WorkflowJob","name":"main"}]}"#
    } else if path.starts_with("/job/org/") {
        r#"{"jobs":[{"_class":"org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject","name":"r0"},{"_class":"org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject","name":"r1"},{"_class":"org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject","name":"r2"},{"_class":"org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject","name":"r3"},{"_class":"org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject","name":"r4"},{"_class":"org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject","name":"r5"},{"_class":"org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject","name":"r6"},{"_class":"org.jenkinsci.plugins.workflow.multibranch.WorkflowMultiBranchProject","name":"r7"}]}"#
    } else {
        r#"{"jobs":[{"_class":"jenkins.branch.OrganizationFolder","name":"org"}]}"#
    };
    Reply {
        status: "200 OK",
        headers: "Content-Type: application/json\r\n",
        body,
    }
}

#[tokio::test]
async fn sibling_folders_are_read_side_by_side() {
    let delay = Duration::from_millis(100);
    let (client, requests) = client_for(org_folder_of_eight_projects, delay).await;

    let started = std::time::Instant::now();
    let jobs = client.discover_all_jobs().await.unwrap();
    let elapsed = started.elapsed();

    assert_eq!(jobs.len(), 8);
    assert_eq!(requests.load(Ordering::SeqCst), 10);
    assert!(elapsed < delay * 6, "walk took {elapsed:?}");
}

fn build_list(key: &str, count: i64) -> String {
    let builds: Vec<String> = (0..count)
        .map(|offset| format!(r#"{{"number":{},"result":"SUCCESS"}}"#, 150 - offset))
        .collect();
    format!(r#"{{"{key}":[{}]}}"#, builds.join(","))
}

static CAPPED_BUILDS: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| build_list("builds", 100));
static ALL_BUILDS: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| build_list("allBuilds", 150));

fn job_with_150_builds(path: &str) -> Reply {
    let body = if path.contains("tree=allBuilds") {
        ALL_BUILDS.as_str()
    } else {
        CAPPED_BUILDS.as_str()
    };
    Reply {
        status: "200 OK",
        headers: "Content-Type: application/json\r\n",
        body,
    }
}

#[tokio::test]
async fn build_history_reads_past_the_newest_100_builds() {
    let (client, requests) = client_for(job_with_150_builds, Duration::ZERO).await;

    let builds = client.fetch_build_history("build", 200).await.unwrap();

    assert_eq!(builds.len(), 150);
    assert_eq!(builds.last().map(|build| build.number), Some(1));
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}
