use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    AvailablePipeline,
    LogAvailability,
    LogNode,
    LogNodeKind,
    Pipeline,
    PipelineRun,
    PipelineStatus,
};
use serde_json::{
    json,
    Value,
};

use crate::config;
use crate::types::{
    StackListItem,
    Update,
    UpdateListItem,
    UpdateLogs,
};

pub(crate) fn map_stack_state(state: &str) -> PipelineStatus {
    match state {
        "running" => PipelineStatus::Success,
        "deploying" | "removing" => PipelineStatus::Running,
        "restarting" | "unhealthy" | "dead" => PipelineStatus::Failed,
        "paused" | "stopped" | "down" => PipelineStatus::Cancelled,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn map_update_status(status: &str, success: bool) -> PipelineStatus {
    match (status, success) {
        ("Queued", _) => PipelineStatus::Pending,
        ("InProgress", _) => PipelineStatus::Running,
        ("Complete", true) => PipelineStatus::Success,
        ("Complete", false) => PipelineStatus::Failed,
        _ => PipelineStatus::Pending,
    }
}

pub(crate) fn run_url(server_url: &str, stack_id: &str, update_id: &str) -> String {
    if update_id.is_empty() {
        format!("{server_url}/stacks/{stack_id}")
    } else {
        format!("{server_url}/updates/{update_id}")
    }
}

fn timestamp(millis: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(millis).unwrap_or_default()
}

fn optional_timestamp(millis: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(millis).filter(|_| millis > 0)
}

fn repository(stack: &StackListItem) -> String {
    match stack.info.host() {
        "" => stack.name.clone(),
        host => format!("{host}/{}", stack.name),
    }
}

pub(crate) fn map_pipeline(
    stack: &StackListItem, provider_id: i64, last_update_start: Option<i64>,
) -> Pipeline {
    let info = &stack.info;
    let mut metadata = HashMap::from([
        ("stack_id".to_string(), json!(stack.id)),
        ("state".to_string(), json!(info.state)),
        (
            "update_available".to_string(),
            json!(info.update_available()),
        ),
        ("service_count".to_string(), json!(info.services.len())),
        ("project_missing".to_string(), json!(info.project_missing)),
        ("missing_files".to_string(), json!(info.missing_files.len())),
    ]);
    let optional = [
        ("server", Some(info.host())),
        ("repo", Some(info.repo.as_str())),
        ("repo_url", Some(info.repo_link.as_str())),
        ("docker_status", info.status.as_deref()),
        ("deployed_hash", info.deployed_hash.as_deref()),
        ("latest_hash", info.latest_hash.as_deref()),
    ];
    for (key, value) in optional {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            metadata.insert(key.to_string(), json!(value));
        }
    }

    let branch = (!info.repo.is_empty() && !info.branch.is_empty()).then(|| info.branch.clone());

    Pipeline {
        id: config::build_pipeline_id(provider_id, &stack.id),
        provider_id,
        provider_type: "komodo".to_string(),
        name: stack.name.clone(),
        status: map_stack_state(&info.state),
        last_run: last_update_start.map(timestamp),
        last_updated: Utc::now(),
        repository: repository(stack),
        branch,
        workflow_file: None,
        metadata,
    }
}

pub(crate) fn map_available_pipeline(stack: &StackListItem) -> AvailablePipeline {
    let info = &stack.info;
    let mut details = vec![format!("State: {}", info.state)];
    if !info.host().is_empty() {
        details.push(format!("Server: {}", info.host()));
    }
    if !info.repo.is_empty() {
        details.push(format!("Repo: {}@{}", info.repo, info.branch));
    }

    AvailablePipeline {
        id: stack.id.clone(),
        name: stack.name.clone(),
        description: Some(details.join(" | ")),
        organization: Some(info.host().to_string()).filter(|host| !host.is_empty()),
        repository: Some(info.repo.clone()).filter(|repo| !repo.is_empty()),
    }
}

pub(crate) fn map_run(
    pipeline_id: &str, stack_id: &str, server_url: &str, item: &UpdateListItem,
    detail: Option<&Update>,
) -> PipelineRun {
    let (status, success) = detail.map_or((item.status.as_str(), item.success), |update| {
        (update.status.as_str(), update.success)
    });
    let started_at = timestamp(item.start_ts);
    let end_ts = detail.and_then(|update| update.end_ts);
    let actor = [&item.username, &item.operator]
        .into_iter()
        .find(|name| !name.is_empty())
        .cloned();

    let mut metadata: HashMap<String, Value> = HashMap::from([
        ("operation".to_string(), json!(item.operation)),
        ("update_status".to_string(), json!(status)),
        ("operator".to_string(), json!(item.operator)),
    ]);
    if !item.id.is_empty() {
        metadata.insert("update_id".to_string(), json!(item.id));
    }

    PipelineRun {
        id: format!("{pipeline_id}__{}", item.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: item.start_ts,
        status: map_update_status(status, success),
        started_at,
        concluded_at: end_ts.map(timestamp),
        duration_seconds: end_ts.map(|end| (end - item.start_ts).max(0) / 1000),
        logs_url: run_url(server_url, stack_id, &item.id),
        commit_sha: detail
            .map(|update| update.commit_hash.clone())
            .filter(|hash| !hash.is_empty()),
        commit_message: None,
        branch: None,
        actor,
        inputs: None,
        metadata,
    }
}

pub(crate) fn map_log_nodes(update: &UpdateLogs, web_url: &str) -> Vec<LogNode> {
    let last = update.logs.len().saturating_sub(1);
    update
        .logs
        .iter()
        .enumerate()
        .map(|(index, log)| {
            let open = update.status == "InProgress" && index == last;
            let started_at = optional_timestamp(log.start_ts);
            let name = match log.stage.trim() {
                "" => format!("Stage {}", index + 1),
                stage => stage.to_string(),
            };
            LogNode {
                id: index.to_string(),
                parent_id: None,
                kind: LogNodeKind::Job,
                name,
                status: map_update_status(
                    if open { "InProgress" } else { "Complete" },
                    log.success,
                ),
                started_at,
                finished_at: started_at
                    .and(optional_timestamp(log.end_ts))
                    .filter(|_| !open),
                log: Some(if open {
                    LogAvailability::Partial
                } else {
                    LogAvailability::Ready
                }),
                web_url: Some(web_url.to_string()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(json: &str) -> StackListItem {
        serde_json::from_str(json).unwrap()
    }

    fn update_item(json: &str) -> UpdateListItem {
        serde_json::from_str(json).unwrap()
    }

    const MAIN_STACK: &str = r#"{
        "id": "66a1", "type": "Stack", "name": "web", "template": false, "tags": [],
        "info": {
            "swarm_id": "", "swarm_name": "", "server_id": "66s1", "server_name": "prod-1",
            "files_on_host": false, "file_contents": false, "linked_repo": "",
            "linked_repo_name": "", "git_provider": "github.com", "repo": "acme/web",
            "branch": "main", "repo_link": "https://github.com/acme/web/tree/main",
            "state": "running", "status": "running(2)",
            "services": [
                {"service": "app", "image": "acme/web:1", "latest_image": "acme/web:2", "update_available": true},
                {"service": "db", "image": "postgres:16", "latest_image": null, "update_available": false}
            ],
            "auto_update_all_services": false, "project_missing": false, "missing_files": [],
            "deployed_hash": "abc1234", "latest_hash": "def5678"
        }
    }"#;

    const V1_STACK: &str = r#"{
        "id": "66a2", "type": "Stack", "name": "files", "template": false, "tags": [],
        "info": {
            "server_id": "66s1", "files_on_host": true, "file_contents": false,
            "linked_repo": "", "git_provider": "github.com", "repo": "", "branch": "main",
            "repo_link": "", "state": "down", "status": null,
            "services": [{"service": "app", "image": "nginx", "update_available": false}],
            "project_missing": false, "missing_files": ["compose.yaml"],
            "deployed_hash": null, "latest_hash": null
        }
    }"#;

    #[test]
    fn stack_states_become_pipeline_statuses() {
        let expected = [
            ("deploying", PipelineStatus::Running),
            ("running", PipelineStatus::Success),
            ("paused", PipelineStatus::Cancelled),
            ("stopped", PipelineStatus::Cancelled),
            ("created", PipelineStatus::Pending),
            ("restarting", PipelineStatus::Failed),
            ("dead", PipelineStatus::Failed),
            ("removing", PipelineStatus::Running),
            ("unhealthy", PipelineStatus::Failed),
            ("down", PipelineStatus::Cancelled),
            ("unknown", PipelineStatus::Pending),
            ("something_new", PipelineStatus::Pending),
            ("", PipelineStatus::Pending),
        ];
        for (state, status) in expected {
            assert_eq!(map_stack_state(state), status, "{state}");
        }
    }

    #[test]
    fn update_statuses_and_success_become_run_statuses() {
        let expected = [
            ("Queued", false, PipelineStatus::Pending),
            ("Queued", true, PipelineStatus::Pending),
            ("InProgress", false, PipelineStatus::Running),
            ("InProgress", true, PipelineStatus::Running),
            ("Complete", true, PipelineStatus::Success),
            ("Complete", false, PipelineStatus::Failed),
            ("SomethingNew", true, PipelineStatus::Pending),
            ("", false, PipelineStatus::Pending),
        ];
        for (status, success, expected) in expected {
            assert_eq!(
                map_update_status(status, success),
                expected,
                "{status} {success}"
            );
        }
    }

    #[test]
    fn stack_on_main_maps_server_repo_and_update_facts() {
        let pipeline = map_pipeline(&stack(MAIN_STACK), 3, Some(1761660000123));

        assert_eq!(pipeline.id, "komodo__3__66a1");
        assert_eq!(pipeline.provider_type, "komodo");
        assert_eq!(pipeline.name, "web");
        assert_eq!(pipeline.status, PipelineStatus::Success);
        assert_eq!(pipeline.repository, "prod-1/web");
        assert_eq!(pipeline.branch.as_deref(), Some("main"));
        assert_eq!(
            pipeline.last_run.map(|at| at.timestamp_millis()),
            Some(1761660000123)
        );
        assert_eq!(pipeline.metadata["server"], json!("prod-1"));
        assert_eq!(pipeline.metadata["state"], json!("running"));
        assert_eq!(pipeline.metadata["update_available"], json!(true));
        assert_eq!(pipeline.metadata["service_count"], json!(2));
        assert_eq!(pipeline.metadata["repo"], json!("acme/web"));
        assert_eq!(
            pipeline.metadata["repo_url"],
            json!("https://github.com/acme/web/tree/main")
        );
        assert_eq!(pipeline.metadata["docker_status"], json!("running(2)"));
        assert_eq!(pipeline.metadata["deployed_hash"], json!("abc1234"));
        assert_eq!(pipeline.metadata["latest_hash"], json!("def5678"));
    }

    #[test]
    fn stack_on_v1_without_names_falls_back_to_server_id_and_skips_empty_facts() {
        let pipeline = map_pipeline(&stack(V1_STACK), 3, None);

        assert_eq!(pipeline.status, PipelineStatus::Cancelled);
        assert_eq!(pipeline.last_run, None);
        assert_eq!(pipeline.repository, "66s1/files");
        assert_eq!(pipeline.branch, None);
        assert_eq!(pipeline.metadata["server"], json!("66s1"));
        assert_eq!(pipeline.metadata["update_available"], json!(false));
        assert_eq!(pipeline.metadata["missing_files"], json!(1));
        for key in [
            "repo",
            "repo_url",
            "docker_status",
            "deployed_hash",
            "latest_hash",
        ] {
            assert!(!pipeline.metadata.contains_key(key), "{key}");
        }
    }

    #[test]
    fn stack_with_only_a_name_is_still_mapped() {
        let pipeline = map_pipeline(&stack(r#"{"id": "x", "name": "bare"}"#), 1, None);

        assert_eq!(pipeline.repository, "bare");
        assert_eq!(pipeline.status, PipelineStatus::Pending);
        assert!(!pipeline.metadata.contains_key("server"));
    }

    #[test]
    fn swarm_stack_is_grouped_by_swarm_name() {
        let pipeline = map_pipeline(
            &stack(
                r#"{"id": "x", "name": "api", "info": {"swarm_id": "s", "swarm_name": "edge", "server_id": "", "state": "running"}}"#,
            ),
            1,
            None,
        );

        assert_eq!(pipeline.repository, "edge/api");
        assert_eq!(pipeline.metadata["server"], json!("edge"));
    }

    #[test]
    fn available_pipeline_is_keyed_by_stack_id() {
        let available = map_available_pipeline(&stack(MAIN_STACK));

        assert_eq!(available.id, "66a1");
        assert_eq!(available.name, "web");
        assert_eq!(available.organization.as_deref(), Some("prod-1"));
        assert_eq!(available.repository.as_deref(), Some("acme/web"));
        assert_eq!(
            available.description.as_deref(),
            Some("State: running | Server: prod-1 | Repo: acme/web@main")
        );
    }

    #[test]
    fn listed_update_maps_actor_status_start_and_link() {
        let item = update_item(
            r#"{"id": "66u1", "operation": "DeployStack", "start_ts": 1761660000123, "success": true,
                "username": "ada", "operator": "66usr", "target": {"type": "Stack", "id": "66a1"},
                "status": "Complete"}"#,
        );
        let run = map_run(
            "komodo__3__66a1",
            "66a1",
            "https://komodo.test",
            &item,
            None,
        );

        assert_eq!(run.id, "komodo__3__66a1__66u1");
        assert_eq!(run.run_number, 1761660000123);
        assert_eq!(run.status, PipelineStatus::Success);
        assert_eq!(run.actor.as_deref(), Some("ada"));
        assert_eq!(run.started_at.timestamp_millis(), 1761660000123);
        assert_eq!(run.concluded_at, None);
        assert_eq!(run.duration_seconds, None);
        assert_eq!(run.logs_url, "https://komodo.test/updates/66u1");
        assert_eq!(run.metadata["operation"], json!("DeployStack"));
        assert_eq!(run.metadata["update_id"], json!("66u1"));
    }

    #[test]
    fn detailed_update_adds_end_duration_commit_and_current_status() {
        let item = update_item(
            r#"{"id": "66u1", "operation": "DeployStack", "start_ts": 1761660000000,
                "success": false, "username": "", "operator": "Github", "status": "InProgress"}"#,
        );
        let detail: Update = serde_json::from_str(
            r#"{"_id": {"$oid": "66u1"}, "operation": "DeployStack", "start_ts": 1761660000000,
                "success": false, "operator": "Github", "logs": [], "end_ts": 1761660042500,
                "status": "Complete", "commit_hash": "abc1234"}"#,
        )
        .unwrap();
        let run = map_run(
            "komodo__3__66a1",
            "66a1",
            "https://komodo.test",
            &item,
            Some(&detail),
        );

        assert_eq!(run.status, PipelineStatus::Failed);
        assert_eq!(run.actor.as_deref(), Some("Github"));
        assert_eq!(run.duration_seconds, Some(42));
        assert_eq!(
            run.concluded_at.map(|at| at.timestamp_millis()),
            Some(1761660042500)
        );
        assert_eq!(run.commit_sha.as_deref(), Some("abc1234"));
    }

    #[test]
    fn update_without_id_links_to_the_stack_page() {
        let run = map_run(
            "komodo__3__66a1",
            "66a1",
            "https://komodo.test",
            &update_item(r#"{"start_ts": 5}"#),
            None,
        );

        assert_eq!(run.logs_url, "https://komodo.test/stacks/66a1");
        assert_eq!(run.status, PipelineStatus::Pending);
        assert_eq!(run.actor, None);
    }

    #[test]
    fn update_ids_parse_from_object_id_and_plain_string() {
        let object: Update = serde_json::from_str(r#"{"_id": {"$oid": "66u1"}}"#).unwrap();
        let plain: Update = serde_json::from_str(r#"{"_id": "66u2"}"#).unwrap();
        let missing: Update = serde_json::from_str("{}").unwrap();

        assert_eq!(object.id.unwrap().into_string(), "66u1");
        assert_eq!(plain.id.unwrap().into_string(), "66u2");
        assert!(missing.id.is_none());
    }

    const STAGES: &str = r#"[
        {"stage": "Clone Repo", "command": "git clone", "stdout": "ok", "stderr": "", "success": true, "start_ts": 1761660000000, "end_ts": 1761660002000},
        {"stage": "Compose Up", "command": "docker compose up -d", "stdout": "", "stderr": "boom", "success": false, "start_ts": 1761660002000, "end_ts": 1761660009500}
    ]"#;

    fn log_nodes(status: &str, stages: &str) -> Vec<LogNode> {
        let update: UpdateLogs =
            serde_json::from_str(&format!(r#"{{"status": "{status}", "logs": {stages}}}"#))
                .unwrap();
        map_log_nodes(&update, "https://komodo.test/updates/66u1")
    }

    #[test]
    fn every_update_status_maps_stages_to_a_status_and_availability() {
        let finished = [
            (PipelineStatus::Success, LogAvailability::Ready),
            (PipelineStatus::Failed, LogAvailability::Ready),
        ];
        let in_progress = [
            (PipelineStatus::Success, LogAvailability::Ready),
            (PipelineStatus::Running, LogAvailability::Partial),
        ];
        let cases = [
            ("Complete", finished),
            ("Queued", finished),
            ("something_new", finished),
            ("", finished),
            ("InProgress", in_progress),
        ];
        for (update_status, expected) in cases {
            let nodes = log_nodes(update_status, STAGES);
            let actual: Vec<_> = nodes
                .iter()
                .map(|node| (node.status, node.log.unwrap()))
                .collect();
            assert_eq!(actual, expected, "{update_status:?}");
        }
    }

    #[test]
    fn stages_become_job_nodes_named_after_the_stage_with_their_own_times() {
        let nodes = log_nodes("Complete", STAGES);

        assert_eq!(
            nodes
                .iter()
                .map(|node| (node.id.as_str(), node.name.as_str(), node.kind))
                .collect::<Vec<_>>(),
            [
                ("0", "Clone Repo", LogNodeKind::Job),
                ("1", "Compose Up", LogNodeKind::Job)
            ]
        );
        assert!(nodes.iter().all(|node| node.parent_id.is_none()));
        assert!(nodes
            .iter()
            .all(|node| node.web_url.as_deref() == Some("https://komodo.test/updates/66u1")));
        let millis = |at: Option<DateTime<Utc>>| at.map(|at| at.timestamp_millis());
        assert_eq!(millis(nodes[0].started_at), Some(1761660000000));
        assert_eq!(millis(nodes[0].finished_at), Some(1761660002000));
        assert_eq!(millis(nodes[1].started_at), Some(1761660002000));
        assert_eq!(millis(nodes[1].finished_at), Some(1761660009500));
    }

    #[test]
    fn the_open_stage_of_a_running_update_has_no_end_time() {
        let nodes = log_nodes("InProgress", STAGES);

        assert!(nodes[0].finished_at.is_some());
        assert!(nodes[1].started_at.is_some());
        assert_eq!(nodes[1].finished_at, None);
    }

    #[test]
    fn unnamed_stages_and_missing_times_get_plain_fallbacks() {
        let nodes = log_nodes(
            "Complete",
            r#"[{"stage": " ", "success": true}, {"stage": "Compose Down", "success": true}]"#,
        );

        assert_eq!(nodes[0].name, "Stage 1");
        assert_eq!(nodes[1].name, "Compose Down");
        assert_eq!(nodes[0].started_at, None);
        assert_eq!(nodes[0].finished_at, None);
    }

    #[test]
    fn an_update_without_logs_has_no_nodes() {
        assert!(log_nodes("Queued", "[]").is_empty());
        assert!(log_nodes("InProgress", "[]").is_empty());
    }
}
