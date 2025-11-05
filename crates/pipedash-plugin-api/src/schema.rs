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

// ============================================================================
// Table Schema Types
// ============================================================================

/// Data type for a table column
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ColumnDataType {
    /// Plain text string
    String,
    /// Numeric value
    Number,
    /// ISO 8601 datetime
    DateTime,
    /// Duration in seconds
    Duration,
    /// Status enum value
    Status,
    /// Badge/tag display
    Badge,
    /// URL/link
    Url,
    /// JSON object
    Json,
    /// Boolean value
    Boolean,
    /// Custom type (requires custom renderer)
    Custom(String),
}

/// How to render a column's cell content
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CellRenderer {
    /// Plain text
    Text,
    /// Badge component
    Badge,
    /// Formatted datetime
    DateTime,
    /// Formatted duration (e.g., "2h 30m")
    Duration,
    /// Status badge with icon
    StatusBadge,
    /// Commit hash with copy button
    Commit,
    /// Avatar with name
    Avatar,
    /// Truncated text with tooltip
    TruncatedText,
    /// Link/URL
    Link,
    /// JSON viewer
    JsonViewer,
    /// Custom renderer (requires frontend implementation)
    Custom(String),
}

/// Column visibility rules
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ColumnVisibility {
    /// Always show this column
    Always,
    /// Show only if the field has a non-null value
    WhenPresent,
    /// Show only if plugin has a specific capability
    WhenCapability(String),
    /// Conditional based on metadata
    Conditional {
        /// Field path to check (e.g., "metadata.has_git")
        field: String,
        /// Expected value
        equals: serde_json::Value,
    },
}

/// Definition of a single table column
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefinition {
    /// Unique column ID (used for sorting, filtering)
    pub id: String,
    /// Human-readable column header
    pub label: String,
    /// Description/tooltip for column
    pub description: Option<String>,
    /// Path to field in data object (supports nested like "metadata.namespace")
    pub field_path: String,
    /// Data type of the column
    pub data_type: ColumnDataType,
    /// How to render the cell
    pub renderer: CellRenderer,
    /// Visibility rules
    pub visibility: ColumnVisibility,
    /// Whether column is visible by default (when no user preference exists)
    #[serde(default = "default_visible_true")]
    pub default_visible: bool,
    /// Column width in pixels (None = auto)
    pub width: Option<u32>,
    /// Whether column is sortable
    pub sortable: bool,
    /// Whether column can be used in filters
    pub filterable: bool,
    /// Text alignment
    pub align: Option<String>,
}

fn default_visible_true() -> bool {
    true
}

/// Definition of a table
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinition {
    /// Unique table ID (e.g., "pipeline_runs", "agents")
    pub id: String,
    /// Human-readable table name
    pub name: String,
    /// Description of what this table shows
    pub description: Option<String>,
    /// Column definitions
    pub columns: Vec<ColumnDefinition>,
    /// Default sort column ID
    pub default_sort_column: Option<String>,
    /// Default sort direction ("asc" or "desc")
    pub default_sort_direction: Option<String>,
}

/// Complete table schema for a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSchema {
    /// Available tables
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
