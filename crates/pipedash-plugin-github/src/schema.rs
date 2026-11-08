use pipedash_plugin_api::*;
use schema::{
    CellRenderer,
    ColumnDataType,
    ColumnDefinition,
};

pub fn create_table_schema() -> schema::TableSchema {
    let mut runs = defaults::default_pipeline_runs_table();
    if let Some(run_number) = runs.column_mut("run_number") {
        run_number.default_visible = false;
        run_number.label = "Run Number".to_string();
        run_number.description = Some("Sequential run number (workflow-specific)".to_string());
    }
    runs.insert_after(
        "run_number",
        [
            ColumnDefinition::new(
                "run_id",
                "Run ID",
                "metadata.run_id",
                ColumnDataType::Number,
                CellRenderer::Text,
            )
            .described("GitHub run ID (used in URLs)")
            .with_width(130)
            .sortable(),
            ColumnDefinition::badge("owner", "Owner", "metadata.owner")
                .described("Repository owner/organization")
                .with_width(120)
                .sortable(),
            columns::trigger_event_column(
                "Event that triggered the workflow (push, pull_request, workflow_dispatch, etc.)",
            ),
        ],
    );
    defaults::table_schema(runs, defaults::default_pipelines_table())
}
