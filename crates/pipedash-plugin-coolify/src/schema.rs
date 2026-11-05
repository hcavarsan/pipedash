use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(create_pipelines_table())
}

fn create_pipelines_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipelines_table();
    let status_index = table
        .columns
        .iter()
        .position(|column| column.id == "status")
        .unwrap_or(1);

    let leading = [
        schema::ColumnDefinition {
            field_path: "resource_state.label".to_string(),
            ..metadata_column(
                "state",
                "State",
                "Live state of the Coolify application (Up, Unhealthy, Exited, ...)",
                schema::CellRenderer::Badge,
                true,
            )
        },
        metadata_column(
            "server",
            "Server",
            "Server the application is deployed on",
            schema::CellRenderer::Badge,
            true,
        ),
    ];
    for (offset, column) in leading.into_iter().enumerate() {
        table.columns.insert(status_index + 1 + offset, column);
    }

    table.columns.extend([
        metadata_column(
            "build_pack",
            "Build Pack",
            "Coolify build pack (nixpacks, dockerfile, dockercompose, ...)",
            schema::CellRenderer::Badge,
            false,
        ),
        metadata_column(
            "domains",
            "Domains",
            "Domains served by the application",
            schema::CellRenderer::Text,
            false,
        ),
    ]);

    table
}

fn create_pipeline_runs_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipeline_runs_table();
    let status_index = table
        .columns
        .iter()
        .position(|column| column.id == "status")
        .unwrap_or(1);

    table.columns.insert(
        status_index + 1,
        metadata_column(
            "source",
            "Source",
            "What started the deployment (Webhook, Manual, API, Rollback, Pull request)",
            schema::CellRenderer::Badge,
            true,
        ),
    );

    table
}

fn metadata_column(
    id: &str, label: &str, description: &str, renderer: schema::CellRenderer, default_visible: bool,
) -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: id.to_string(),
        label: label.to_string(),
        description: Some(description.to_string()),
        field_path: format!("metadata.{id}"),
        data_type: schema::ColumnDataType::String,
        renderer,
        visibility: schema::ColumnVisibility::Always,
        default_visible,
        width: Some(140),
        sortable: true,
        filterable: false,
    }
}
