use std::collections::{
    HashMap,
    HashSet,
};

use pipedash_plugin_api::{
    AuthMode,
    ConfigField,
    ConfigSchema,
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
const CONTEXT_KEY: &str = "context";
const KUBECONFIG_KEY: &str = "kubeconfig";
const SCOPE_KEYS: [&str; 3] = [KUBECONFIG_KEY, CONTEXT_KEY, "namespaces"];
const DEFAULT_REFRESH_INTERVAL: i64 = 30;
const REGEX_SIZE_LIMIT: usize = 1 << 20;
const REGEX_NEST_LIMIT: u32 = 64;

fn in_mode(schema: &ConfigSchema, key: &str, mode: AuthMode) -> bool {
    schema
        .auth_modes
        .get(key)
        .is_none_or(|modes| modes.contains(&mode))
}

pub(super) fn value_of<'a>(values: &'a HashMap<String, String>, key: &str) -> &'a str {
    values.get(key).map_or("", String::as_str)
}

pub(super) fn active_fields(
    schema: &ConfigSchema, mode: AuthMode,
) -> impl Iterator<Item = &ConfigField> {
    schema.fields.iter().filter(move |field| {
        (field.key != TOKEN_KEY || mode == AuthMode::Token) && in_mode(schema, &field.key, mode)
    })
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

fn scope_changed(values: &HashMap<String, String>, stored: &ProviderConfig) -> bool {
    SCOPE_KEYS
        .iter()
        .any(|key| value_of(values, key) != value_of(&stored.config, key))
}

pub(super) fn selection_needs_refresh(
    draft: &ProviderDraft, stored: Option<&ProviderConfig>,
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

    !carries_selection || scope_changed(&draft.values, stored)
}

pub(super) fn effective_values(
    draft: &ProviderDraft, stored: Option<&ProviderConfig>,
) -> HashMap<String, String> {
    let mut values = draft.values.clone();
    let Some(stored) = stored.filter(|stored| scope_changed(&values, stored)) else {
        return values;
    };
    values.remove(SELECTION_KEY);
    let moved = |key: &str| value_of(&values, key) != value_of(&stored.config, key);
    let stale_context = moved(KUBECONFIG_KEY) && !moved(CONTEXT_KEY);
    if stale_context && values.contains_key(CONTEXT_KEY) {
        values.insert(CONTEXT_KEY.to_string(), String::new());
    }
    values
}

pub(super) fn provider_config(
    metadata: &PluginMetadata, draft: &ProviderDraft, stored: Option<&ProviderConfig>,
    selection_required: bool,
) -> ProviderConfig {
    let mode = draft.auth_mode;
    let mut values = effective_values(draft, stored);
    values.retain(|key, _| in_mode(&metadata.config_schema, key, mode));

    let token = values.remove(TOKEN_KEY).unwrap_or_default();
    let token = if mode == AuthMode::Token {
        token
    } else {
        String::new()
    };
    if mode != AuthMode::SignIn {
        values.remove(SIGN_IN_CONFIG_KEY);
    }
    if mode == AuthMode::None {
        values.insert(AUTH_MODE_KEY.to_string(), "none".to_string());
    } else {
        values.remove(AUTH_MODE_KEY);
    }

    let kept = values.remove(SELECTION_KEY);
    let selected = draft.selected_pipeline_ids.as_deref().map(normalize_ids);
    let selection = match selected {
        Some(ids) if selection_required || !ids.is_empty() => Some(ids.join(",")),
        _ => kept.or_else(|| {
            stored
                .filter(|stored| !selection_required && !scope_changed(&draft.values, stored))
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
