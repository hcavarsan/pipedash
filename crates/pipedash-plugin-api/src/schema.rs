use serde::{
    Deserialize,
    Serialize,
};

/// Configuration field type for schema-based UI generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigFieldType {
    /// Single-line text input
    Text,
    /// Password input (hidden)
    Password,
    /// Multi-line text area
    TextArea,
    /// Boolean checkbox
    Boolean,
    /// Single selection dropdown
    Select,
    /// Multiple selection
    MultiSelect,
    /// Number input
    Number,
}

/// A single configuration field definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigField {
    /// Field key (used in config HashMap)
    pub key: String,
    /// Human-readable label
    pub label: String,
    /// Field description/help text
    pub description: Option<String>,
    /// Field type
    pub field_type: ConfigFieldType,
    /// Whether the field is required
    pub required: bool,
    /// Default value (as JSON)
    pub default_value: Option<serde_json::Value>,
    /// Options for Select/MultiSelect types
    pub options: Option<Vec<String>>,
    /// Validation regex (optional)
    pub validation_regex: Option<String>,
    /// Validation error message
    pub validation_message: Option<String>,
}

/// Complete configuration schema for a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    /// Schema fields
    pub fields: Vec<ConfigField>,
}

impl ConfigSchema {
    pub fn new() -> Self {
        Self { fields: Vec::new() }
    }

    pub fn add_field(mut self, field: ConfigField) -> Self {
        self.fields.push(field);
        self
    }
}

impl Default for ConfigSchema {
    fn default() -> Self {
        Self::new()
    }
}
