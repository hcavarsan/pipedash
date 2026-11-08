use super::{
    tree_from_nodes,
    with_layout,
};
use crate::application::test_support::stub_run;
use crate::domain::{
    LogAvailability,
    LogNode,
    LogNodeKind,
    PipelineStatus,
    RunLogTree,
};

fn node(id: &str, parent: Option<&str>, kind: LogNodeKind) -> LogNode {
    LogNode {
        id: id.to_string(),
        parent_id: parent.map(str::to_string),
        kind,
        name: id.to_string(),
        status: PipelineStatus::Success,
        started_at: None,
        finished_at: None,
        log: Some(LogAvailability::Ready),
        web_url: None,
    }
}

fn stage(id: &str) -> LogNode {
    LogNode {
        log: None,
        ..node(id, None, LogNodeKind::Stage)
    }
}

fn tree_of(nodes: Vec<LogNode>) -> RunLogTree {
    tree_from_nodes(&stub_run("p", 1, 1), nodes)
}

fn groups(tree: &RunLogTree) -> Vec<(Option<&str>, Vec<&str>, Vec<&str>)> {
    tree.groups
        .iter()
        .map(|group| {
            (
                group.stage.as_deref(),
                group.node_ids.iter().map(String::as_str).collect(),
                group
                    .failed_first_node_ids
                    .iter()
                    .map(String::as_str)
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn jobs_group_under_their_nearest_stage_and_failed_jobs_lead_the_failed_first_order() {
    let tree = tree_of(vec![
        stage("build"),
        node("a", Some("build"), LogNodeKind::Job),
        LogNode {
            status: PipelineStatus::Failed,
            ..node("b", Some("build"), LogNodeKind::Job)
        },
        LogNode {
            log: None,
            ..node("step", Some("b"), LogNodeKind::Step)
        },
        node("lone", None, LogNodeKind::Job),
    ]);

    assert_eq!(
        groups(&tree),
        vec![
            (Some("build"), vec!["a", "b"], vec!["b", "a"]),
            (None, vec!["lone"], vec!["lone"]),
        ]
    );
}

#[test]
fn step_ids_are_listed_per_parent_in_tree_order() {
    let tree = tree_of(vec![
        node("job", None, LogNodeKind::Job),
        node("s2", Some("job"), LogNodeKind::Step),
        node("other", None, LogNodeKind::Job),
        node("s1", Some("job"), LogNodeKind::Step),
        node("sub", Some("job"), LogNodeKind::Job),
    ]);

    assert_eq!(tree.step_ids_by_node.len(), 1);
    assert_eq!(tree.step_ids_by_node["job"], vec!["s2", "s1"]);
}

#[test]
fn empty_input_orphan_parents_and_duplicate_stage_names_group_predictably() {
    let empty = tree_of(Vec::new());
    assert!(empty.groups.is_empty());
    assert!(empty.step_ids_by_node.is_empty());

    let tree = tree_of(vec![
        LogNode {
            name: "build".to_string(),
            ..stage("s1")
        },
        LogNode {
            name: "build".to_string(),
            ..stage("s2")
        },
        node("a", Some("s1"), LogNodeKind::Job),
        node("b", Some("s2"), LogNodeKind::Job),
        node("orphan", Some("missing"), LogNodeKind::Job),
    ]);

    assert_eq!(
        groups(&tree),
        vec![
            (Some("build"), vec!["a", "b"], vec!["a", "b"]),
            (None, vec!["orphan"], vec!["orphan"]),
        ]
    );
}

#[test]
fn cyclic_parent_chains_terminate_and_leave_the_job_ungrouped() {
    let tree = tree_of(vec![
        node("self", Some("self"), LogNodeKind::Step),
        node("x", Some("y"), LogNodeKind::Step),
        node("y", Some("x"), LogNodeKind::Step),
    ]);
    assert_eq!(
        groups(&tree),
        vec![(None, vec!["self", "x", "y"], vec!["self", "x", "y"])]
    );

    let mut stored = tree_of(vec![
        node("x", None, LogNodeKind::Step),
        node("y", Some("x"), LogNodeKind::Step),
    ]);
    stored.nodes[0].parent_id = Some("y".to_string());
    let stored = with_layout(stored);
    assert_eq!(
        groups(&stored),
        vec![(None, vec!["x", "y"], vec!["x", "y"])]
    );
}

#[test]
fn stored_trees_without_layout_fields_are_laid_out_when_restored() {
    let tree = tree_of(vec![
        stage("build"),
        node("job", Some("build"), LogNodeKind::Job),
        node("step", Some("job"), LogNodeKind::Step),
    ]);
    let mut legacy = serde_json::to_value(&tree).unwrap();
    legacy.as_object_mut().unwrap().remove("groups");
    legacy.as_object_mut().unwrap().remove("step_ids_by_node");

    let restored: RunLogTree = serde_json::from_value(legacy).unwrap();

    assert!(restored.groups.is_empty());
    assert_eq!(with_layout(restored), tree);
}
