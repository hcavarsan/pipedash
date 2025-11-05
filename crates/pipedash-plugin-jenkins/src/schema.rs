//! Jenkins table schema definitions
//!
//! This module contains all table and column definitions specific to Jenkins.

use pipedash_plugin_api::*;

/// Creates the complete table schema for Jenkins plugin
///
/// This includes:
/// - Pipeline runs table with Jenkins-specific columns (trigger_cause)
/// - Standard pipelines table
pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(pipedash_plugin_api::defaults::default_pipelines_table())
}

/// Creates the pipeline_runs table with Jenkins-specific columns
///
/// Extends the default pipeline_runs table with:
/// - `trigger_cause`: How the build was triggered (manual, timer, upstream,
///   webhook, etc.)
fn create_pipeline_runs_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipeline_runs_table();

    // Add Jenkins-specific trigger cause column
    table.columns.insert(2, create_trigger_cause_column());

    table
}

/// Creates the trigger_cause column definition
///
/// Shows how the Jenkins build was triggered (manual, timer, upstream, webhook,
/// etc.).
fn create_trigger_cause_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "trigger_cause".to_string(),
        label: "Trigger".to_string(),
        description: Some(
            "How the build was triggered (manual, timer, upstream, webhook, etc.)".to_string(),
        ),
        field_path: "metadata.trigger_cause".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: true,
        width: Some(140),
        sortable: false,
        filterable: false,
        align: None,
    }
}
