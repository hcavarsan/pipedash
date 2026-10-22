use pipedash_plugin_api::schema::{
    CellRenderer,
    ColumnDataType,
    ColumnDefinition,
    ColumnVisibility,
    TableDefinition,
    TableSchema,
};

pub fn create_table_schema() -> TableSchema {
    TableSchema::new()
        .add_table(create_runs_table())
        .add_table(create_pipelines_table())
}

fn column(
    id: &str, label: &str, description: &str, field_path: &str, data_type: ColumnDataType,
    renderer: CellRenderer, width: u32,
) -> ColumnDefinition {
    ColumnDefinition {
        id: id.to_string(),
        label: label.to_string(),
        description: Some(description.to_string()),
        field_path: field_path.to_string(),
        data_type,
        renderer,
        visibility: ColumnVisibility::WhenPresent,
        default_visible: true,
        width: Some(width),
        sortable: true,
        filterable: false,
    }
}

fn always(column: ColumnDefinition) -> ColumnDefinition {
    ColumnDefinition {
        visibility: ColumnVisibility::Always,
        ..column
    }
}

fn hidden(column: ColumnDefinition) -> ColumnDefinition {
    ColumnDefinition {
        default_visible: false,
        ..column
    }
}

fn filterable(column: ColumnDefinition) -> ColumnDefinition {
    ColumnDefinition {
        filterable: true,
        ..column
    }
}

fn create_runs_table() -> TableDefinition {
    TableDefinition {
        id: "pipeline_runs".to_string(),
        name: "Reconciliations".to_string(),
        description: Some(
            "Reconciliation history Flux keeps in status.history, plus the current status when it is not covered by that history. Flux does not keep a complete execution log."
                .to_string(),
        ),
        columns: vec![
            always(column(
                "run_label",
                "Record",
                "Reconciliation, Helm release version, or the current state reported by Flux",
                "metadata.run_label",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                170,
            )),
            always(filterable(column(
                "status",
                "Status",
                "Pipedash status derived from the Flux record",
                "status",
                ColumnDataType::Status,
                CellRenderer::StatusBadge,
                130,
            ))),
            column(
                "reason",
                "Reason",
                "Flux condition reason, reconciliation result, or Helm release status",
                "metadata.reason",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                170,
            ),
            hidden(column(
                "message",
                "Message",
                "Message of the Flux Ready condition. History records only carry it when they match the current state.",
                "metadata.message",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                360,
            )),
            column(
                "revision",
                "Revision",
                "Source revision for a Kustomization, chart version for a HelmRelease",
                "metadata.revision",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                220,
            ),
            hidden(column(
                "release_version",
                "Release",
                "Helm release version",
                "metadata.release_version",
                ColumnDataType::Number,
                CellRenderer::Text,
                100,
            )),
            hidden(column(
                "chart",
                "Chart",
                "Helm chart name and version",
                "metadata.chart",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                170,
            )),
            always(column(
                "started_at",
                "Observed",
                "Timestamp taken from Flux status, newest first by default. See Time source for what it represents; it is not always the start of a reconciliation.",
                "started_at",
                ColumnDataType::DateTime,
                CellRenderer::DateTime,
                180,
            )),
            hidden(column(
                "time_source",
                "Time source",
                "Flux status field the Observed timestamp comes from",
                "metadata.time_source",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                240,
            )),
            hidden(column(
                "concluded_at",
                "Last reconciled",
                "When Flux recorded the end of the reconciliation",
                "concluded_at",
                ColumnDataType::DateTime,
                CellRenderer::DateTime,
                180,
            )),
            column(
                "duration_seconds",
                "Duration",
                "Duration of the reconciliation or Helm action reported by Flux",
                "duration_seconds",
                ColumnDataType::Duration,
                CellRenderer::Duration,
                120,
            ),
            hidden(column(
                "total_reconciliations",
                "Reconciliations",
                "How many reconciliations Flux grouped into this record",
                "metadata.total_reconciliations",
                ColumnDataType::Number,
                CellRenderer::Text,
                140,
            )),
            hidden(column(
                "record_source",
                "Source",
                "status.history from Flux, or the current snapshot of status conditions",
                "metadata.record_source",
                ColumnDataType::String,
                CellRenderer::Badge,
                150,
            )),
            hidden(column(
                "commit_sha",
                "Commit",
                "Commit digest taken from the source revision",
                "commit_sha",
                ColumnDataType::String,
                CellRenderer::Commit,
                140,
            )),
            hidden(column(
                "branch",
                "Ref",
                "Branch or tag taken from the source revision",
                "branch",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                140,
            )),
            hidden(column(
                "digest",
                "Digest",
                "Digest Flux recorded for the reconciled manifests or Helm release",
                "metadata.digest",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                200,
            )),
            hidden(column(
                "run_number",
                "Record ID",
                "Numeric identity of the record",
                "run_number",
                ColumnDataType::Number,
                CellRenderer::Text,
                160,
            )),
        ],
        default_sort_column: Some("started_at".to_string()),
        default_sort_direction: Some("desc".to_string()),
    }
}

