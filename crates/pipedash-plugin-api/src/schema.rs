use std::collections::BTreeMap;

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
    Token,
    SignIn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    pub fields: Vec<ConfigField>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub auth_modes: BTreeMap<String, AuthMode>,
}

impl ConfigSchema {
    pub fn new() -> Self {
        Self {
            fields: Vec::new(),
            auth_modes: BTreeMap::new(),
        }
    }

    pub fn add_field(mut self, field: ConfigField) -> Self {
        self.fields.push(field);
        self
    }

    pub fn add_auth_field(mut self, mode: AuthMode, field: ConfigField) -> Self {
        self.auth_modes.insert(field.key.clone(), mode);
        self.fields.push(field);
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
    fn auth_fields_serialize_their_mode_by_key() {
        let schema = ConfigSchema::new()
            .add_field(field("deck_url"))
            .add_auth_field(AuthMode::SignIn, field("sso_issuer"));
        let value = serde_json::to_value(&schema).unwrap();
        assert_eq!(value["auth_modes"], json!({"sso_issuer": "sign_in"}));
        assert_eq!(value["fields"].as_array().unwrap().len(), 2);

        let plain = serde_json::to_value(ConfigSchema::new()).unwrap();
        assert!(plain.get("auth_modes").is_none());
    }
}
