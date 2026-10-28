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

    let mut update_available = metadata_column(
        "update_available",
        "Update",
        "Whether a newer image is available for any service",
        schema::CellRenderer::Badge,
        true,
    );
    update_available.data_type = schema::ColumnDataType::Boolean;
    let leading = [
        metadata_column(
            "state",
            "State",
            "Komodo stack state (running, stopped, unhealthy, down, ...)",
            schema::CellRenderer::Badge,
            true,
        ),
        metadata_column(
            "server",
            "Server",
            "Server or swarm the stack is deployed on",
            schema::CellRenderer::Badge,
            true,
        ),
        update_available,
    ];
    for (offset, column) in leading.into_iter().enumerate() {
        table.columns.insert(status_index + 1 + offset, column);
    }

    table.columns.extend([
        metadata_column(
            "repo",
            "Git Repository",
            "Git repository the stack is deployed from",
            schema::CellRenderer::Text,
            false,
        ),
        metadata_column(
            "deployed_hash",
            "Deployed Commit",
            "Short commit hash that is deployed (repo based stacks)",
            schema::CellRenderer::Text,
            false,
        ),
        metadata_column(
            "latest_hash",
            "Latest Commit",
            "Short commit hash at the head of the branch (repo based stacks)",
            schema::CellRenderer::Text,
            false,
        ),
        metadata_column(
            "docker_status",
            "Docker Status",
            "Status string reported by docker compose",
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
            "operation",
            "Operation",
            "Komodo operation recorded by the update (DeployStack, StartStack, ...)",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn column_ids(table: &str) -> Vec<String> {
        create_table_schema()
            .get_table(table)
            .unwrap()
            .columns
            .iter()
            .map(|column| column.id.clone())
            .collect()
    }

    #[test]
    fn pipeline_columns_follow_status_with_stack_facts() {
        let ids = column_ids("pipelines");

        assert_eq!(
            ids[..6],
            [
                "name",
                "status",
                "state",
                "server",
                "update_available",
                "repository"
            ]
        );
        assert_eq!(
            ids[ids.len() - 4..],
            ["repo", "deployed_hash", "latest_hash", "docker_status"]
        );
    }

    #[test]
    fn run_columns_show_the_operation_after_status() {
        let ids = column_ids("pipeline_runs");

        assert_eq!(ids[..3], ["run_number", "status", "operation"]);
    }

    #[test]
    fn stack_facts_read_from_pipeline_metadata() {
        let schema = create_table_schema();
        let table = schema.get_table("pipelines").unwrap();
        let state = table.columns.iter().find(|c| c.id == "state").unwrap();
        let update = table
            .columns
            .iter()
            .find(|c| c.id == "update_available")
            .unwrap();

        assert_eq!(state.field_path, "metadata.state");
        assert_eq!(update.field_path, "metadata.update_available");
        assert_eq!(update.data_type, schema::ColumnDataType::Boolean);
    }
}
