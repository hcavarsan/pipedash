use pipedash_plugin_api::schema::{
    CellRenderer,
    ColumnDataType,
    ColumnDefinition,
};

pub(super) fn after_status_columns() -> Vec<ColumnDefinition> {
    vec![
        ColumnDefinition::new(
            "project",
            "Project",
            "metadata.project",
            ColumnDataType::String,
            CellRenderer::Badge,
        )
        .described("ArgoCD project")
        .with_width(150)
        .sortable(),
        ColumnDefinition::new(
            "sync_status",
            "Sync Status",
            "metadata.sync_status",
            ColumnDataType::String,
            CellRenderer::StatusBadge,
        )
        .described("ArgoCD sync status (Synced, OutOfSync, Unknown)")
        .with_width(120)
        .sortable(),
        ColumnDefinition::new(
            "health_status",
            "Health",
            "metadata.health_status",
            ColumnDataType::String,
            CellRenderer::StatusBadge,
        )
        .described("ArgoCD health status (Healthy, Progressing, Degraded, Suspended, Missing)")
        .with_width(110)
        .sortable(),
        ColumnDefinition::new(
            "resource_health_summary",
            "Resources",
            "metadata.resource_health_summary",
            ColumnDataType::String,
            CellRenderer::Badge,
        )
        .described("Healthy resources vs total count")
        .with_width(120),
        ColumnDefinition::new(
            "out_of_sync_count",
            "Out of Sync",
            "metadata.out_of_sync_count",
            ColumnDataType::Number,
            CellRenderer::Badge,
        )
        .described("Number of resources out of sync")
        .with_width(110)
        .sortable(),
        ColumnDefinition::new(
            "last_sync_time",
            "Last Sync",
            "metadata.last_sync_time",
            ColumnDataType::DateTime,
            CellRenderer::DateTime,
        )
        .described("When the application was last successfully synced")
        .with_width(150)
        .sortable(),
        ColumnDefinition::new(
            "destination",
            "Destination",
            "metadata.destination_namespace",
            ColumnDataType::String,
            CellRenderer::Badge,
        )
        .described("Target cluster and namespace")
        .with_width(150)
        .sortable(),
        ColumnDefinition::new(
            "source_type",
            "Type",
            "metadata.source_type",
            ColumnDataType::String,
            CellRenderer::Badge,
        )
        .described("Deployment type: Helm, Kustomize, or Plain manifests")
        .with_width(100)
        .sortable(),
        ColumnDefinition::new(
            "target_revision",
            "Revision",
            "metadata.target_revision",
            ColumnDataType::String,
            CellRenderer::Badge,
        )
        .described("Target Git branch, tag, or commit")
        .with_width(120)
        .sortable(),
        ColumnDefinition::new(
            "auto_sync",
            "Auto-Sync",
            "metadata.auto_sync_enabled",
            ColumnDataType::Boolean,
            CellRenderer::Badge,
        )
        .described("Whether automated sync is enabled")
        .with_width(100)
        .sortable(),
    ]
}

pub(super) fn trailing_columns() -> Vec<ColumnDefinition> {
    vec![
        ColumnDefinition::new(
            "current_revision",
            "Current Revision",
            "metadata.current_revision",
            ColumnDataType::String,
            CellRenderer::Text,
        )
        .described("Actually deployed Git revision")
        .hidden()
        .with_width(150),
        ColumnDefinition::new(
            "source_path",
            "Source Path",
            "metadata.source_path",
            ColumnDataType::String,
            CellRenderer::Text,
        )
        .described("Path in Git repository")
        .hidden()
        .with_width(200)
        .sortable(),
        ColumnDefinition::new(
            "repository_url",
            "Repository URL",
            "metadata.repo_url",
            ColumnDataType::String,
            CellRenderer::Text,
        )
        .described("Full Git repository URL")
        .hidden()
        .with_width(300)
        .sortable(),
        ColumnDefinition::new(
            "cluster_server",
            "Cluster Server",
            "metadata.destination_cluster",
            ColumnDataType::String,
            CellRenderer::Text,
        )
        .described("Kubernetes cluster server URL")
        .hidden()
        .with_width(250)
        .sortable(),
        ColumnDefinition::new(
            "prune_enabled",
            "Prune",
            "metadata.prune_enabled",
            ColumnDataType::Boolean,
            CellRenderer::Badge,
        )
        .described("Auto-sync prunes orphaned resources")
        .hidden()
        .with_width(80)
        .sortable(),
        ColumnDefinition::new(
            "self_heal_enabled",
            "Self-Heal",
            "metadata.self_heal_enabled",
            ColumnDataType::Boolean,
            CellRenderer::Badge,
        )
        .described("Auto-sync self-heals manual changes")
        .hidden()
        .with_width(90)
        .sortable(),
        ColumnDefinition::new(
            "health_message",
            "Health Message",
            "metadata.health_message",
            ColumnDataType::String,
            CellRenderer::Text,
        )
        .described("Details about application health")
        .hidden()
        .with_width(250),
    ]
}
