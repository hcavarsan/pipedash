use pipedash_plugin_api::*;
use schema::{
    CellRenderer,
    ColumnDataType,
    ColumnDefinition,
};

pub fn create_table_schema() -> schema::TableSchema {
    let mut runs = defaults::default_pipeline_runs_table();
    runs.insert_after(
        "status",
        [ColumnDefinition::metadata(
            "operation",
            "Operation",
            "Komodo operation recorded by the update (DeployStack, StartStack, ...)",
            CellRenderer::Badge,
            true,
        )],
    );

    let mut state = ColumnDefinition::metadata(
        "state",
        "State",
        "Live state of the Komodo stack (Up, Stopped, Unhealthy, Down, ...)",
        CellRenderer::Badge,
        true,
    );
    state.field_path = "resource_state.label".to_string();
    let mut update_available = ColumnDefinition::metadata(
        "update_available",
        "Update",
        "Whether a newer image is available for any service",
        CellRenderer::Badge,
        true,
    );
    update_available.data_type = ColumnDataType::Boolean;

    let mut pipelines = defaults::default_pipelines_table();
    pipelines.insert_after(
        "status",
        [
            state,
            ColumnDefinition::metadata(
                "server",
                "Server",
                "Server or swarm the stack is deployed on",
                CellRenderer::Badge,
                true,
            ),
            update_available,
        ],
    );
    pipelines.columns.extend([
        ColumnDefinition::metadata(
            "repo",
            "Git Repository",
            "Git repository the stack is deployed from",
            CellRenderer::Text,
            false,
        ),
        ColumnDefinition::metadata(
            "deployed_hash",
            "Deployed Commit",
            "Short commit hash that is deployed (repo based stacks)",
            CellRenderer::Text,
            false,
        ),
        ColumnDefinition::metadata(
            "latest_hash",
            "Latest Commit",
            "Short commit hash at the head of the branch (repo based stacks)",
            CellRenderer::Text,
            false,
        ),
        ColumnDefinition::metadata(
            "docker_status",
            "Docker Status",
            "Status string reported by docker compose",
            CellRenderer::Text,
            false,
        ),
    ]);
    defaults::table_schema(runs, pipelines)
}

#[cfg(test)]
mod tests;
