use std::collections::{
    BTreeMap,
    HashMap,
};

use pipedash_plugin_api::{
    AuthMode,
    PluginMetadata,
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
        let metadata = self.draft_metadata(&draft.provider_type)?;
        let stored = self.draft_stored(draft).await?;

        Ok(self.check_draft(metadata, draft, stored.as_ref()).await)
    }

    pub async fn save_draft(&self, draft: ProviderDraft) -> DomainResult<i64> {
        let metadata = self.draft_metadata(&draft.provider_type)?;
        let stored = self.draft_stored(&draft).await?;
        let result = self.check_draft(metadata, &draft, stored.as_ref()).await;
        if let Some(message) = result.errors.values().next() {
            return Err(DomainError::InvalidConfig(message.clone()));
        }

        let config = rules::provider_config(metadata, &draft, stored.as_ref());
        match draft.provider_id {
            Some(id) => {
                self.update_provider(id, config).await?;
                Ok(id)
            }
            None => self.add_provider(config).await,
        }
    }

    fn draft_metadata(&self, provider_type: &str) -> DomainResult<&PluginMetadata> {
        self.plugin_registry
            .get(provider_type)
            .map(|plugin| plugin.metadata())
            .ok_or_else(|| {
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
        &self, metadata: &PluginMetadata, draft: &ProviderDraft, stored: Option<&ProviderConfig>,
    ) -> ProviderDraftResult {
        let values = rules::effective_values(draft, stored);
        let mut errors = BTreeMap::new();
        let mut active_field_keys = Vec::new();
        let mut missing_field_labels = Vec::new();

        if draft.name.trim().is_empty() {
            errors.insert(rules::NAME_KEY.to_string(), rules::NAME_ERROR.to_string());
        }
        for field in rules::active_fields(&metadata.config_schema, draft.auth_mode) {
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

        if let Some(ids) = &draft.selected_pipeline_ids {
            if rules::normalize_ids(ids).is_empty() {
                errors.insert(
                    rules::SELECTION_KEY.to_string(),
                    format!(
                        "Please select at least one {}",
                        metadata.presentation.terminology.item
                    ),
                );
            }
        }

        ProviderDraftResult {
            errors,
            active_field_keys,
            missing_field_labels,
            credentials_complete,
            selection_needs_refresh: rules::selection_needs_refresh(draft, stored),
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
