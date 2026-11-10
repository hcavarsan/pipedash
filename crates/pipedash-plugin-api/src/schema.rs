use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigFieldType {
    Text,
    Password,
    TextArea,
    Boolean,
    Select,
    MultiSelect,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSection {
    Authentication,
    Connection,
    Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigField {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub field_type: ConfigFieldType,
    pub required: bool,
    pub default_value: Option<serde_json::Value>,
    pub options: Option<Vec<String>>,
    pub validation_regex: Option<String>,
    pub validation_message: Option<String>,
    pub section: ConfigSection,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auth_modes: Vec<AuthMode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    None,
    Token,
    SignIn,
    Kubeconfig,
}

pub const AUTH_MODE_KEY: &str = "auth_mode";

impl AuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Token => "token",
            Self::SignIn => "sign_in",
            Self::Kubeconfig => "kubeconfig",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::None, Self::Token, Self::SignIn, Self::Kubeconfig]
            .into_iter()
            .find(|mode| mode.as_str() == value.trim())
    }

    pub fn from_config(config: &HashMap<String, String>) -> Option<Self> {
        config
            .get(AUTH_MODE_KEY)
            .and_then(|value| Self::parse(value))
    }
}

pub fn is_no_auth(config: &HashMap<String, String>) -> bool {
    AuthMode::from_config(config) == Some(AuthMode::None)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthOption {
    pub mode: AuthMode,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    pub fields: Vec<ConfigField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auth_options: Vec<AuthOption>,
}

impl ConfigSchema {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            auth_options: Vec::new(),
        }
    }

    pub fn add_field(mut self, field: ConfigField) -> Self {
        self.fields.push(field);
        self
    }

    pub fn add_auth_field(mut self, modes: &[AuthMode], mut field: ConfigField) -> Self {
        field.auth_modes = modes.to_vec();
        self.fields.push(field);
        self
    }

    pub fn auth_option(mut self, option: AuthOption) -> Self {
        self.auth_options.push(option);
        self
    }

    pub fn default_mode(&self) -> AuthMode {
        self.auth_options
            .first()
            .map_or(AuthMode::Token, |option| option.mode)
    }

    pub fn allows(&self, mode: AuthMode) -> bool {
        if self.auth_options.is_empty() {
            return mode == AuthMode::Token;
        }
        self.auth_options.iter().any(|option| option.mode == mode)
    }

    pub fn active_fields(&self, mode: AuthMode) -> impl Iterator<Item = &ConfigField> {
        self.fields
            .iter()
            .filter(move |field| field.auth_modes.is_empty() || field.auth_modes.contains(&mode))
    }
}

impl Default for ConfigSchema {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ColumnDataType {
    String,
    Number,
    DateTime,
    Duration,
    Status,
    Badge,
    Url,
    Json,
    Boolean,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CellRenderer {
    Text,
    Badge,
    DateTime,
    Duration,
    StatusBadge,
    Commit,
    Avatar,
    TruncatedText,
    Link,
    JsonViewer,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ColumnVisibility {
    Always,
    WhenPresent,
    WhenCapability(String),
    Conditional {
        field: String,
        equals: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub field_path: String,
    pub data_type: ColumnDataType,
    pub renderer: CellRenderer,
    pub visibility: ColumnVisibility,
    #[serde(default = "default_visible_true")]
    pub default_visible: bool,
    pub width: Option<u32>,
    pub sort: ColumnSort,
    pub filterable: bool,
}

fn default_visible_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnSort {
    #[default]
    None,
    Value,
    HistoryOrder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub columns: Vec<ColumnDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    pub tables: Vec<TableDefinition>,
}

impl TableSchema {
    pub fn new() -> Self {
        Self { tables: Vec::new() }
    }

    pub fn add_table(mut self, table: TableDefinition) -> Self {
        self.tables.push(table);
        self
    }

    pub fn get_table(&self, table_id: &str) -> Option<&TableDefinition> {
        self.tables.iter().find(|t| t.id == table_id)
    }
}

impl Default for TableSchema {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
