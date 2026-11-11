use std::collections::{
    BTreeMap,
    HashMap,
};

use pipedash_plugin_api::plugin::Plugin;
use pipedash_plugin_api::{
    AuthMode,
    AUTH_MODE_KEY,
};

use super::ProviderService;
use crate::domain::provider_draft::{
    ProviderDraft,
    ProviderDraftResult,
};
use crate::domain::{
    DomainError,
    DomainResult,
    ProviderConfig,
    SignInStatus,
    SIGN_IN_CONFIG_KEY,
};

mod rules;
#[cfg(test)]
mod tests;

impl ProviderService {
    pub async fn validate_draft(&self, draft: &ProviderDraft) -> DomainResult<ProviderDraftResult> {
        let plugin = self.draft_plugin(&draft.provider_type)?;
        let stored = self.draft_stored(draft).await?;

        Ok(self.check_draft(plugin, draft, stored.as_ref()).await)
    }

    pub async fn save_draft(&self, draft: ProviderDraft) -> DomainResult<i64> {
        let plugin = self.draft_plugin(&draft.provider_type)?;
        let stored = self.draft_stored(&draft).await?;
        let result = self.check_draft(plugin, &draft, stored.as_ref()).await;
        if let Some(message) = result.errors.values().next() {
            return Err(DomainError::InvalidConfig(message.clone()));
        }

        let config = rules::provider_config(
            plugin.metadata(),
            &draft,
            stored.as_ref(),
            result.selection_required,
        );
        if result.selection_required
            && stored
                .as_ref()
                .is_some_and(|stored| !plugin.selection_required(&stored.config))
            && !rules::has_selection(&config.config)
        {
            return Err(DomainError::InvalidConfig(rules::selection_error(
                plugin.metadata(),
            )));
        }
        match draft.provider_id {
            Some(id) => {
                self.update_provider(id, config).await?;
                Ok(id)
            }
            None => self.add_provider(config).await,
        }
    }

    fn draft_plugin(&self, provider_type: &str) -> DomainResult<&dyn Plugin> {
        self.plugin_registry.get(provider_type).ok_or_else(|| {
            DomainError::InvalidProviderType(format!("Unknown provider type: {provider_type}"))
        })
    }

    async fn draft_stored(&self, draft: &ProviderDraft) -> DomainResult<Option<ProviderConfig>> {
        match draft.provider_id {
            Some(id) => self.get_provider_config(id).await.map(Some),
            None => Ok(None),
        }
    }

    async fn check_draft(
        &self, plugin: &dyn Plugin, draft: &ProviderDraft, stored: Option<&ProviderConfig>,
    ) -> ProviderDraftResult {
        let metadata = plugin.metadata();
        let values = rules::effective_values(draft, stored);
        let selection_required = plugin.selection_required(&values);
        let mut errors = BTreeMap::new();
        let mut active_field_keys = Vec::new();
        let mut missing_field_labels = Vec::new();

        if draft.name.trim().is_empty() {
            errors.insert(rules::NAME_KEY.to_string(), rules::NAME_ERROR.to_string());
        }
        if !metadata.config_schema.allows(draft.auth_mode) {
            errors.insert(
                AUTH_MODE_KEY.to_string(),
                "Choose one of this provider's connection methods".to_string(),
            );
        }
        for field in metadata.config_schema.active_fields(draft.auth_mode) {
            let value = rules::value_of(&values, &field.key);
            active_field_keys.push(field.key.clone());
            if field.required && value.trim().is_empty() {
                missing_field_labels.push(field.label.clone());
            }
            if let Some(message) = rules::validate_field(field, value) {
                errors.insert(field.key.clone(), message);
            }
        }

        let sign_in_error = self.sign_in_error(draft.auth_mode, &values).await;
        let credentials_complete = missing_field_labels.is_empty() && sign_in_error.is_none();
        if let Some(message) = sign_in_error {
            errors.insert(SIGN_IN_CONFIG_KEY.to_string(), message.to_string());
        }

        if selection_required
            && draft
                .selected_pipeline_ids
                .as_ref()
                .is_some_and(|ids| !ids.iter().any(|id| !id.trim().is_empty()))
        {
            errors.insert(
                rules::SELECTION_KEY.to_string(),
                rules::selection_error(metadata),
            );
        }

        ProviderDraftResult {
            errors,
            active_field_keys,
            missing_field_labels,
            credentials_complete,
            selection_required,
            selection_needs_refresh: selection_required
                && (rules::selection_needs_refresh(draft, stored)
                    || (stored.is_some() && !rules::has_selection(&values))),
        }
    }

    async fn sign_in_error(
        &self, mode: AuthMode, values: &HashMap<String, String>,
    ) -> Option<&'static str> {
        if mode != AuthMode::SignIn {
            return None;
        }
        let reference = rules::value_of(values, SIGN_IN_CONFIG_KEY);
        if reference.is_empty() {
            return Some("Sign in to continue");
        }
        match self.sign_in_status(reference).await {
            Ok(SignInStatus::Completed { .. }) => None,
            _ => Some("Finish signing in to continue"),
        }
    }
}
