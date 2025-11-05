//! GitHub Actions table schema definitions
//!
//! This module contains all table and column definitions specific to GitHub
//! Actions.

use pipedash_plugin_api::*;

/// Creates the complete table schema for GitHub Actions plugin
///
/// This includes:
/// - Pipeline runs table with GitHub-specific columns (run_id, owner, event)
/// - Standard pipelines table
pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(pipedash_plugin_api::defaults::default_pipelines_table())
}

/// Creates the pipeline_runs table with GitHub-specific columns
///
/// Extends the default pipeline_runs table with:
/// - `run_id`: GitHub's global run ID (used in URLs)
/// - `owner`: Repository owner/organization
/// - `event`: Trigger event (push, pull_request, workflow_dispatch, etc.)
///
/// Also modifies the default `run_number` column to be hidden by default
/// since GitHub's run_id is more commonly used.
fn create_pipeline_runs_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipeline_runs_table();

    // Hide the default run_number column (sequential workflow number)
    // GitHub's run_id is more useful for URLs
    if let Some(run_number_col) = table.columns.iter_mut().find(|c| c.id == "run_number") {
        run_number_col.default_visible = false;
        run_number_col.label = "Run Number".to_string();
        run_number_col.description = Some("Sequential run number (workflow-specific)".to_string());
    }

    // Add GitHub-specific columns
    table.columns.insert(1, create_run_id_column());
    table.columns.insert(2, create_owner_column());
    table.columns.insert(3, create_event_column());

    table
}

/// Creates the run_id column definition
///
/// The run_id is GitHub's global identifier for workflow runs,
/// used in GitHub URLs and API calls.
fn create_run_id_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "run_id".to_string(),
        label: "Run ID".to_string(),
        description: Some("GitHub run ID (used in URLs)".to_string()),
        field_path: "metadata.run_id".to_string(),
        data_type: schema::ColumnDataType::Number,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(130),
        sortable: true,
        filterable: false,
        align: Some("center".to_string()),
    }
}

/// Creates the owner column definition
///
/// Displays the repository owner or organization name.
fn create_owner_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "owner".to_string(),
        label: "Owner".to_string(),
        description: Some("Repository owner/organization".to_string()),
        field_path: "metadata.owner".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(120),
        sortable: true,
        filterable: false,
        align: None,
    }
}

/// Creates the event column definition
///
/// Shows what triggered the workflow run (push, pull_request,
/// workflow_dispatch, schedule, etc.).
fn create_event_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "event".to_string(),
        label: "Trigger".to_string(),
        description: Some(
            "Event that triggered the workflow (push, pull_request, workflow_dispatch, etc.)"
                .to_string(),
        ),
        field_path: "metadata.event".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(130),
        sortable: true,
        filterable: false,
        align: None,
    }
}
