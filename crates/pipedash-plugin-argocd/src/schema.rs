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
        .position(|c| c.id == "status")
        .unwrap_or(2);

    table
        .columns
        .insert(status_index + 1, create_project_column());
    table
        .columns
        .insert(status_index + 2, create_sync_status_column());
    table
        .columns
        .insert(status_index + 3, create_health_status_column());
    table
        .columns
        .insert(status_index + 4, create_resource_health_summary_column());
    table
        .columns
        .insert(status_index + 5, create_out_of_sync_count_column());
    table
        .columns
        .insert(status_index + 6, create_last_sync_time_column());
    table
        .columns
        .insert(status_index + 7, create_destination_column());
    table
        .columns
        .insert(status_index + 8, create_source_type_column());
    table
        .columns
        .insert(status_index + 9, create_target_revision_column());
    table
        .columns
        .insert(status_index + 10, create_auto_sync_column());

    table.columns.push(create_current_revision_column());
    table.columns.push(create_source_path_column());
    table.columns.push(create_repository_url_column());
    table.columns.push(create_cluster_server_column());
    table.columns.push(create_prune_enabled_column());
    table.columns.push(create_self_heal_enabled_column());
    table.columns.push(create_health_message_column());

    table
}

fn create_pipeline_runs_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipeline_runs_table();

    let status_index = table
        .columns
        .iter()
        .position(|c| c.id == "status")
        .unwrap_or(1);

    table
        .columns
        .insert(status_index + 1, create_run_source_type_column());
    table
        .columns
        .insert(status_index + 2, create_run_destination_column());
    table
        .columns
        .insert(status_index + 3, create_run_source_path_column());

    table.columns.push(create_sync_revision_column());
    table.columns.push(create_operation_message_column());
    table.columns.push(create_run_app_sync_status_column());
    table.columns.push(create_run_app_health_status_column());
    table.columns.push(create_helm_chart_column());

    table
}

fn create_sync_status_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "sync_status".to_string(),
        label: "Sync Status".to_string(),
        description: Some("ArgoCD sync status (Synced, OutOfSync, Unknown)".to_string()),
        field_path: "metadata.sync_status".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::StatusBadge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(120),
        sortable: true,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_health_status_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "health_status".to_string(),
        label: "Health".to_string(),
        description: Some(
            "ArgoCD health status (Healthy, Progressing, Degraded, Suspended, Missing)".to_string(),
        ),
        field_path: "metadata.health_status".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::StatusBadge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(110),
        sortable: true,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_destination_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "destination".to_string(),
        label: "Destination".to_string(),
        description: Some("Target cluster and namespace".to_string()),
        field_path: "metadata.destination_namespace".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(150),
        sortable: true,
        filterable: false,
        align: None,
    }
}

fn create_target_revision_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "target_revision".to_string(),
        label: "Revision".to_string(),
        description: Some("Target Git branch, tag, or commit".to_string()),
        field_path: "metadata.target_revision".to_string(),
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

fn create_auto_sync_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "auto_sync".to_string(),
        label: "Auto-Sync".to_string(),
        description: Some("Whether automated sync is enabled".to_string()),
        field_path: "metadata.auto_sync_enabled".to_string(),
        data_type: schema::ColumnDataType::Boolean,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(100),
        sortable: true,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_sync_revision_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "sync_revision".to_string(),
        label: "Synced Revision".to_string(),
        description: Some("Git commit that was synced".to_string()),
        field_path: "metadata.sync_revision".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(150),
        sortable: false,
        filterable: false,
        align: None,
    }
}

fn create_current_revision_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "current_revision".to_string(),
        label: "Current Revision".to_string(),
        description: Some("Actually deployed Git revision".to_string()),
        field_path: "metadata.current_revision".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(150),
        sortable: false,
        filterable: false,
        align: None,
    }
}

fn create_source_path_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "source_path".to_string(),
        label: "Source Path".to_string(),
        description: Some("Path in Git repository".to_string()),
        field_path: "metadata.source_path".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(200),
        sortable: true,
        filterable: false,
        align: None,
    }
}

fn create_repository_url_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "repository_url".to_string(),
        label: "Repository URL".to_string(),
        description: Some("Full Git repository URL".to_string()),
        field_path: "metadata.repo_url".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(300),
        sortable: true,
        filterable: false,
        align: None,
    }
}

fn create_cluster_server_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "cluster_server".to_string(),
        label: "Cluster Server".to_string(),
        description: Some("Kubernetes cluster server URL".to_string()),
        field_path: "metadata.destination_cluster".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(250),
        sortable: true,
        filterable: false,
        align: None,
    }
}

