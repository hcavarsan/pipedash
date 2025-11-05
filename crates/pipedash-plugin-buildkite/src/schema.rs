//! Buildkite table schema definitions
//!
//! This module contains all table and column definitions specific to Buildkite.

use pipedash_plugin_api::*;

/// Creates the complete table schema for Buildkite plugin
///
/// This includes:
/// - Standard pipeline runs table
/// - Pipelines table with Buildkite-specific organization column
pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(create_pipelines_table())
}

/// Creates the pipeline_runs table for Buildkite
///
/// Uses the standard default columns without modifications.
fn create_pipeline_runs_table() -> schema::TableDefinition {
    pipedash_plugin_api::defaults::default_pipeline_runs_table()
}

/// Creates the pipelines table with Buildkite-specific columns
///
/// Extends the standard pipelines table with:
/// - `organization`: Buildkite organization slug
fn create_pipelines_table() -> schema::TableDefinition {
    use schema::*;

    TableDefinition {
        id: "pipelines".to_string(),
        name: "Pipelines".to_string(),
        description: Some("All Buildkite pipelines".to_string()),
        columns: vec![
            create_name_column(),
            create_organization_column(),
            create_status_column(),
            create_repository_column(),
        ],
        default_sort_column: Some("name".to_string()),
        default_sort_direction: Some("asc".to_string()),
    }
}

/// Creates the name column for pipelines table
fn create_name_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "name".to_string(),
        label: "Name".to_string(),
        description: None,
        field_path: "name".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(200),
        sortable: true,
        filterable: true,
        align: None,
    }
}

/// Creates the organization column for pipelines table
///
/// Displays the Buildkite organization slug from metadata.
fn create_organization_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "organization".to_string(),
        label: "Organization".to_string(),
        description: None,
        field_path: "metadata.organization_slug".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: true,
        width: Some(140),
        sortable: true,
        filterable: true,
        align: None,
    }
}

/// Creates the status column for pipelines table
fn create_status_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "status".to_string(),
        label: "Status".to_string(),
        description: None,
        field_path: "status".to_string(),
        data_type: schema::ColumnDataType::Status,
        renderer: schema::CellRenderer::StatusBadge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(120),
        sortable: true,
        filterable: true,
        align: None,
    }
}

/// Creates the repository column for pipelines table
fn create_repository_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "repository".to_string(),
        label: "Repository".to_string(),
        description: None,
        field_path: "repository".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::TruncatedText,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(180),
        sortable: true,
        filterable: true,
        align: None,
    }
}
