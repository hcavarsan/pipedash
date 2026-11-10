use pipedash_plugin_api::*;
use schema::{
    CellRenderer,
    ColumnDataType,
    ColumnDefinition,
    TableDefinition,
};

pub fn create_table_schema() -> schema::TableSchema {
    defaults::table_schema(
        create_pipeline_runs_table(),
        defaults::default_pipelines_table(),
    )
}

fn create_pipeline_runs_table() -> TableDefinition {
    TableDefinition::new(
        "pipeline_runs",
        "Pipeline Runs",
        "Tekton PipelineRun executions",
        vec![
            ColumnDefinition::new(
                "run_identifier",
                "Run",
                "metadata.run_identifier",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
            )
            .described("Run identifier (event ID or PipelineRun name)")
            .with_width(180)
            .history_order(),
            ColumnDefinition::new(
                "namespace",
                "Namespace",
                "metadata.namespace",
                ColumnDataType::String,
                CellRenderer::Badge,
            )
            .described("Kubernetes namespace")
            .hidden()
            .with_width(140)
            .sortable(),
            ColumnDefinition::new(
                "status",
                "Status",
                "status",
                ColumnDataType::Status,
                CellRenderer::StatusBadge,
            )
            .described("Pipeline run status")
            .with_width(110)
            .sortable(),
            ColumnDefinition::new(
                "pipeline",
                "Pipeline",
                "metadata.pipeline_name",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
            )
            .described("Tekton Pipeline template name")
            .with_width(180),
            ColumnDefinition::new(
                "branch",
                "Branch",
                "branch",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
            )
            .described("Git branch")
            .when_present()
            .hidden()
            .with_width(140)
            .sortable(),
            ColumnDefinition::new(
                "started_at",
                "Started",
                "started_at",
                ColumnDataType::DateTime,
                CellRenderer::DateTime,
            )
            .described("Start time")
            .hidden()
            .with_width(160)
            .sortable(),
            ColumnDefinition::new(
                "duration_seconds",
                "Duration",
                "duration_seconds",
                ColumnDataType::Duration,
                CellRenderer::Duration,
            )
            .described("Execution duration")
            .with_width(100)
            .sortable(),
            ColumnDefinition::new(
                "commit_sha",
                "Commit",
                "commit_sha",
                ColumnDataType::String,
                CellRenderer::Commit,
            )
            .described("Git commit SHA")
            .when_present()
            .hidden()
            .with_width(100),
            ColumnDefinition::new(
                "trigger",
                "Triggered By",
                "metadata.trigger",
                ColumnDataType::String,
                CellRenderer::Badge,
            )
            .described("User or system that triggered the run")
            .when_present()
            .with_width(140),
            ColumnDefinition::new(
                "pipelinerun",
                "PipelineRun",
                "metadata.pipelinerun_name",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
            )
            .described("Kubernetes PipelineRun resource name (for kubectl)")
            .when_present()
            .hidden()
            .with_width(200),
            ColumnDefinition::new(
                "event_type",
                "Event Listener",
                "metadata.event_type",
                ColumnDataType::String,
                CellRenderer::Badge,
            )
            .described("Tekton EventListener or Trigger that started this run")
            .when_present()
            .hidden()
            .with_width(150),
            ColumnDefinition::new(
                "task_summary",
                "Tasks",
                "metadata.task_summary",
                ColumnDataType::String,
                CellRenderer::Text,
            )
            .described("Task completion summary (completed/total)")
            .when_present()
            .hidden()
            .with_width(80),
            ColumnDefinition::new(
                "concluded_at",
                "Concluded",
                "concluded_at",
                ColumnDataType::DateTime,
                CellRenderer::DateTime,
            )
            .described("When the pipeline run completed or failed")
            .when_present()
            .hidden()
            .with_width(160)
            .sortable(),
            ColumnDefinition::new(
                "service_account",
                "Service Account",
                "metadata.service_account",
                ColumnDataType::String,
                CellRenderer::Badge,
            )
            .described("Kubernetes ServiceAccount used for the run")
            .when_present()
            .hidden()
            .with_width(140),
        ],
    )
}
