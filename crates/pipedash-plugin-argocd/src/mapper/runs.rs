use std::collections::HashMap;

use pipedash_plugin_api::utils::duration_seconds;
use pipedash_plugin_api::{
    Correlation,
    Observed,
    PipelineRun,
    PipelineStatus,
    RunEvidence,
};

use super::{
    app_url,
    correlation,
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

    metadata.insert(
        "destination_namespace".to_string(),
        serde_json::json!(app.spec.destination.namespace),
    );
    metadata.insert(
        "destination_cluster".to_string(),
        serde_json::json!(app.spec.destination.cluster()),
    );

    let run_number = history.deployed_at.timestamp();

    PipelineRun {
        correlation: correlation::history(history),
        evidence: RunEvidence {
            outcome: Observed::Absent,
            timing: if history.deploy_started_at.is_some() {
                Observed::Recorded
            } else {
                Observed::Absent
            },
            attempt: false,
        },
        id: format!("argocd-sync-{}-{}", app.metadata.name, history.id),
        pipeline_id,
        run_number,
        status,
        started_at: history.deploy_started_at.unwrap_or(history.deployed_at),
        concluded_at: Some(history.deployed_at),
        duration_seconds: duration_seconds(history.deploy_started_at, Some(history.deployed_at)),
        logs_url,
        commit_sha: history.primary_revision().map(str::to_string),
        commit_message: None,
        branch: app.spec.primary_source().target_revision.clone(),
        actor: None,
        inputs: None,
        metadata,
    }
}

fn latest_run_number(app: &types::Application) -> Option<i64> {
    let operation = app.status.operation_state.as_ref();
    operation
        .map(|operation| operation.started_at.timestamp())
        .or_else(|| {
            let history = app.status.history.iter().flatten();
            history.map(|entry| entry.deployed_at.timestamp()).max()
        })
}

pub(crate) fn follows_live_state(app: &types::Application, run: &PipelineRun) -> bool {
    run.status == PipelineStatus::Success && latest_run_number(app) == Some(run.run_number)
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
        correlation: Correlation::default(),
        evidence: RunEvidence::default(),
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

#[cfg(test)]
mod tests;
