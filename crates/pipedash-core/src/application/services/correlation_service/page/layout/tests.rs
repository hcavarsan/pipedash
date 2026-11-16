use super::*;
use crate::domain::application::dto::FlowNodeKind;

fn node(id: &str, title: &str, column: u32) -> FlowNode {
    FlowNode {
        id: id.into(),
        kind: FlowNodeKind::Target,
        column,
        row: 0,
        title: title.into(),
        subtitle: None,
        status: None,
        pipeline_id: None,
        count: 1,
    }
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(from, to)| ((*from).to_string(), (*to).to_string()))
        .collect()
}

fn names(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

type Pair<'a> = (&'a str, &'a str);
type Case<'a> = (&'a [&'a str], &'a [Pair<'a>], &'a [(&'a str, u32)]);

#[test]
fn direction_dag_depth_is_the_longest_path() {
    let cases: [Case<'_>; 3] = [
        (
            &["qa", "prod"],
            &[("qa", "prod")],
            &[("qa", 3), ("prod", 4)],
        ),
        (
            &["dev", "qa", "prod"],
            &[("dev", "qa"), ("qa", "prod"), ("dev", "prod")],
            &[("dev", 3), ("qa", 4), ("prod", 5)],
        ),
        (
            &["a", "b", "c", "d"],
            &[("a", "d"), ("b", "d"), ("c", "b")],
            &[("a", 3), ("b", 4), ("c", 3), ("d", 5)],
        ),
    ];
    for (groups, directions, expected) in cases {
        let found = target_columns(&names(groups), &pairs(directions));
        for (group, column) in expected {
            assert_eq!(found[*group], *column, "{group} in {directions:?}");
        }
    }
}

#[test]
fn a_direction_cycle_still_terminates() {
    let found = target_columns(&names(&["a", "b"]), &pairs(&[("a", "b"), ("b", "a")]));
    assert_eq!(found.len(), 2);
}
#[test]
fn undirected_targets_share_column_three() {
    let found = target_columns(&names(&["qa", "staging", "prod"]), &[]);
    assert!(found.values().all(|column| *column == FIRST_TARGET_COLUMN));
}

#[test]
fn unknown_groups_in_directions_are_ignored() {
    let found = target_columns(&names(&["qa"]), &pairs(&[("qa", "gone"), ("gone", "qa")]));
    assert_eq!(found["qa"], FIRST_TARGET_COLUMN);
}

#[test]
fn artifact_overflow_keeps_four_and_counts_the_rest() {
    let cases = [(0, 0, 0), (4, 4, 0), (5, 4, 1), (9, 4, 5)];
    for (total, shown, hidden) in cases {
        let (kept, overflow) = split_artifacts((0..total).collect::<Vec<_>>());
        assert_eq!((kept.len(), overflow), (shown, hidden), "{total} artifacts");
    }
}

#[test]
fn rows_follow_title_then_id_from_zero() {
    let mut nodes = vec![
        node("b", "Beta", 3),
        node("z", "Alpha", 3),
        node("a", "Alpha", 3),
        node("only", "Solo", 4),
    ];
    let (columns, rows) = arrange(&mut nodes);
    let row = |id: &str| nodes.iter().find(|n| n.id == id).map(|n| n.row);
    assert_eq!(row("a"), Some(0));
    assert_eq!(row("z"), Some(1));
    assert_eq!(row("b"), Some(2));
    assert_eq!(row("only"), Some(0));
    assert_eq!((columns, rows), (5, 3));
}

#[test]
fn empty_flow_has_no_dimensions() {
    assert_eq!(arrange(&mut []), (0, 0));
}
