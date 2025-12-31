use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(create_pipelines_table())
}

fn create_pipeline_runs_table() -> schema::TableDefinition {
    pipedash_plugin_api::defaults::default_pipeline_runs_table()
}

fn create_pipelines_table() -> schema::TableDefinition {
    use schema::*;

    TableDefinition {
        id: "pipelines".to_string(),
        name: "Pipelines".to_string(),
        description: Some("All Buildkite pipelines".to_string()),
        columns: vec![
            create_name_column(),
            create_organization_column(),
            create_status_column(),
            create_repository_column(),
        ],
        default_sort_column: Some("name".to_string()),
        default_sort_direction: Some("asc".to_string()),
    }
}

fn create_name_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "name".to_string(),
        label: "Name".to_string(),
        description: None,
        field_path: "name".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(200),
        sortable: true,
        filterable: true,
        align: None,
    }
}

fn create_organization_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "organization".to_string(),
        label: "Organization".to_string(),
        description: None,
        field_path: "metadata.organization_slug".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::WhenPresent,
        default_visible: true,
        width: Some(140),
        sortable: true,
        filterable: true,
        align: None,
    }
}

fn create_status_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "status".to_string(),
        label: "Status".to_string(),
        description: None,
        field_path: "status".to_string(),
        data_type: schema::ColumnDataType::Status,
        renderer: schema::CellRenderer::StatusBadge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(120),
        sortable: true,
        filterable: true,
        align: None,
    }
}

fn create_repository_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "repository".to_string(),
        label: "Repository".to_string(),
        description: None,
        field_path: "repository".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::TruncatedText,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(180),
        sortable: true,
        filterable: true,
        align: None,
    }
}
