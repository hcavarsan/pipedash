use pipedash_plugin_api::*;
use schema::ColumnDefinition;

pub fn create_table_schema() -> schema::TableSchema {
    let mut runs = defaults::default_pipeline_runs_table();
    runs.insert_after(
        "run_number",
        [
            ColumnDefinition::badge("namespace", "Namespace", "metadata.namespace")
                .described("GitLab group or namespace path")
                .hidden()
                .with_width(150)
                .sortable(),
        ],
    );
    runs.insert_after(
        "status",
        [
            ColumnDefinition::badge("source", "Source", "metadata.source")
                .described("Pipeline trigger source (push, web, scheduled, trigger, api, etc.)")
                .when_present()
                .hidden()
                .with_width(100),
        ],
    );
    defaults::table_schema(runs, defaults::default_pipelines_table())
}
