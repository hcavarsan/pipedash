use pipedash_plugin_api::*;
use schema::{
    CellRenderer,
    ColumnDefinition,
};

pub fn create_table_schema() -> schema::TableSchema {
    let mut runs = defaults::default_pipeline_runs_table();
    runs.insert_after(
        "status",
        [ColumnDefinition::metadata(
            "source",
            "Source",
            "What started the deployment (Webhook, Manual, API, Rollback, Pull request)",
            CellRenderer::Badge,
            true,
        )],
    );

    let mut pipelines = defaults::default_pipelines_table();
    pipelines.insert_after(
        "status",
        [
            ColumnDefinition::metadata(
                "state",
                "State",
                "Live state of the Coolify application (Up, Unhealthy, Exited, ...)",
                CellRenderer::Badge,
                true,
            ),
            ColumnDefinition::metadata(
                "server",
                "Server",
                "Server the application is deployed on",
                CellRenderer::Badge,
                true,
            ),
        ],
    );
    pipelines.columns.extend([
        ColumnDefinition::metadata(
            "build_pack",
            "Build Pack",
            "Coolify build pack (nixpacks, dockerfile, dockercompose, ...)",
            CellRenderer::Badge,
            false,
        ),
        ColumnDefinition::metadata(
            "domains",
            "Domains",
            "Domains served by the application",
            CellRenderer::Text,
            false,
        ),
    ]);
    defaults::table_schema(runs, pipelines)
}
