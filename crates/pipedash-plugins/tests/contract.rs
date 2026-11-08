#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use pipedash_core::infrastructure::config::{
    TokenRefError,
    TokenReference,
};
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
