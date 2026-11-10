use pipedash_plugin_api::schema::ColumnVisibility;

use super::create_runs_table;

#[test]
fn the_run_list_shows_status_and_reason_and_leaves_the_message_to_the_details() {
    let table = create_runs_table();
    let column = |id: &str| table.columns.iter().find(|column| column.id == id).unwrap();

    assert!(column("status").default_visible);
    assert!(column("reason").default_visible);
    assert!(!column("message").default_visible);
    assert_eq!(column("message").visibility, ColumnVisibility::WhenPresent);
}
