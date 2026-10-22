use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(pipedash_plugin_api::defaults::default_pipelines_table())
}

fn create_pipeline_runs_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipeline_runs_table();
    table.columns.insert(1, create_event_column());
    table
}

fn create_event_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "event".to_string(),
        label: "Trigger".to_string(),
        description: Some(
            "Event that triggered the workflow (push, pull_request, workflow_dispatch, etc.)"
                .to_string(),
        ),
        field_path: "metadata.event".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(130),
        sortable: true,
        filterable: false,
    }
}
