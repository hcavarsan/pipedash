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

    assert_eq!(state.field_path, "resource_state.label");
    assert_eq!(update.field_path, "metadata.update_available");
    assert_eq!(update.data_type, schema::ColumnDataType::Boolean);
}
