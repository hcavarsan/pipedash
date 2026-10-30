use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    schema::TableSchema::new()
        .add_table(create_pipeline_runs_table())
        .add_table(create_pipelines_table())
}

fn job_type_column() -> schema::ColumnDefinition {
    metadata_column(
        "job_type",
        "Type",
        "Prow job type (periodic, postsubmit, presubmit, batch)",
        schema::CellRenderer::Badge,
        true,
    )
}

fn cluster_column() -> schema::ColumnDefinition {
    metadata_column(
        "cluster",
        "Cluster",
        "Build cluster that runs the job",
        schema::CellRenderer::Badge,
        false,
    )
}

fn status_index(table: &schema::TableDefinition) -> usize {
    table
        .columns
        .iter()
        .position(|column| column.id == "status")
        .unwrap_or(1)
}

fn create_pipelines_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipelines_table();
    let index = status_index(&table);
    table.columns.insert(index + 1, job_type_column());
    table.columns.push(cluster_column());
    table
}

fn create_pipeline_runs_table() -> schema::TableDefinition {
    let mut table = pipedash_plugin_api::defaults::default_pipeline_runs_table();
    if let Some(run) = table
        .columns
        .iter_mut()
        .find(|column| column.id == "run_number")
    {
        run.label = "Build".to_string();
        run.description =
            Some("Prow build ID, as shown in Deck and in the artifact paths".to_string());
        run.field_path = "metadata.build_id".to_string();
        run.data_type = schema::ColumnDataType::String;
        run.width = Some(190);
    }
    let index = status_index(&table);
    let leading = [
        job_type_column(),
        metadata_column(
            "pull",
            "Pull request",
            "Pull request number the run tested",
            schema::CellRenderer::Text,
            true,
        ),
    ];
    for (offset, column) in leading.into_iter().enumerate() {
        table.columns.insert(index + 1 + offset, column);
    }
    table.columns.extend([
        cluster_column(),
        metadata_column(
            "description",
            "Description",
            "Status text Prow reported for the run",
            schema::CellRenderer::TruncatedText,
            false,
        ),
    ]);
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
