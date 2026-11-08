use serde_json::json;

use super::*;

fn strategy_json(force: bool, apply_only: bool) -> serde_json::Value {
    serde_json::to_value(sync_strategy(force, apply_only)).unwrap()
}

#[test]
fn apply_only_sync_skips_hooks() {
    assert_eq!(
        strategy_json(false, true),
        json!({ "apply": { "force": false }, "hook": null })
    );
    assert_eq!(
        strategy_json(true, true),
        json!({ "apply": { "force": true }, "hook": null })
    );
}

#[test]
fn force_sync_keeps_hooks() {
    assert_eq!(
        strategy_json(true, false),
        json!({ "apply": null, "hook": { "force": true } })
    );
}

#[test]
fn plain_sync_sends_no_strategy() {
    assert_eq!(strategy_json(false, false), serde_json::Value::Null);
}
