use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    defaults::table_schema(
        defaults::default_pipeline_runs_table(),
        defaults::default_pipelines_table(),
    )
}
