use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(pipedash_plugin_api::defaults::default_pipelines_table())
}

fn create_pipeline_runs_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipeline_runs_table();
    table.columns.insert(1, create_build_number_column());
    table
}

fn create_build_number_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "build_number".to_string(),
        label: "Build".to_string(),
        description: Some("Build number, such as 20261028.1".to_string()),
        field_path: "metadata.build_number".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: true,
        width: Some(130),
        sortable: true,
        filterable: false,
    }
}
