use pipedash_plugin_api::*;
use schema::ColumnDefinition;

pub fn create_table_schema() -> schema::TableSchema {
    let mut pipelines = defaults::default_pipelines_table();
    pipelines.columns.push(
        ColumnDefinition::text("project_slug", "Project Slug", "metadata.project_slug")
            .described("CircleCI project slug used by the API")
            .hidden()
            .with_width(200)
            .sortable(),
    );
    defaults::table_schema(defaults::default_pipeline_runs_table(), pipelines)
}
