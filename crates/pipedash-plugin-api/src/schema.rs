use std::collections::{
    BTreeMap,
    HashMap,
};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    None,
    Token,
    SignIn,
}

pub const AUTH_MODE_KEY: &str = "auth_mode";

pub fn is_no_auth(config: &HashMap<String, String>) -> bool {
    config
        .get(AUTH_MODE_KEY)
        .is_some_and(|mode| mode.trim() == "none")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    pub fields: Vec<ConfigField>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub auth_modes: BTreeMap<String, Vec<AuthMode>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_auth: bool,
}

impl ConfigSchema {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            auth_modes: BTreeMap::new(),
            no_auth: false,
        }
    }

    pub fn add_field(mut self, field: ConfigField) -> Self {
        self.fields.push(field);
        self
    }

    pub fn add_auth_field(mut self, modes: &[AuthMode], field: ConfigField) -> Self {
        self.auth_modes.insert(field.key.clone(), modes.to_vec());
        self.fields.push(field);
        self
    }

    pub fn allow_no_auth(mut self) -> Self {
        self.no_auth = true;
        self
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
    pub sortable: bool,
    pub filterable: bool,
}

fn default_visible_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub columns: Vec<ColumnDefinition>,
    pub default_sort_column: Option<String>,
    pub default_sort_direction: Option<String>,
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
mod tests {
    use serde_json::json;

    use super::*;

    fn field(key: &str) -> ConfigField {
        ConfigField {
            key: key.to_string(),
            label: key.to_string(),
            description: None,
            field_type: ConfigFieldType::Text,
            required: false,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        }
    }

    #[test]
    fn auth_fields_serialize_their_modes_by_key() {
        let schema = ConfigSchema::new()
            .allow_no_auth()
            .add_field(field("deck_url"))
            .add_auth_field(&[AuthMode::SignIn], field("sso_issuer"))
            .add_auth_field(&[AuthMode::Token, AuthMode::SignIn], field("kubeconfig"));
        let value = serde_json::to_value(&schema).unwrap();
        assert_eq!(
            value["auth_modes"],
            json!({"sso_issuer": ["sign_in"], "kubeconfig": ["token", "sign_in"]})
        );
        assert_eq!(value["no_auth"], json!(true));
        assert_eq!(value["fields"].as_array().unwrap().len(), 3);

        let plain = serde_json::to_value(ConfigSchema::new()).unwrap();
        assert!(plain.get("auth_modes").is_none());
        assert!(plain.get("no_auth").is_none());
    }

    #[test]
    fn no_auth_is_read_from_the_auth_mode_key() {
        let config = |mode: &str| HashMap::from([(AUTH_MODE_KEY.to_string(), mode.to_string())]);
        assert!(is_no_auth(&config("none")));
        assert!(!is_no_auth(&config("token")));
        assert!(!is_no_auth(&HashMap::new()));
    }
}
