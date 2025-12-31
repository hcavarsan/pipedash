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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
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
    pub align: Option<String>,
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
