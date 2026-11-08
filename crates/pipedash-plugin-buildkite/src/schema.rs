use pipedash_plugin_api::*;
use schema::ColumnDefinition;

pub fn create_table_schema() -> schema::TableSchema {
    let mut pipelines = defaults::default_pipelines_table();
    pipelines.description = Some("All Buildkite pipelines".to_string());
    pipelines.columns.retain(|column| column.id != "branch");
    pipelines.insert_after(
        "name",
        [
            ColumnDefinition::badge("organization", "Organization", "metadata.organization_slug")
                .when_present()
                .with_width(140)
                .sortable()
                .filterable(),
        ],
    );
    defaults::table_schema(defaults::default_pipeline_runs_table(), pipelines)
}
