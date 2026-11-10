use serde_json::json;

use super::*;
use crate::types::SyncStrategyApply;

fn request(revision: Option<&str>) -> SyncRequest {
    SyncRequest {
        revision: revision.map(str::to_string),
        prune: Some(true),
        dry_run: None,
        strategy: None,
    }
}

fn app(extra: Value) -> Value {
    let mut app = json!({
        "metadata": {"name": "web", "resourceVersion": "9"},
        "spec": {"source": {"targetRevision": "main"}},
    });
    for (key, value) in extra.as_object().unwrap() {
        app[key] = value.clone();
    }
    app
}

#[test]
fn sync_sets_only_the_requested_options() {
    let patch = sync_patch(&app(json!({})), "web", &request(None)).unwrap();

    assert_eq!(patch["metadata"]["resourceVersion"], "9");
    assert_eq!(
        patch["operation"]["sync"],
        json!({"prune": true, "dryRun": false})
    );
}

#[test]
fn sync_carries_revision_strategy_and_sources() {
    let app = app(json!({"spec": {"sources": [{"targetRevision": "main"}, {"chart": "redis"}]}}));
    let request = SyncRequest {
        strategy: Some(SyncStrategy {
            apply: Some(SyncStrategyApply { force: Some(true) }),
            hook: None,
        }),
        ..request(Some("abc123"))
    };

    let sync = sync_patch(&app, "web", &request).unwrap()["operation"]["sync"].clone();

    assert_eq!(sync["revision"], "abc123");
    assert_eq!(sync["syncStrategy"], json!({"apply": {"force": true}}));
    assert_eq!(sync["sources"], app["spec"]["sources"]);
}

#[test]
fn sync_keeps_the_sync_options_and_retry_of_the_application() {
    let policy = json!({
        "syncOptions": ["CreateNamespace=true"],
        "retry": {"limit": 3},
    });
    let app = app(json!({"spec": {"syncPolicy": policy}}));

    let operation = sync_patch(&app, "web", &request(None)).unwrap()["operation"].clone();

    assert_eq!(
        operation["sync"]["syncOptions"],
        json!(["CreateNamespace=true"])
    );
    assert_eq!(operation["retry"], json!({"limit": 3}));
}

#[test]
fn sync_is_refused_for_busy_or_deleted_applications() {
    let busy = [
        json!({"operation": {"sync": {}}}),
        json!({"status": {"operationState": {"phase": "Running"}}}),
        json!({"status": {"operationState": {"phase": "Terminating"}}}),
    ];
    for extra in busy {
        let error = sync_patch(&app(extra), "web", &request(None)).unwrap_err();
        assert!(matches!(error, PluginError::ApiError(_)), "{error:?}");
    }

    let deleting = app(json!({"metadata": {"deletionTimestamp": "2026-10-27T10:00:00Z"}}));
    let error = sync_patch(&deleting, "web", &request(None)).unwrap_err();
    assert!(matches!(error, PluginError::InvalidConfig(_)), "{error:?}");

    let finished = app(json!({"status": {"operationState": {"phase": "Succeeded"}}}));
    assert!(sync_patch(&finished, "web", &request(None)).is_ok());
}

#[test]
fn auto_sync_only_accepts_its_own_target_revision() {
    let automated = app(
        json!({"spec": {"source": {"targetRevision": "main"}, "syncPolicy": {"automated": {}}}}),
    );

    let error = sync_patch(&automated, "web", &request(Some("dev"))).unwrap_err();
    assert!(
        matches!(&error, PluginError::InvalidConfig(message) if message == "Cannot sync to dev: auto-sync currently set to main"),
        "{error:?}"
    );
    assert!(sync_patch(&automated, "web", &request(Some("main"))).is_ok());
    assert!(sync_patch(&automated, "web", &request(None)).is_ok());

    let disabled = app(
        json!({"spec": {"source": {"targetRevision": "main"}, "syncPolicy": {"automated": {"enabled": false}}}}),
    );
    assert!(sync_patch(&disabled, "web", &request(Some("dev"))).is_ok());
    assert!(sync_patch(&app(json!({})), "web", &request(Some("dev"))).is_ok());
}

#[test]
fn terminate_needs_a_running_operation() {
    let running =
        app(json!({"operation": {"sync": {}}, "status": {"operationState": {"phase": "Running"}}}));

    let patch = terminate_patch(&running, "web").unwrap();

    assert_eq!(patch["status"]["operationState"]["phase"], "Terminating");
    assert_eq!(patch["metadata"]["resourceVersion"], "9");
    for idle in [
        app(json!({})),
        app(json!({"status": {"operationState": {"phase": "Succeeded"}}})),
        app(json!({"operation": {"sync": {}}})),
    ] {
        assert!(matches!(
            terminate_patch(&idle, "web"),
            Err(PluginError::InvalidConfig(_))
        ));
    }
}
