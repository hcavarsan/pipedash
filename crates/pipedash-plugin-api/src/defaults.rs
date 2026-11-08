use crate::schema::*;

pub fn table_schema(runs: TableDefinition, pipelines: TableDefinition) -> TableSchema {
    TableSchema::new().add_table(runs).add_table(pipelines)
}

fn status_column() -> ColumnDefinition {
    ColumnDefinition::new(
        "status",
        "Status",
        "status",
        ColumnDataType::Status,
        CellRenderer::StatusBadge,
    )
    .with_width(120)
    .sortable()
    .filterable()
}

pub fn default_pipeline_runs_table() -> TableDefinition {
    TableDefinition::new(
        "pipeline_runs",
        "Pipeline Runs",
        "Build/run history for pipelines",
        ("run_number", "desc"),
        vec![
            ColumnDefinition::new(
                "run_number",
                "Run",
                "run_number",
                ColumnDataType::Number,
                CellRenderer::Text,
            )
            .described("Run/build number")
            .with_width(100)
            .sortable(),
            status_column(),
            ColumnDefinition::new(
                "branch",
                "Branch",
                "branch",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
            )
            .when_present()
            .with_width(150)
            .sortable()
            .filterable(),
            ColumnDefinition::new(
                "started_at",
                "Started",
                "started_at",
                ColumnDataType::DateTime,
                CellRenderer::DateTime,
            )
            .with_width(180)
            .sortable(),
            ColumnDefinition::new(
                "duration_seconds",
                "Duration",
                "duration_seconds",
                ColumnDataType::Duration,
                CellRenderer::Duration,
            )
            .with_width(120)
            .sortable(),
            ColumnDefinition::new(
                "commit_sha",
                "Commit",
                "commit_sha",
                ColumnDataType::String,
                CellRenderer::Commit,
            )
            .when_present()
            .hidden()
            .with_width(140),
            ColumnDefinition::new(
                "actor",
                "Actor",
                "actor",
                ColumnDataType::String,
                CellRenderer::Text,
            )
            .described("Who triggered the run")
            .when_present()
            .hidden()
            .with_width(120)
            .sortable()
            .filterable(),
        ],
    )
}

pub fn default_pipelines_table() -> TableDefinition {
    TableDefinition::new(
        "pipelines",
        "Pipelines",
        "All pipelines/workflows",
        ("name", "asc"),
        vec![
            ColumnDefinition::new(
                "name",
                "Name",
                "name",
                ColumnDataType::String,
                CellRenderer::Text,
            )
            .with_width(200)
            .sortable()
            .filterable(),
            status_column(),
            ColumnDefinition::new(
                "repository",
                "Repository",
                "repository",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
            )
            .with_width(180)
            .sortable()
            .filterable(),
            ColumnDefinition::new(
                "branch",
                "Branch",
                "branch",
                ColumnDataType::String,
                CellRenderer::Badge,
            )
            .when_present()
            .with_width(120)
            .sortable()
            .filterable(),
        ],
    )
}
