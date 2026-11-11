use std::collections::{
    BTreeMap,
    HashMap,
};
use std::fmt;

use pipedash_plugin_api::AuthMode;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderDraft {
    pub provider_type: String,
    pub name: String,
    pub auth_mode: AuthMode,
    pub values: HashMap<String, String>,
    pub selected_pipeline_ids: Option<Vec<String>>,
    pub provider_id: Option<i64>,
}

impl fmt::Debug for ProviderDraft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut value_keys: Vec<&String> = self.values.keys().collect();
        value_keys.sort();
        f.debug_struct("ProviderDraft")
            .field("provider_type", &self.provider_type)
            .field("name", &self.name)
            .field("auth_mode", &self.auth_mode)
            .field("value_keys", &value_keys)
            .field("selected_pipeline_ids", &self.selected_pipeline_ids)
            .field("provider_id", &self.provider_id)
            .finish()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDraftResult {
    pub errors: BTreeMap<String, String>,
    pub active_field_keys: Vec<String>,
    pub missing_field_labels: Vec<String>,
    pub credentials_complete: bool,
    pub selection_required: bool,
    pub selection_needs_refresh: bool,
}
