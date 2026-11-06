use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(pipedash_plugin_api::defaults::default_pipeline_runs_table())
        .add_table(create_pipelines_table())
}

fn create_pipelines_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipelines_table();
    table.columns.push(schema::ColumnDefinition {
        id: "project_slug".to_string(),
        label: "Project Slug".to_string(),
        description: Some("CircleCI project slug used by the API".to_string()),
        field_path: "metadata.project_slug".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(200),
        sortable: true,
        filterable: false,
    });
    table
}
