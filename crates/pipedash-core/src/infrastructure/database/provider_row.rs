use std::collections::HashMap;
use std::fmt::Display;

use crate::domain::{
    DomainError,
    DomainResult,
    ProviderConfig,
};

pub(super) fn failed<E: Display>(action: &'static str) -> impl Fn(E) -> DomainError {
    move |error| DomainError::DatabaseError(format!("Failed to {action}: {error}"))
}

pub(super) fn serialize_config(config: &ProviderConfig) -> DomainResult<String> {
    serde_json::to_string(&config.config).map_err(failed("serialize config"))
}

pub(super) fn ensure_unique(existing: Option<i64>, name: &str) -> DomainResult<()> {
    match existing {
        Some(_) => Err(DomainError::InvalidConfig(format!(
            "Provider '{name}' already exists"
        ))),
        None => Ok(()),
    }
}

pub(super) fn ensure_found(rows_affected: u64, id: i64) -> DomainResult<()> {
    match rows_affected {
        0 => Err(DomainError::ProviderNotFound(format!(
            "Provider {id} not found"
        ))),
        _ => Ok(()),
    }
}

pub(super) fn provider_config(
    id: i64, name: String, provider_type: String, token: String, config_json: &str,
    refresh_interval: i64, version: i64,
) -> DomainResult<ProviderConfig> {
    let config: HashMap<String, String> = if config_json.trim().is_empty() {
        HashMap::new()
    } else {
        serde_json::from_str(config_json).map_err(|e| {
            DomainError::DatabaseError(format!("Failed to parse config for {}: {}", id, e))
        })?
    };

    Ok(ProviderConfig {
        id: Some(id),
        name,
        provider_type,
        config,
        token,
        refresh_interval,
        version: Some(version),
    })
}
