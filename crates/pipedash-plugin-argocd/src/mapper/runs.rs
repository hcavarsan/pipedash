use std::collections::HashMap;

use pipedash_plugin_api::utils::duration_seconds;
use pipedash_plugin_api::{
    PipelineRun,
    PipelineStatus,
};

use super::{
    app_url,
    operation_status,
    pipeline_id_of,
};
use crate::types;

fn insert_app_status(metadata: &mut HashMap<String, serde_json::Value>, app: &types::Application) {
    metadata.insert(
        "app_sync_status".to_string(),
        serde_json::json!(app.status.sync.status),
    );
    metadata.insert(
        "app_health_status".to_string(),
        serde_json::json!(app.status.health.status),
    );
    metadata.insert(
        "destination_namespace".to_string(),
        serde_json::json!(app.spec.destination.namespace),
    );
}

pub(crate) fn map_history_to_run(
    history: &types::RevisionHistory, app: &types::Application, server_url: &str,
) -> PipelineRun {
    let pipeline_id = pipeline_id_of(app);

    let status = PipelineStatus::Success;

    let logs_url = app_url(server_url, &app.metadata.name);

    let mut metadata = HashMap::new();
    metadata.insert(
        "sync_revision".to_string(),
        serde_json::json!(history.primary_revision()),
    );
    metadata.insert("history_id".to_string(), serde_json::json!(history.id));

    if let Some(ref source) = history.source {
        metadata.insert("source_path".to_string(), serde_json::json!(source.path));

        metadata.insert("source_type".to_string(), serde_json::json!(source.kind()));

        if let Some(ref chart) = source.chart {
            metadata.insert("helm_chart".to_string(), serde_json::json!(chart));
        }
    }

    insert_app_status(&mut metadata, app);
    metadata.insert(
        "destination_cluster".to_string(),
        serde_json::json!(app.spec.destination.cluster()),
    );

    let run_number = history.deployed_at.timestamp();

    PipelineRun {
        id: format!("argocd-sync-{}-{}", app.metadata.name, history.id),
        pipeline_id,
        run_number,
        status,
        started_at: history.deployed_at,
        concluded_at: Some(history.deployed_at),
        duration_seconds: None,
        logs_url,
        commit_sha: history.primary_revision().map(str::to_string),
        commit_message: None,
        branch: app.spec.primary_source().target_revision.clone(),
        actor: None,
        inputs: None,
        metadata,
    }
}

pub(crate) fn map_runs(
    app: &types::Application, server_url: &str, limit: usize,
) -> Vec<PipelineRun> {
    let operation = map_operation_to_run(app, server_url);
    let operation_started = app.status.operation_state.as_ref().map(|op| op.started_at);
    let history = app
        .status
        .history
        .iter()
        .flatten()
        .rev()
        .filter(|entry| operation_started.is_none() || entry.deploy_started_at != operation_started)
        .take(limit)
        .map(|entry| map_history_to_run(entry, app, server_url));

    operation.into_iter().chain(history).collect()
}

pub(crate) fn map_operation_to_run(
    app: &types::Application, server_url: &str,
) -> Option<PipelineRun> {
    let operation_state = app.status.operation_state.as_ref()?;

    let pipeline_id = pipeline_id_of(app);

    let status = operation_status(&operation_state.phase);

    let duration = duration_seconds(
        Some(operation_state.started_at),
        operation_state.finished_at,
    );

    let logs_url = app_url(server_url, &app.metadata.name);

    let mut metadata = HashMap::new();
    metadata.insert(
        "operation_phase".to_string(),
        serde_json::json!(operation_state.phase),
    );
    if let Some(ref msg) = operation_state.message {
        metadata.insert("operation_message".to_string(), serde_json::json!(msg));
    }

    let source = app.spec.primary_source();
    metadata.insert("source_type".to_string(), serde_json::json!(source.kind()));
    insert_app_status(&mut metadata, app);

    if let Some(path) = &source.path {
        metadata.insert("source_path".to_string(), serde_json::json!(path));
    }

    let run_number = operation_state.started_at.timestamp();

    Some(PipelineRun {
        id: format!("argocd-sync-{}-current", app.metadata.name),
        pipeline_id,
        run_number,
        status,
        started_at: operation_state.started_at,
        concluded_at: operation_state.finished_at,
        duration_seconds: duration,
        logs_url,
        commit_sha: app.status.sync.revision.clone(),
        commit_message: None,
        branch: source.target_revision.clone(),
        actor: None,
        inputs: None,
        metadata,
    })
}
