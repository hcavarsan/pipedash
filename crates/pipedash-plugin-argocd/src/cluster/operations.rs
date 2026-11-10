use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use serde_json::{
    json,
    Map,
    Value,
};

use super::ACTOR;
use crate::types::{
    SyncRequest,
    SyncStrategy,
};

fn operation_in_progress(app: &Value) -> bool {
    app.get("operation")
        .is_some_and(|operation| !operation.is_null())
        || matches!(
            app.pointer("/status/operationState/phase")
                .and_then(Value::as_str),
            Some("Running" | "Terminating")
        )
}

fn auto_sync_enabled(app: &Value) -> bool {
    app.pointer("/spec/syncPolicy/automated")
        .is_some_and(|automated| {
            automated.is_object() && automated.get("enabled") != Some(&Value::Bool(false))
        })
}

fn strategy_json(strategy: &SyncStrategy) -> Value {
    let mut out = Map::new();
    if let Some(apply) = &strategy.apply {
        out.insert("apply".to_string(), json!({ "force": apply.force }));
    }
    if let Some(hook) = &strategy.hook {
        out.insert("hook".to_string(), json!({ "force": hook.force }));
    }
    Value::Object(out)
}

pub(super) fn sync_patch(app: &Value, name: &str, request: &SyncRequest) -> PluginResult<Value> {
    if app
        .pointer("/metadata/deletionTimestamp")
        .is_some_and(|stamp| !stamp.is_null())
    {
        return Err(PluginError::InvalidConfig(format!(
            "Application {name} is being deleted"
        )));
    }
    if operation_in_progress(app) {
        return Err(PluginError::ApiError(format!(
            "Another operation is already in progress for {name}"
        )));
    }

    let revision = request
        .revision
        .as_deref()
        .filter(|revision| !revision.is_empty());
    if let Some(revision) = revision.filter(|_| auto_sync_enabled(app)) {
        let target = app
            .pointer("/spec/sources/0/targetRevision")
            .or_else(|| app.pointer("/spec/source/targetRevision"))
            .and_then(Value::as_str)
            .filter(|target| !target.is_empty())
            .unwrap_or("HEAD");
        if revision != target {
            return Err(PluginError::InvalidConfig(format!(
                "Cannot sync to {revision}: auto-sync currently set to {target}"
            )));
        }
    }

    let mut sync = json!({
        "prune": request.prune.unwrap_or(false),
        "dryRun": request.dry_run.unwrap_or(false),
    });
    if let Some(revision) = revision {
        sync["revision"] = json!(revision);
    }
    if let Some(strategy) = &request.strategy {
        sync["syncStrategy"] = strategy_json(strategy);
    }
    let copied = |pointer: &str| {
        app.pointer(pointer)
            .filter(|value| match value {
                Value::Array(items) => !items.is_empty(),
                Value::Object(fields) => !fields.is_empty(),
                _ => false,
            })
            .cloned()
    };
    if let Some(sources) = copied("/spec/sources") {
        sync["sources"] = sources;
    }
    if let Some(options) = copied("/spec/syncPolicy/syncOptions") {
        sync["syncOptions"] = options;
    }
    let mut operation = json!({ "initiatedBy": { "username": ACTOR }, "sync": sync });
    if let Some(retry) = copied("/spec/syncPolicy/retry") {
        operation["retry"] = retry;
    }

    Ok(json!({
        "metadata": { "resourceVersion": app.pointer("/metadata/resourceVersion") },
        "operation": operation,
    }))
}

pub(super) fn terminate_patch(app: &Value, name: &str) -> PluginResult<Value> {
    let running = app
        .get("operation")
        .is_some_and(|operation| !operation.is_null())
        && app.pointer("/status/operationState").is_some();
    if !running {
        return Err(PluginError::InvalidConfig(format!(
            "No operation is in progress for {name}"
        )));
    }

    Ok(json!({
        "metadata": { "resourceVersion": app.pointer("/metadata/resourceVersion") },
        "status": { "operationState": { "phase": "Terminating" } },
    }))
}

#[cfg(test)]
mod tests;
