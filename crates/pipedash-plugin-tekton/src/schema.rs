//! Tekton CD table schema definitions
//!
//! This module contains all table and column definitions specific to Tekton.

use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(pipedash_plugin_api::defaults::default_pipelines_table())
}

/// Creates the pipeline_runs table with Tekton-specific columns
///
/// Tekton has extensive metadata that's important for Kubernetes users:
/// - `namespace`: Kubernetes namespace
/// - `pipeline_name`: Tekton Pipeline template name
/// - `pipelinerun_name`: Kubernetes PipelineRun resource name (for kubectl)
/// - `trigger`: User or system that triggered the run
/// - `event_type`: Tekton EventListener or Trigger name
fn create_pipeline_runs_table() -> schema::TableDefinition {
    use schema::*;

    TableDefinition {
        id: "pipeline_runs".to_string(),
        name: "Pipeline Runs".to_string(),
        description: Some("Tekton PipelineRun executions".to_string()),
        columns: vec![
            create_run_identifier_column(),
            create_namespace_column(),
            create_status_column(),
            create_pipeline_column(),
            create_branch_column(),
            create_started_at_column(),
            create_duration_column(),
            create_commit_sha_column(),
            create_trigger_column(),
            create_pipelinerun_column(),
            create_event_type_column(),
            create_task_summary_column(),
            create_concluded_at_column(),
            create_service_account_column(),
        ],
        default_sort_column: Some("run_number".to_string()),
        default_sort_direction: Some("desc".to_string()),
    }
}

/// Creates the run_identifier column (event ID or PipelineRun name)
fn create_run_identifier_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "run_identifier".to_string(),
        label: "Run".to_string(),
        description: Some("Run identifier (event ID or PipelineRun name)".to_string()),
        field_path: "metadata.run_identifier".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::TruncatedText,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(180),
        sortable: false,
        filterable: false,
        align: Some("left".to_string()),
    }
}

/// Creates the namespace column (critical for K8s users)
fn create_namespace_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "namespace".to_string(),
        label: "Namespace".to_string(),
        description: Some("Kubernetes namespace".to_string()),
        field_path: "metadata.namespace".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(140),
        sortable: true,
        filterable: false,
        align: None,
    }
}

/// Creates the status column
fn create_status_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "status".to_string(),
        label: "Status".to_string(),
        description: Some("Pipeline run status".to_string()),
        field_path: "status".to_string(),
        data_type: schema::ColumnDataType::Status,
        renderer: schema::CellRenderer::StatusBadge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(110),
        sortable: true,
        filterable: false,
        align: None,
    }
}

/// Creates the pipeline column (the template being run)
fn create_pipeline_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "pipeline".to_string(),
        label: "Pipeline".to_string(),
        description: Some("Tekton Pipeline template name".to_string()),
        field_path: "metadata.pipeline_name".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::TruncatedText,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(180),
        sortable: false,
        filterable: false,
        align: None,
    }
}

/// Creates the branch column
fn create_branch_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "branch".to_string(),
        label: "Branch".to_string(),
        description: Some("Git branch".to_string()),
        field_path: "branch".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::TruncatedText,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: false,
        width: Some(140),
        sortable: true,
        filterable: false,
        align: None,
    }
}

/// Creates the started_at column
fn create_started_at_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "started_at".to_string(),
        label: "Started".to_string(),
        description: Some("Start time".to_string()),
        field_path: "started_at".to_string(),
        data_type: schema::ColumnDataType::DateTime,
        renderer: schema::CellRenderer::DateTime,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(160),
        sortable: true,
        filterable: false,
        align: None,
    }
}

/// Creates the duration column
fn create_duration_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "duration_seconds".to_string(),
        label: "Duration".to_string(),
        description: Some("Execution duration".to_string()),
        field_path: "duration_seconds".to_string(),
        data_type: schema::ColumnDataType::Duration,
        renderer: schema::CellRenderer::Duration,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(100),
        sortable: true,
        filterable: false,
        align: None,
    }
}

/// Creates the commit_sha column
fn create_commit_sha_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "commit_sha".to_string(),
        label: "Commit".to_string(),
        description: Some("Git commit SHA".to_string()),
        field_path: "commit_sha".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Commit,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: false,
        width: Some(100),
        sortable: false,
        filterable: false,
        align: None,
    }
}

/// Creates the trigger column
fn create_trigger_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "trigger".to_string(),
        label: "Triggered By".to_string(),
        description: Some("User or system that triggered the run".to_string()),
        field_path: "metadata.trigger".to_string(),
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

/// Creates the pipelinerun column (for kubectl commands)
fn create_pipelinerun_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "pipelinerun".to_string(),
        label: "PipelineRun".to_string(),
        description: Some("Kubernetes PipelineRun resource name (for kubectl)".to_string()),
        field_path: "metadata.pipelinerun_name".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::TruncatedText,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: false,
        width: Some(200),
        sortable: false,
        filterable: false,
        align: None,
    }
}

/// Creates the event_type column
fn create_event_type_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "event_type".to_string(),
        label: "Event Listener".to_string(),
        description: Some("Tekton EventListener or Trigger that started this run".to_string()),
        field_path: "metadata.event_type".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: false,
        width: Some(150),
        sortable: false,
        filterable: false,
        align: None,
    }
}

/// Creates the task_summary column
fn create_task_summary_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "task_summary".to_string(),
        label: "Tasks".to_string(),
        description: Some("Task completion summary (completed/total)".to_string()),
        field_path: "metadata.task_summary".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: false,
        width: Some(80),
        sortable: false,
        filterable: false,
        align: Some("center".to_string()),
    }
}

/// Creates the concluded_at column (hidden by default)
fn create_concluded_at_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "concluded_at".to_string(),
        label: "Concluded".to_string(),
        description: Some("When the pipeline run completed or failed".to_string()),
        field_path: "concluded_at".to_string(),
        data_type: schema::ColumnDataType::DateTime,
        renderer: schema::CellRenderer::DateTime,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: false,
        width: Some(160),
        sortable: true,
        filterable: false,
        align: None,
    }
}

/// Creates the service_account column (hidden by default)
fn create_service_account_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "service_account".to_string(),
        label: "Service Account".to_string(),
        description: Some("Kubernetes ServiceAccount used for the run".to_string()),
        field_path: "metadata.service_account".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: false,
        width: Some(140),
        sortable: false,
        filterable: false,
        align: None,
    }
}
