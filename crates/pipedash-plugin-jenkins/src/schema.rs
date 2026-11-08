use pipedash_plugin_api::*;
use schema::ColumnDefinition;

pub fn create_table_schema() -> schema::TableSchema {
    let mut runs = defaults::default_pipeline_runs_table();
    runs.insert_after(
        "status",
        [
            ColumnDefinition::badge("trigger_cause", "Trigger", "metadata.trigger_cause")
                .described("How the build was triggered (manual, timer, upstream, webhook, etc.)")
                .when_present()
                .with_width(140),
        ],
    );
    defaults::table_schema(runs, defaults::default_pipelines_table())
}
