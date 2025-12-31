use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(pipedash_plugin_api::defaults::default_pipelines_table())
}

fn create_pipeline_runs_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipeline_runs_table();

    table.columns.insert(1, create_workspace_column());
    table.columns.push(create_selector_column());

    table
}

fn create_workspace_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "workspace".to_string(),
        label: "Workspace".to_string(),
        description: Some("Bitbucket workspace".to_string()),
        field_path: "metadata.workspace".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(120),
        sortable: true,
        filterable: false,
        align: None,
    }
}

fn create_selector_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "selector_type".to_string(),
        label: "Type".to_string(),
        description: Some("Pipeline trigger type (custom, branches, default, tags)".to_string()),
        field_path: "metadata.selector_type".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: false,
        width: Some(100),
        sortable: false,
        filterable: false,
        align: None,
    }
}
