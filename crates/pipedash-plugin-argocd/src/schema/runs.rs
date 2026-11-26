use pipedash_plugin_api::schema::{
    CellRenderer,
    ColumnDataType,
    ColumnDefinition,
};

pub(super) fn after_status_columns() -> Vec<ColumnDefinition> {
    vec![
        ColumnDefinition::new(
            "source_type",
            "Type",
            "metadata.source_type",
            ColumnDataType::String,
            CellRenderer::Badge,
        )
        .described("Deployment type synced: Helm, Kustomize, or Plain")
        .with_width(100)
        .sortable(),
        ColumnDefinition::new(
            "destination",
            "Namespace",
            "metadata.destination_namespace",
            ColumnDataType::String,
            CellRenderer::Badge,
        )
        .described("Target namespace for this sync")
        .with_width(130)
        .sortable(),
        ColumnDefinition::new(
            "source_path",
            "Path",
            "metadata.source_path",
            ColumnDataType::String,
            CellRenderer::Text,
        )
        .described("Path in Git repository that was synced")
        .with_width(200),
    ]
}

pub(super) fn trailing_columns() -> Vec<ColumnDefinition> {
    vec![
        ColumnDefinition::new(
            "sync_revision",
            "Synced Revision",
            "metadata.sync_revision",
            ColumnDataType::String,
            CellRenderer::Text,
        )
        .described("Git commit that was synced")
        .hidden()
        .with_width(150),
        ColumnDefinition::new(
            "operation_message",
            "Operation Message",
            "metadata.operation_message",
            ColumnDataType::String,
            CellRenderer::Text,
        )
        .described("Sync operation status or error message")
        .hidden()
        .with_width(300),
        ColumnDefinition::new(
            "app_sync_status",
            "Current Sync",
            "metadata.app_sync_status",
            ColumnDataType::String,
            CellRenderer::Badge,
        )
        .described("Application sync status; shown on the current sync only")
        .hidden()
        .with_width(120),
        ColumnDefinition::new(
            "app_health_status",
            "Current Health",
            "metadata.app_health_status",
            ColumnDataType::String,
            CellRenderer::Badge,
        )
        .described("Application health status; shown on the current sync only")
        .hidden()
        .with_width(120),
        ColumnDefinition::new(
            "helm_chart",
            "Helm Chart",
            "metadata.helm_chart",
            ColumnDataType::String,
            CellRenderer::Badge,
        )
        .described("Helm chart name (if Helm deployment)")
        .hidden()
        .with_width(150)
        .sortable(),
    ]
}
