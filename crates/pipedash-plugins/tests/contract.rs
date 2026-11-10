#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use pipedash_core::infrastructure::config::{
    TokenRefError,
    TokenReference,
};
use pipedash_plugin_api::schema::ColumnSort;
use pipedash_plugin_api::{
    PipelineFacts,
    PipelineStatus,
    PluginMetadata,
    RunFacts,
};

const STATUSES: [PipelineStatus; 6] = [
    PipelineStatus::Success,
    PipelineStatus::Failed,
    PipelineStatus::Running,
    PipelineStatus::Pending,
    PipelineStatus::Cancelled,
    PipelineStatus::Skipped,
];

fn each_plugin(mut check: impl FnMut(&PluginMetadata, &dyn pipedash_plugin_api::Plugin)) {
    let registry = pipedash_plugins::registry();
    for metadata in registry.metadata() {
        let plugin = registry.create(&metadata.provider_type).unwrap();
        check(metadata, plugin.as_ref());
    }
}

fn pipeline(
    status: PipelineStatus, metadata: &HashMap<String, serde_json::Value>,
) -> PipelineFacts<'_> {
    PipelineFacts {
        id: "contract__1__pipeline",
        status,
        branch: Some("main"),
        workflow_file: None,
        resource_state: None,
        metadata,
    }
}

fn run(status: PipelineStatus, metadata: &HashMap<String, serde_json::Value>) -> RunFacts<'_> {
    RunFacts {
        run_number: 7,
        status,
        branch: Some("main"),
        actor: Some("someone"),
        commit_message: Some("change"),
        inputs: None,
        metadata,
    }
}

#[test]
fn metadata_survives_a_serde_round_trip() {
    each_plugin(|metadata, _| {
        let json = serde_json::to_value(metadata).unwrap();
        let decoded: PluginMetadata = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            json,
            "{}",
            metadata.provider_type
        );
    });
}

#[test]
fn config_schema_is_internally_consistent() {
    each_plugin(|metadata, _| {
        let schema = &metadata.config_schema;
        for field in &schema.fields {
            if let Some(pattern) = &field.validation_regex {
                assert!(
                    regex::Regex::new(pattern).is_ok(),
                    "{}: {}",
                    metadata.provider_type,
                    field.key
                );
            }
        }
        for key in schema.auth_modes.keys() {
            assert!(
                schema.fields.iter().any(|field| &field.key == key),
                "{}: auth mode for unknown field {key}",
                metadata.provider_type
            );
        }
    });
}

#[test]
fn pipeline_actions_never_exceed_declared_capabilities() {
    let empty = HashMap::new();
    each_plugin(|metadata, plugin| {
        let capabilities = &metadata.capabilities;
        for status in STATUSES {
            let actions = plugin.pipeline_actions(&pipeline(status, &empty));
            let provider = &metadata.provider_type;
            assert!(
                capabilities.trigger || !actions.trigger.allowed,
                "{provider}"
            );
            assert!(capabilities.cancel || !actions.cancel.allowed, "{provider}");
            assert!(
                capabilities.force_cancel || !actions.force_cancel.allowed,
                "{provider}"
            );
        }
    });
}

#[test]
fn run_actions_never_exceed_declared_capabilities() {
    let empty = HashMap::new();
    each_plugin(|metadata, plugin| {
        let capabilities = &metadata.capabilities;
        for status in STATUSES {
            let actions = plugin.run_actions(&pipeline(status, &empty), &run(status, &empty));
            let provider = &metadata.provider_type;
            assert!(capabilities.cancel || !actions.cancel.allowed, "{provider}");
            assert!(
                capabilities.force_cancel || !actions.force_cancel.allowed,
                "{provider}"
            );
            assert!(capabilities.trigger || !actions.rerun.allowed, "{provider}");
        }
    });
}

