use std::collections::{
    HashMap,
    HashSet,
};

use pipedash_plugin_api::{
    AuthMode,
    ConfigField,
    ConfigSchema,
    ConfigScope,
    PluginMetadata,
    AUTH_MODE_KEY,
};
use regex::RegexBuilder;

use crate::domain::provider_draft::ProviderDraft;
use crate::domain::{
    ProviderConfig,
    SIGN_IN_CONFIG_KEY,
};

pub(super) const NAME_KEY: &str = "providerName";
pub(super) const NAME_ERROR: &str = "Provider name is required";
pub(super) const SELECTION_KEY: &str = "selected_items";
const TOKEN_KEY: &str = "token";
const DEFAULT_REFRESH_INTERVAL: i64 = 30;
const REGEX_SIZE_LIMIT: usize = 1 << 20;
const REGEX_NEST_LIMIT: u32 = 64;

fn key_in_mode(schema: &ConfigSchema, key: &str, mode: AuthMode) -> bool {
    schema.fields.iter().all(|field| field.key != key)
        || schema.active_fields(mode).any(|field| field.key == key)
}

pub(super) fn value_of<'a>(values: &'a HashMap<String, String>, key: &str) -> &'a str {
    values.get(key).map_or("", String::as_str)
}

pub(super) fn validate_field(field: &ConfigField, value: &str) -> Option<String> {
    if field.required && value.trim().is_empty() {
        return Some(format!("{} is required", field.label));
    }
    let pattern = field
        .validation_regex
        .as_deref()
        .filter(|p| !p.is_empty())?;
    if value.is_empty() {
        return None;
    }
    let Ok(regex) = RegexBuilder::new(pattern)
        .size_limit(REGEX_SIZE_LIMIT)
        .dfa_size_limit(REGEX_SIZE_LIMIT)
        .nest_limit(REGEX_NEST_LIMIT)
        .build()
    else {
        return Some(format!(
            "The validation rule for {} is invalid. Contact the plugin author.",
            field.label
        ));
    };
    if regex.is_match(value) {
        return None;
    }
    Some(
        field
            .validation_message
            .clone()
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| format!("Invalid format for {}", field.label)),
    )
}

pub(super) fn has_selection(values: &HashMap<String, String>) -> bool {
    pipedash_plugin_api::config::comma_separated(value_of(values, SELECTION_KEY))
        .next()
        .is_some()
}

pub(super) fn selection_error(metadata: &PluginMetadata) -> String {
    format!(
        "Please select at least one {}",
        metadata.presentation.terminology.item
    )
}

pub(super) fn normalize_ids(ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty() && seen.insert(*id))
        .map(str::to_owned)
        .collect()
}

fn scope_fields(
    schema: &ConfigSchema, matches: impl Fn(ConfigScope) -> bool,
) -> impl Iterator<Item = &ConfigField> {
    schema
        .fields
        .iter()
        .filter(move |field| field.scope.is_some_and(&matches))
}

fn scope_moved(
    schema: &ConfigSchema, values: &HashMap<String, String>, stored: &ProviderConfig,
    matches: impl Fn(ConfigScope) -> bool,
) -> bool {
    scope_fields(schema, matches)
        .any(|field| value_of(values, &field.key) != value_of(&stored.config, &field.key))
}

fn scope_changed(
    schema: &ConfigSchema, values: &HashMap<String, String>, stored: &ProviderConfig,
) -> bool {
    scope_moved(schema, values, stored, |_| true)
}

pub(super) fn selection_needs_refresh(
    schema: &ConfigSchema, draft: &ProviderDraft, stored: Option<&ProviderConfig>,
) -> bool {
    let Some(stored) = stored else {
        return false;
    };
    if value_of(&stored.config, SELECTION_KEY).trim().is_empty() {
        return false;
    }
    let carries_selection = draft
        .selected_pipeline_ids
        .as_ref()
        .is_some_and(|ids| !ids.is_empty())
        || !value_of(&draft.values, SELECTION_KEY).trim().is_empty();

    !carries_selection || scope_changed(schema, &draft.values, stored)
}

pub(super) fn effective_values(
    schema: &ConfigSchema, draft: &ProviderDraft, stored: Option<&ProviderConfig>,
) -> HashMap<String, String> {
    let mut values = draft.values.clone();
    let Some(stored) = stored.filter(|stored| scope_changed(schema, &values, stored)) else {
        return values;
    };
    values.remove(SELECTION_KEY);
    let is_context = |scope| scope == ConfigScope::Context;
    let stale_context = scope_moved(schema, &values, stored, |scope| {
        scope == ConfigScope::Cluster
    }) && !scope_moved(schema, &values, stored, is_context);
    if !stale_context {
        return values;
    }
    for field in scope_fields(schema, is_context) {
        if let Some(value) = values.get_mut(&field.key) {
            value.clear();
        }
    }
    values
}

pub(super) fn provider_config(
    metadata: &PluginMetadata, draft: &ProviderDraft, stored: Option<&ProviderConfig>,
    selection_required: bool,
) -> ProviderConfig {
    let mode = draft.auth_mode;
    let schema = &metadata.config_schema;
    let mut values = effective_values(schema, draft, stored);
    values.retain(|key, _| key_in_mode(schema, key, mode));

    let token = values.remove(TOKEN_KEY).unwrap_or_default();
    if mode != AuthMode::SignIn {
        values.remove(SIGN_IN_CONFIG_KEY);
    }
    if schema.auth_options.is_empty() {
        values.remove(AUTH_MODE_KEY);
    } else {
        values.insert(AUTH_MODE_KEY.to_string(), mode.as_str().to_string());
    }

    let kept = values.remove(SELECTION_KEY);
    let selected = draft.selected_pipeline_ids.as_deref().map(normalize_ids);
    let selection = match selected {
        Some(ids) if selection_required || !ids.is_empty() => Some(ids.join(",")),
        _ => kept.or_else(|| {
            stored
                .filter(|stored| {
                    !selection_required && !scope_changed(schema, &draft.values, stored)
                })
                .and_then(|stored| stored.config.get(SELECTION_KEY))
                .cloned()
        }),
    };
    if let Some(selection) = selection.filter(|selection| !selection.trim().is_empty()) {
        values.insert(SELECTION_KEY.to_string(), selection);
    }

    ProviderConfig {
        id: draft.provider_id,
        name: draft.name.clone(),
        provider_type: draft.provider_type.clone(),
        token,
        config: values,
        refresh_interval: stored.map_or(DEFAULT_REFRESH_INTERVAL, |stored| stored.refresh_interval),
        version: stored.and_then(|stored| stored.version),
    }
}
