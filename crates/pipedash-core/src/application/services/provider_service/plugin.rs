use std::collections::HashMap;
use std::sync::Arc;

use pipedash_plugin_api::{
    Plugin as PluginTrait,
    PluginRegistry,
    CONTROLLER_HEALTH_FEATURE,
};

use super::ProviderService;
use crate::domain::{
    DomainError,
    DomainResult,
    Provider,
    ProviderConfig,
    SignInCredential,
};
use crate::infrastructure::config::token_ref::TokenReference;
use crate::infrastructure::providers::PluginAdapter;
use crate::infrastructure::HttpClientManager;

impl ProviderService {
    pub fn available_plugins(
        registry: &PluginRegistry,
    ) -> Vec<pipedash_plugin_api::PluginMetadata> {
        let mut metadata: Vec<_> = registry.metadata().cloned().collect();
        metadata.sort_by_cached_key(|plugin| plugin.name.to_lowercase());
        metadata
    }

    pub fn list_available_plugins(&self) -> Vec<pipedash_plugin_api::PluginMetadata> {
        Self::available_plugins(&self.plugin_registry)
    }

    pub fn retains_run_history(&self, provider_type: &str) -> bool {
        self.plugin_registry
            .get(provider_type)
            .is_some_and(|plugin| plugin.metadata().capabilities.retains_run_history)
    }

    pub(crate) fn run_history_policy(
        &self, provider_type: &str,
    ) -> pipedash_plugin_api::RunHistoryPolicy {
        self.plugin_registry
            .get(provider_type)
            .map(|plugin| plugin.metadata().run_history)
            .unwrap_or_default()
    }

    pub async fn init_plugin(
        &self, provider_type: &str, mut config: HashMap<String, String>, provider_id: i64,
    ) -> DomainResult<Box<dyn PluginTrait>> {
        self.resolve_sign_in(&mut config).await?;
        init_plugin(
            &self.plugin_registry,
            &self.http_client_manager,
            provider_type,
            config,
            provider_id,
        )
    }

    pub async fn field_options(
        &self, provider_type: &str, field_key: &str, mut config: HashMap<String, String>,
    ) -> DomainResult<Vec<String>> {
        self.resolve_sign_in(&mut config).await?;
        let (mut plugin, client) = build_plugin(
            &self.plugin_registry,
            &self.http_client_manager,
            provider_type,
            &config,
        )?;
        let init_error = plugin.initialize(0, config.clone(), Some(client)).err();

        match plugin.get_field_options(field_key, &config).await {
            Ok(options) => Ok(options),
            Err(e) => Err(match init_error {
                Some(init) => {
                    DomainError::InvalidConfig(format!("Failed to initialize plugin: {init}"))
                }
                None => PluginAdapter::map_error(e),
            }),
        }
    }
}

pub(super) fn build_plugin(
    registry: &PluginRegistry, http: &HttpClientManager, provider_type: &str,
    config: &HashMap<String, String>,
) -> DomainResult<(Box<dyn PluginTrait>, Arc<reqwest::Client>)> {
    let plugin = registry.create(provider_type).ok_or_else(|| {
        DomainError::InvalidProviderType(format!("Unknown provider type: {}", provider_type))
    })?;
    let client = http.client_for(&plugin.client_options(config))?;

    Ok((plugin, client))
}

pub(super) async fn keep_selection_with_ci(
    plugin: &dyn PluginTrait, config: &mut ProviderConfig,
) -> DomainResult<()> {
    let Some(selected) = config.config.get("selected_items") else {
        return Ok(());
    };
    let ids: Vec<String> = selected
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let Ok(kept) = plugin.ids_with_ci(ids.clone()).await else {
        return Ok(());
    };
    if kept.is_empty() {
        return Err(DomainError::InvalidConfig(
            "None of the selected repositories has a CI file or a pipeline run yet".to_string(),
        ));
    }
    if kept.len() < ids.len() {
        tracing::info!(
            selected = ids.len(),
            kept = kept.len(),
            "Skipped selected repositories without a CI file or a pipeline run"
        );
        config
            .config
            .insert("selected_items".to_string(), kept.join(","));
    }
    Ok(())
}

fn init_plugin(
    registry: &PluginRegistry, http: &HttpClientManager, provider_type: &str,
    config: HashMap<String, String>, provider_id: i64,
) -> DomainResult<Box<dyn PluginTrait>> {
    let (mut plugin, client) = build_plugin(registry, http, provider_type, &config)?;
    plugin
        .initialize(provider_id, config, Some(client))
        .map_err(|e| DomainError::InvalidConfig(format!("Failed to initialize plugin: {e}")))?;

    Ok(plugin)
}

pub(crate) fn controller_health(registry: &PluginRegistry, provider_type: &str) -> bool {
    registry.get(provider_type).is_some_and(|plugin| {
        plugin
            .metadata()
            .features
            .iter()
            .any(|feature| feature.id == CONTROLLER_HEALTH_FEATURE)
    })
}

pub(crate) fn plugin_provider(
    registry: &PluginRegistry, http: &HttpClientManager, config: &ProviderConfig,
) -> DomainResult<Arc<dyn Provider>> {
    if !registry.is_registered(&config.provider_type) {
        return Err(DomainError::InvalidProviderType(format!(
            "Plugin not found for provider type: {}",
            config.provider_type
        )));
    }

    let provider_id = config.id.unwrap_or(0);
    let (token, renew_at) = match SignInCredential::decode(&config.token) {
        Some(credential) => {
            let renew_at = credential.renew_at();
            (credential.token, renew_at)
        }
        None => (
            resolve_token_reference(provider_id, &config.token, registry),
            None,
        ),
    };
    let mut plugin_config = config.config.clone();
    plugin_config.insert("token".to_string(), token);

    let plugin = init_plugin(
        registry,
        http,
        &config.provider_type,
        plugin_config,
        provider_id,
    )?;

    Ok(Arc::new(PluginAdapter::new(
        plugin,
        config.provider_type.clone(),
        provider_id,
        renew_at,
    )))
}

fn resolve_token_reference(provider_id: i64, token: &str, registry: &PluginRegistry) -> String {
    if token.is_empty() {
        return String::new();
    }
    match TokenReference::parse(token, registry) {
        Ok(TokenReference::EnvVar(var_name)) => std::env::var(&var_name).unwrap_or_else(|_| {
            tracing::warn!(
                provider_id = provider_id,
                var_name = %var_name,
                "Environment variable not found, using empty token"
            );
            String::new()
        }),
        Ok(TokenReference::None) => String::new(),
        Ok(_) | Err(_) => token.to_string(),
    }
}
