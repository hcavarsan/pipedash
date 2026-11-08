use pipedash_plugin_api::*;
use schema::ColumnDefinition;

pub fn create_table_schema() -> schema::TableSchema {
    let mut runs = defaults::default_pipeline_runs_table();
    runs.insert_after(
        "run_number",
        [
            ColumnDefinition::text("build_number", "Build", "metadata.build_number")
                .described("Build number, such as 20261028.1")
                .when_present()
                .with_width(130)
                .sortable(),
        ],
    );
    defaults::table_schema(runs, defaults::default_pipelines_table())
}
