//! GitLab CI table schema definitions
//!
//! This module contains all table and column definitions specific to GitLab
//! CI/CD.

use pipedash_plugin_api::*;

/// Creates the complete table schema for GitLab CI plugin
///
/// This includes:
/// - Pipeline runs table with GitLab-specific columns (namespace, source)
/// - Standard pipelines table
pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(pipedash_plugin_api::defaults::default_pipelines_table())
}

/// Creates the pipeline_runs table with GitLab-specific columns
///
/// Extends the default pipeline_runs table with:
/// - `namespace`: GitLab group or namespace path
/// - `source`: Pipeline trigger source (push, web, scheduled, trigger, api,
///   etc.)
fn create_pipeline_runs_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipeline_runs_table();

    // Add GitLab-specific columns
    table.columns.insert(1, create_namespace_column());
    table.columns.insert(3, create_source_column());

    table
}

/// Creates the namespace column definition
///
/// Displays the GitLab group or namespace path for the project.
fn create_namespace_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "namespace".to_string(),
        label: "Namespace".to_string(),
        description: Some("GitLab group or namespace path".to_string()),
        field_path: "metadata.namespace".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(150),
        sortable: true,
        filterable: false,
        align: None,
    }
}

/// Creates the source column definition
///
/// Shows what triggered the pipeline run (push, web, scheduled, trigger, api,
/// etc.).
fn create_source_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "source".to_string(),
        label: "Source".to_string(),
        description: Some(
            "Pipeline trigger source (push, web, scheduled, trigger, api, etc.)".to_string(),
        ),
        field_path: "metadata.source".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: false,
        width: Some(100),
        sortable: false,
        filterable: false,
        align: None,
    }
}
