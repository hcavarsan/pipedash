use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(pipedash_plugin_api::defaults::default_pipelines_table())
}

fn create_pipeline_runs_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipeline_runs_table();

    if let Some(run_number_col) = table.columns.iter_mut().find(|c| c.id == "run_number") {
        run_number_col.default_visible = false;
        run_number_col.label = "Run Number".to_string();
        run_number_col.description = Some("Sequential run number (workflow-specific)".to_string());
    }

    table.columns.insert(1, create_run_id_column());
    table.columns.insert(2, create_owner_column());
    table.columns.insert(3, create_event_column());

    table
}

fn create_run_id_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "run_id".to_string(),
        label: "Run ID".to_string(),
        description: Some("GitHub run ID (used in URLs)".to_string()),
        field_path: "metadata.run_id".to_string(),
        data_type: schema::ColumnDataType::Number,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(130),
        sortable: true,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_owner_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "owner".to_string(),
        label: "Owner".to_string(),
        description: Some("Repository owner/organization".to_string()),
        field_path: "metadata.owner".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(120),
        sortable: true,
        filterable: false,
        align: None,
    }
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
        align: None,
    }
}
