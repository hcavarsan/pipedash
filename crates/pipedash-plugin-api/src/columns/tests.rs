use super::*;
use crate::defaults::default_pipeline_runs_table;

fn ids(table: &TableDefinition) -> Vec<&str> {
    table.columns.iter().map(|c| c.id.as_str()).collect()
}

#[test]
fn builder_defaults_and_overrides() {
    let column = ColumnDefinition::new(
        "x",
        "X",
        "metadata.x",
        ColumnDataType::String,
        CellRenderer::Text,
    );
    assert_eq!(column.visibility, ColumnVisibility::Always);
    assert!(column.default_visible && column.sort == ColumnSort::None && !column.filterable);
    assert!(column.description.is_none() && column.width.is_none());

    let column = column
        .described("d")
        .when_present()
        .hidden()
        .with_width(5)
        .sortable()
        .filterable();
    assert_eq!(column.visibility, ColumnVisibility::WhenPresent);
    assert_eq!(column.description.as_deref(), Some("d"));
    assert!(!column.default_visible && column.sort == ColumnSort::Value && column.filterable);
    assert_eq!(column.width, Some(5));
}

#[test]
fn metadata_column_shape() {
    let column = ColumnDefinition::metadata("state", "State", "desc", CellRenderer::Badge, false);
    assert_eq!(column.field_path, "metadata.state");
    assert_eq!(column.width, Some(140));
    assert!(column.sort == ColumnSort::Value && !column.default_visible);
}

#[test]
fn insert_after_places_columns_or_appends() {
    let mut table = default_pipeline_runs_table();
    table.insert_after("status", [trigger_event_column("e")]);
    assert_eq!(ids(&table)[..3], ["run_number", "status", "event"]);

    table.insert_after("missing", [trigger_event_column("f")]);
    assert_eq!(ids(&table).last(), Some(&"event"));
    assert!(table.column_mut("event").is_some());
    assert!(table.column_mut("nope").is_none());
}

#[test]
fn badge_and_text_are_string_columns() {
    let badge = ColumnDefinition::badge("a", "A", "metadata.a");
    assert_eq!(badge.data_type, ColumnDataType::String);
    assert_eq!(badge.renderer, CellRenderer::Badge);
    let text = ColumnDefinition::text("b", "B", "metadata.b");
    assert_eq!(text.data_type, ColumnDataType::String);
    assert_eq!(text.renderer, CellRenderer::Text);
}
