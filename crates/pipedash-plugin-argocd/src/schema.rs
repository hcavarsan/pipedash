use pipedash_plugin_api::{
    defaults,
    schema,
};

mod pipelines;
mod runs;

pub fn create_table_schema() -> schema::TableSchema {
    let mut runs = defaults::default_pipeline_runs_table();
    runs.insert_after("status", runs::after_status_columns());
    runs.columns.extend(runs::trailing_columns());

    let mut pipelines = defaults::default_pipelines_table();
    pipelines.insert_after("status", pipelines::after_status_columns());
    pipelines.columns.extend(pipelines::trailing_columns());
    defaults::table_schema(runs, pipelines)
}
