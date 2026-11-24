use pipedash_plugin_api::*;
use schema::ColumnDefinition;

pub fn create_table_schema() -> schema::TableSchema {
    let mut runs = defaults::default_pipeline_runs_table();
    runs.description = Some("Argo workflows started from a template or cron workflow".to_string());
    runs.columns.retain(|column| column.id != "run_number");
    runs.columns.insert(
        0,
        ColumnDefinition::text("workflow", "Workflow", "metadata.workflow")
            .described("Workflow resource name (for kubectl and argo)")
            .with_width(220)
            .history_order(),
    );
    runs.columns.extend([
        ColumnDefinition::text("progress", "Progress", "metadata.progress")
            .described("Completed and total steps")
            .when_present()
            .hidden()
            .with_width(90),
        ColumnDefinition::text("message", "Message", "metadata.message")
            .when_present()
            .hidden()
            .with_width(240),
    ]);

    let mut pipelines = defaults::default_pipelines_table();
    pipelines.columns.retain(|column| column.id != "branch");
    pipelines.insert_after(
        "name",
        [
            ColumnDefinition::badge("kind", "Kind", "metadata.kind")
                .with_width(150)
                .sortable()
                .filterable(),
            ColumnDefinition::badge("namespace", "Namespace", "metadata.namespace")
                .with_width(140)
                .sortable()
                .filterable(),
            ColumnDefinition::text("schedule", "Schedule", "metadata.schedule")
                .when_present()
                .hidden()
                .with_width(140),
        ],
    );
    defaults::table_schema(runs, pipelines)
}
