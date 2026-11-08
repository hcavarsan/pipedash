use super::reference::ViewRef;
use super::*;

#[test]
fn patch_parent_distinguishes_absent_null_and_value() {
    let absent: ViewPatch = serde_json::from_str("{}").unwrap();
    let top: ViewPatch = serde_json::from_str(r#"{"parent":null}"#).unwrap();
    let nested: ViewPatch = serde_json::from_str(r#"{"parent":"12"}"#).unwrap();
    assert_eq!(absent.parent, None);
    assert_eq!(top.parent, Some(None));
    assert_eq!(nested.parent, Some(Some(ViewRef::Custom(12))));
}

#[test]
fn query_statuses_accept_a_list_or_comma_separated_text() {
    let listed: PipelineViewQuery =
        serde_json::from_str(r#"{"statuses":["running","pending"]}"#).unwrap();
    let joined: PipelineViewQuery =
        serde_json::from_str(r#"{"statuses":"running,pending"}"#).unwrap();
    let absent: PipelineViewQuery = serde_json::from_str("{}").unwrap();
    let expected = vec![PipelineStatus::Running, PipelineStatus::Pending];
    assert_eq!(listed.statuses, expected);
    assert_eq!(joined.statuses, expected);
    assert!(absent.statuses.is_empty());
    assert!(serde_json::from_str::<PipelineViewQuery>(r#"{"statuses":"running,bogus"}"#).is_err());
}
