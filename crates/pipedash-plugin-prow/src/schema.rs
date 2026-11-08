use pipedash_plugin_api::*;
use schema::{
    CellRenderer,
    ColumnDataType,
    ColumnDefinition,
};

fn job_type_column() -> ColumnDefinition {
    ColumnDefinition::metadata(
        "job_type",
        "Type",
        "Prow job type (periodic, postsubmit, presubmit, batch)",
        CellRenderer::Badge,
        true,
    )
}

fn cluster_column() -> ColumnDefinition {
    ColumnDefinition::metadata(
        "cluster",
        "Cluster",
        "Build cluster that runs the job",
        CellRenderer::Badge,
        false,
    )
}

pub fn create_table_schema() -> schema::TableSchema {
    let mut runs = defaults::default_pipeline_runs_table();
    if let Some(run) = runs.column_mut("run_number") {
        run.label = "Build".to_string();
        run.description =
            Some("Prow build ID, as shown in Deck and in the artifact paths".to_string());
        run.field_path = "metadata.build_id".to_string();
        run.data_type = ColumnDataType::String;
        run.width = Some(190);
    }
    runs.insert_after(
        "status",
        [
            job_type_column(),
            ColumnDefinition::metadata(
                "pull",
                "Pull request",
                "Pull request number the run tested",
                CellRenderer::Text,
                true,
            ),
        ],
    );
    runs.columns.extend([
        cluster_column(),
        ColumnDefinition::metadata(
            "description",
            "Description",
            "Status text Prow reported for the run",
            CellRenderer::TruncatedText,
            false,
        ),
    ]);

    let mut pipelines = defaults::default_pipelines_table();
    pipelines.insert_after("status", [job_type_column()]);
    pipelines.columns.push(cluster_column());
    defaults::table_schema(runs, pipelines)
}
