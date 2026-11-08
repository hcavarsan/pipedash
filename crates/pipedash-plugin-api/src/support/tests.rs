use std::future::ready;

use super::*;

#[test]
fn organizations_are_sorted_unique_and_carry_the_description() {
    let orgs = organizations(["b", "a", "b"], Some("Namespace"));

    let ids: Vec<_> = orgs.iter().map(|org| org.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert!(orgs
        .iter()
        .all(|org| org.name == org.id && org.description.as_deref() == Some("Namespace")));
    assert!(organizations(["a"], None)[0].description.is_none());
}

#[test]
fn require_configured_rejects_empty_lists() {
    assert!(matches!(
        require_configured(Vec::<u8>::new(), "jobs"),
        Err(PluginError::InvalidConfig(message)) if message == "No jobs configured"
    ));
    assert_eq!(require_configured(vec![1], "jobs").unwrap(), [1]);
}

#[tokio::test]
async fn try_buffered_keeps_order_and_fails_fast() {
    let ok = try_buffered(vec![ready(Ok(1)), ready(Ok(2))], 2).await;
    assert_eq!(ok.unwrap(), [1, 2]);

    let failed = try_buffered(
        vec![
            ready(Ok(1)),
            ready(Err(PluginError::ApiError("boom".into()))),
        ],
        2,
    )
    .await;
    assert!(matches!(failed, Err(PluginError::ApiError(_))));
}

#[tokio::test]
async fn collect_tolerant_skips_permanent_errors_unless_nothing_succeeds() {
    let mut logged = 0;
    let mixed = collect_tolerant(
        vec![
            ready(Ok(1)),
            ready(Err(PluginError::ApiError("bad".into()))),
        ],
        2,
        |_| logged += 1,
    )
    .await;
    assert_eq!(mixed.unwrap(), [1]);
    assert_eq!(logged, 1);

    let none: PluginResult<Vec<u8>> = collect_tolerant(
        vec![ready(Err(PluginError::ApiError("bad".into())))],
        2,
        |_| {},
    )
    .await;
    assert!(matches!(none, Err(PluginError::ApiError(_))));

    let transient = collect_tolerant(
        vec![
            ready(Ok(1)),
            ready(Err(PluginError::NetworkError("down".into()))),
        ],
        2,
        |_| {},
    )
    .await;
    assert!(matches!(transient, Err(PluginError::NetworkError(_))));
}

#[test]
fn id_checks_name_the_offending_value() {
    assert_eq!(digit_id("42", "job ID").unwrap(), "42");
    assert!(matches!(
        digit_id("4x", "job ID"),
        Err(PluginError::InvalidConfig(message)) if message == "Invalid job ID: 4x"
    ));
    assert!(digit_id("", "job ID").is_err());
    assert_eq!(uuid_id("ab-12", "step").unwrap(), "ab-12");
    assert!(uuid_id("zz", "step").is_err());
}

#[test]
fn resume_position_restarts_only_when_a_cursor_was_rejected() {
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let mut sink = LogSink::new(tx);
    let cursor = LogCursor::new(&7u64).unwrap();

    assert_eq!(
        resume_position::<u64>(None, &mut sink, |_| true).unwrap(),
        None
    );
    assert!(!sink.restarted());
    assert_eq!(
        resume_position::<u64>(Some(&cursor), &mut sink, |n| *n == 7).unwrap(),
        Some(7)
    );
    assert!(!sink.restarted());
    assert_eq!(
        resume_position::<u64>(Some(&cursor), &mut sink, |n| *n == 8).unwrap(),
        None
    );
    assert!(sink.restarted());
}