fn create_pipelines_table() -> TableDefinition {
    TableDefinition {
        id: "pipelines".to_string(),
        name: "Flux resources".to_string(),
        description: Some("Flux Kustomizations and HelmReleases".to_string()),
        columns: vec![
            always(filterable(column(
                "name",
                "Resource",
                "Flux kind and resource name",
                "name",
                ColumnDataType::String,
                CellRenderer::Text,
                240,
            ))),
            always(filterable(column(
                "status",
                "Status",
                "Pipedash status derived from Flux conditions",
                "status",
                ColumnDataType::Status,
                CellRenderer::StatusBadge,
                130,
            ))),
            always(filterable(column(
                "namespace",
                "Namespace",
                "Kubernetes namespace",
                "metadata.namespace",
                ColumnDataType::String,
                CellRenderer::Badge,
                160,
            ))),
            column(
                "reason",
                "Reason",
                "Reason of the Flux condition that explains the status",
                "metadata.reason",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                170,
            ),
            column(
                "message",
                "Message",
                "Message of the Flux condition that explains the status",
                "metadata.message",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                360,
            ),
            column(
                "revision",
                "Revision",
                "Last attempted source revision or chart version",
                "metadata.revision",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                220,
            ),
            column(
                "source_ref",
                "Source",
                "Flux source the resource reconciles from",
                "metadata.source_ref",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                240,
            ),
            column(
                "chart",
                "Chart",
                "Helm chart name and version",
                "metadata.chart",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                170,
            ),
            column(
                "release_version",
                "Release",
                "Latest Helm release version recorded by Flux",
                "metadata.release_version",
                ColumnDataType::Number,
                CellRenderer::Text,
                100,
            ),
            column(
                "suspended",
                "Suspended",
                "Whether spec.suspend is set",
                "metadata.suspended",
                ColumnDataType::Boolean,
                CellRenderer::Badge,
                120,
            ),
            column(
                "last_run",
                "Last reconciliation",
                "Newest reconciliation time recorded by Flux, or the time of the current status when no history covers it",
                "last_run",
                ColumnDataType::DateTime,
                CellRenderer::DateTime,
                180,
            ),
            hidden(column(
                "repository",
                "Namespace/source",
                "Namespace and source name used to group resources",
                "repository",
                ColumnDataType::String,
                CellRenderer::TruncatedText,
                200,
            )),
            hidden(column(
                "branch",
                "Ref",
                "Branch or tag taken from the source revision",
                "branch",
                ColumnDataType::String,
                CellRenderer::Badge,
                140,
            )),
        ],
        default_sort_column: Some("name".to_string()),
        default_sort_direction: Some("asc".to_string()),
    }
}
