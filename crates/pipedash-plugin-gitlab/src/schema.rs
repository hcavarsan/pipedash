use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(pipedash_plugin_api::defaults::default_pipelines_table())
}

fn create_pipeline_runs_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipeline_runs_table();

    table.columns.insert(1, create_namespace_column());
    table.columns.insert(3, create_source_column());

    table
}

fn create_namespace_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "namespace".to_string(),
        label: "Namespace".to_string(),
        description: Some("GitLab group or namespace path".to_string()),
        field_path: "metadata.namespace".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(150),
        sortable: true,
        filterable: false,
        align: None,
    }
}

fn create_source_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "source".to_string(),
        label: "Source".to_string(),
        description: Some(
            "Pipeline trigger source (push, web, scheduled, trigger, api, etc.)".to_string(),
        ),
        field_path: "metadata.source".to_string(),
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
