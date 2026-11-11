use pipedash_plugin_api::*;
use schema::ColumnDefinition;

pub fn create_table_schema() -> schema::TableSchema {
    let mut runs = defaults::default_pipeline_runs_table();
    runs.insert_after(
        "run_number",
        [
            ColumnDefinition::badge("workspace", "Workspace", "metadata.workspace")
                .described("Bitbucket workspace")
                .hidden()
                .with_width(120)
                .sortable(),
        ],
    );
    runs.columns.push(
        ColumnDefinition::badge("selector_type", "Type", "metadata.selector_type")
            .described("Pipeline trigger type (custom, branches, default, tags)")
            .when_present()
            .hidden()
            .with_width(100),
    );
    defaults::table_schema(runs, defaults::default_pipelines_table())
}