fn create_prune_enabled_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "prune_enabled".to_string(),
        label: "Prune".to_string(),
        description: Some("Auto-sync prunes orphaned resources".to_string()),
        field_path: "metadata.prune_enabled".to_string(),
        data_type: schema::ColumnDataType::Boolean,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(80),
        sortable: true,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_self_heal_enabled_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "self_heal_enabled".to_string(),
        label: "Self-Heal".to_string(),
        description: Some("Auto-sync self-heals manual changes".to_string()),
        field_path: "metadata.self_heal_enabled".to_string(),
        data_type: schema::ColumnDataType::Boolean,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(90),
        sortable: true,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_health_message_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "health_message".to_string(),
        label: "Health Message".to_string(),
        description: Some("Details about application health".to_string()),
        field_path: "metadata.health_message".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(250),
        sortable: false,
        filterable: false,
        align: None,
    }
}

fn create_out_of_sync_count_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "out_of_sync_count".to_string(),
        label: "Out of Sync".to_string(),
        description: Some("Number of resources out of sync".to_string()),
        field_path: "metadata.out_of_sync_count".to_string(),
        data_type: schema::ColumnDataType::Number,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(110),
        sortable: true,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_project_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "project".to_string(),
        label: "Project".to_string(),
        description: Some("ArgoCD project".to_string()),
        field_path: "metadata.project".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(150),
        sortable: true,
        filterable: false,
        align: None,
    }
}

fn create_resource_health_summary_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "resource_health_summary".to_string(),
        label: "Resources".to_string(),
        description: Some("Healthy resources vs total count".to_string()),
        field_path: "metadata.resource_health_summary".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(120),
        sortable: false,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_last_sync_time_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "last_sync_time".to_string(),
        label: "Last Sync".to_string(),
        description: Some("When the application was last successfully synced".to_string()),
        field_path: "metadata.last_sync_time".to_string(),
        data_type: schema::ColumnDataType::DateTime,
        renderer: schema::CellRenderer::DateTime,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(150),
        sortable: true,
        filterable: false,
        align: None,
    }
}

fn create_source_type_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "source_type".to_string(),
        label: "Type".to_string(),
        description: Some("Deployment type: Helm, Kustomize, or Plain manifests".to_string()),
        field_path: "metadata.source_type".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(100),
        sortable: true,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_run_source_type_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "source_type".to_string(),
        label: "Type".to_string(),
        description: Some("Deployment type synced: Helm, Kustomize, or Plain".to_string()),
        field_path: "metadata.source_type".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(100),
        sortable: true,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_run_destination_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "destination".to_string(),
        label: "Namespace".to_string(),
        description: Some("Target namespace for this sync".to_string()),
        field_path: "metadata.destination_namespace".to_string(),
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

fn create_run_source_path_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "source_path".to_string(),
        label: "Path".to_string(),
        description: Some("Path in Git repository that was synced".to_string()),
        field_path: "metadata.source_path".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: true,
        width: Some(200),
        sortable: false,
        filterable: false,
        align: None,
    }
}

fn create_operation_message_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "operation_message".to_string(),
        label: "Operation Message".to_string(),
        description: Some("Sync operation status or error message".to_string()),
        field_path: "metadata.operation_message".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Text,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(300),
        sortable: false,
        filterable: false,
        align: None,
    }
}

fn create_run_app_sync_status_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "app_sync_status".to_string(),
        label: "Current Sync".to_string(),
        description: Some("Current application sync status".to_string()),
        field_path: "metadata.app_sync_status".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(120),
        sortable: false,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_run_app_health_status_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "app_health_status".to_string(),
        label: "Current Health".to_string(),
        description: Some("Current application health status".to_string()),
        field_path: "metadata.app_health_status".to_string(),
        data_type: schema::ColumnDataType::String,
        renderer: schema::CellRenderer::Badge,
        visibility: schema::ColumnVisibility::Always,
        default_visible: false,
        width: Some(120),
        sortable: false,
        filterable: false,
        align: Some("center".to_string()),
    }
}

fn create_helm_chart_column() -> schema::ColumnDefinition {
    schema::ColumnDefinition {
        id: "helm_chart".to_string(),
        label: "Helm Chart".to_string(),
        description: Some("Helm chart name (if Helm deployment)".to_string()),
        field_path: "metadata.helm_chart".to_string(),
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