#[test]
fn every_run_has_a_title_and_started_label() {
    let empty = HashMap::new();
    each_plugin(|metadata, plugin| {
        for status in STATUSES {
            let display = plugin.run_display(&pipeline(status, &empty), &run(status, &empty));
            assert!(
                !display.title.trim().is_empty(),
                "{}",
                metadata.provider_type
            );
            assert!(
                !display.started_label.trim().is_empty(),
                "{}",
                metadata.provider_type
            );
        }
    });
}

#[test]
fn fresh_trigger_values_only_use_declared_parameters() {
    let empty = HashMap::new();
    each_plugin(|metadata, plugin| {
        let values = plugin.trigger_values(&pipeline(PipelineStatus::Success, &empty), None, &[]);
        assert!(values.is_empty(), "{}", metadata.provider_type);
    });
}

#[test]
fn an_empty_config_selects_no_repositories() {
    each_plugin(|metadata, plugin| {
        assert!(
            plugin.configured_repositories(&HashMap::new()).is_empty(),
            "{}",
            metadata.provider_type
        );
    });
}

#[test]
fn declared_token_prefixes_are_rejected_as_plain_text() {
    let registry = pipedash_plugins::registry();
    for metadata in registry.metadata() {
        for prefix in &metadata.token_prefixes {
            let token = format!("{prefix}contractTokenValue123");
            assert!(
                matches!(
                    TokenReference::parse(&token, &registry),
                    Err(TokenRefError::PlainTextToken(_))
                ),
                "{}: {prefix}",
                metadata.provider_type
            );
        }
    }
}

fn run_sample() -> serde_json::Value {
    let run: pipedash_core::domain::PipelineRun = serde_json::from_value(serde_json::json!({
        "id": "", "pipeline_id": "", "run_number": 1, "status": "success",
        "started_at": "2026-01-01T00:00:00Z", "logs_url": ""
    }))
    .unwrap();
    serde_json::to_value(run).unwrap()
}

fn sorts_by_a_run_value(sample: &serde_json::Value, field_path: &str) -> bool {
    if let Some(key) = field_path.strip_prefix("metadata.") {
        return !key.is_empty();
    }
    field_path
        .split('.')
        .try_fold(sample, |current, part| current.as_object()?.get(part))
        .is_some_and(|leaf| !leaf.is_object() && !leaf.is_array())
}

#[test]
fn sort_field_check_rejects_paths_core_cannot_sort() {
    let sample = run_sample();
    for path in [
        "run_number",
        "concluded_at",
        "pace.live",
        "metadata.build_id",
    ] {
        assert!(sorts_by_a_run_value(&sample, path), "{path}");
    }
    for path in [
        "duration_second",
        "branch.typo",
        "actions",
        "actions.typo",
        "display.context",
        "metadata",
        "metadata.",
    ] {
        assert!(!sorts_by_a_run_value(&sample, path), "{path}");
    }
}

#[test]
fn run_tables_name_one_history_order_and_sort_by_real_fields() {
    let sample = run_sample();
    each_plugin(|metadata, _| {
        let provider = &metadata.provider_type;
        for table in &metadata.table_schema.tables {
            let mut ids: Vec<_> = table.columns.iter().map(|column| &column.id).collect();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(
                ids.len(),
                table.columns.len(),
                "{provider}: {} repeats a column id",
                table.id
            );
            let orders = table
                .columns
                .iter()
                .filter(|column| column.sort == ColumnSort::HistoryOrder)
                .count();
            assert_eq!(
                orders,
                usize::from(table.id == "pipeline_runs"),
                "{provider}: history-order columns in {}",
                table.id
            );
        }
        let runs = metadata.table_schema.get_table("pipeline_runs").unwrap();
        for column in &runs.columns {
            assert!(
                column.sort != ColumnSort::Value
                    || sorts_by_a_run_value(&sample, &column.field_path),
                "{provider}: {} sorts by unknown field {}",
                column.id,
                column.field_path
            );
        }
    });
}
