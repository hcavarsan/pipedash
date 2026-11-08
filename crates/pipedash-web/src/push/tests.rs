use std::future::Future;
use std::time::Duration;

use pipedash_core::domain::{
    AlertKind,
    NewDeviceAlert,
    NotificationPreferences,
};
use pipedash_core::event::{
    CoreEvent,
    EventBus,
};

use super::*;
use crate::state::AppState;
use crate::test_support::{
    hosted,
    stub_server,
};

async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let wait = async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };

    if tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .is_err()
    {
        panic!("timed out waiting for {what}");
    }
}

async fn subscribed(state: &AppState, subscriptions: &[&str]) {
    let core = state.core().await.unwrap();
    for subscription in subscriptions {
        let (device, _) = core.pair_device(subscription, "ios").await.unwrap();
        core.device_pushes()
            .set(&device.id, subscription)
            .await
            .unwrap();
    }
}

fn recorded(id: &str) -> CoreEvent {
    CoreEvent::AlertRecorded {
        id: id.into(),
        seq: 1,
    }
}

#[tokio::test]
async fn a_recorded_alert_reaches_every_subscribed_device_with_only_its_id() {
    let host = hosted("alert-push").await;
    let relay = stub_server().await;
    let state = host
        .state
        .clone()
        .with_push_relay(&format!("{}/", relay.url));
    subscribed(&state, &["sub-1", "sub-2"]).await;
    spawn_alert_push(state.clone());

    state.ws_event_bus.emit(recorded("a1")).await;
    eventually("both pushes", || async { relay.requests().len() == 2 }).await;

    let mut requests = relay.requests();
    requests.sort_by_key(|request| request["subscription_id"].to_string());
    assert_eq!(
        requests,
        vec![
            serde_json::json!({ "subscription_id": "sub-1", "alert_id": "a1" }),
            serde_json::json!({ "subscription_id": "sub-2", "alert_id": "a1" }),
        ]
    );
    let core = state.core().await.unwrap();
    eventually("both successes to be recorded", || async {
        let pushes = core.device_pushes().list().await.unwrap();
        pushes.len() == 2 && pushes.iter().all(|push| push.last_success_at.is_some())
    })
    .await;
}

#[tokio::test]
async fn a_device_that_turned_off_an_alert_kind_is_not_pushed_it() {
    let host = hosted("alert-push-preferences").await;
    let relay = stub_server().await;
    let state = host.state.clone().with_push_relay(&relay.url);
    subscribed(&state, &["sub-1", "sub-2"]).await;
    let core = state.core().await.unwrap();
    let quiet = core.device_pushes().list().await.unwrap();
    let quiet = quiet
        .iter()
        .find(|push| push.relay_subscription_id == "sub-2")
        .unwrap();
    core.devices()
        .set_notification_preferences(
            &quiet.device_id,
            &NotificationPreferences {
                long_run: false,
                ..NotificationPreferences::default()
            },
        )
        .await
        .unwrap();
    let alert = |kind| NewDeviceAlert {
        key: "pipeline:api".into(),
        kind,
        title: "api".into(),
        body: String::new(),
        pipeline_id: Some("api".into()),
        provider_id: Some(1),
        provider_ids: vec![1],
        finish: None,
    };
    let long_run = core
        .device_alerts()
        .insert(&alert(AlertKind::LongRun))
        .await
        .unwrap();
    let failed = core
        .device_alerts()
        .insert(&alert(AlertKind::Failed))
        .await
        .unwrap();
    spawn_alert_push(state.clone());

    state.ws_event_bus.emit(recorded(&long_run.id)).await;
    eventually("the long run push", || async {
        !relay.requests().is_empty()
    })
    .await;
    state.ws_event_bus.emit(recorded(&failed.id)).await;
    eventually("the failure pushes", || async {
        relay.requests().len() == 3
    })
    .await;

    let mut requests = relay.requests();
    requests.sort_by_key(|request| request.to_string());
    let mut expected = vec![
        serde_json::json!({ "subscription_id": "sub-1", "alert_id": long_run.id }),
        serde_json::json!({ "subscription_id": "sub-1", "alert_id": failed.id }),
        serde_json::json!({ "subscription_id": "sub-2", "alert_id": failed.id }),
    ];
    expected.sort_by_key(|request| request.to_string());
    assert_eq!(requests, expected);
}

#[tokio::test]
async fn a_finished_run_is_pushed_only_to_the_device_watching_it() {
    let host = hosted("alert-push-watch").await;
    let relay = stub_server().await;
    let state = host.state.clone().with_push_relay(&relay.url);
    subscribed(&state, &["sub-1", "sub-2"]).await;
    let core = state.core().await.unwrap();
    let pushes = core.device_pushes().list().await.unwrap();
    let watcher = pushes
        .iter()
        .find(|push| push.relay_subscription_id == "sub-2")
        .unwrap();
    core.devices()
        .set_notification_preferences(
            &watcher.device_id,
            &NotificationPreferences {
                watched_pipelines: vec!["api".into()],
                ..NotificationPreferences::default()
            },
        )
        .await
        .unwrap();
    let finished = core
        .device_alerts()
        .insert(&NewDeviceAlert {
            key: "pipeline:api".into(),
            kind: AlertKind::Finished,
            title: "api succeeded".into(),
            body: String::new(),
            pipeline_id: Some("api".into()),
            provider_id: Some(1),
            provider_ids: vec![1],
            finish: None,
        })
        .await
        .unwrap();
    spawn_alert_push(state.clone());

    state.ws_event_bus.emit(recorded(&finished.id)).await;
    eventually("the watcher push", || async {
        !relay.requests().is_empty()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(
        relay.requests(),
        vec![serde_json::json!({ "subscription_id": "sub-2", "alert_id": finished.id })]
    );
}

#[tokio::test]
async fn a_device_the_relay_no_longer_knows_is_forgotten_after_an_alert_push() {
    let host = hosted("alert-push-gone").await;
    let relay = stub_server().await;
    relay.reply_with(404);
    let state = host.state.clone().with_push_relay(&relay.url);
    subscribed(&state, &["sub-1"]).await;
    spawn_alert_push(state.clone());

    state.ws_event_bus.emit(recorded("a1")).await;

    let core = state.core().await.unwrap();
    eventually("the device to be forgotten", || async {
        core.device_pushes().list().await.unwrap().is_empty()
    })
    .await;
    assert_eq!(relay.requests().len(), 1);
}

#[tokio::test]
async fn nothing_is_sent_without_a_relay_or_after_the_host_stops() {
    let host = hosted("alert-push-idle").await;
    let relay = stub_server().await;
    subscribed(&host.state, &["sub-1"]).await;
    spawn_alert_push(host.state.clone());
    host.state.ws_event_bus.emit(recorded("a1")).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(relay.requests().is_empty());

    let state = host.state.clone().with_push_relay(&relay.url);
    spawn_alert_push(state.clone());
    state.closing.send_replace(true);
    tokio::time::sleep(Duration::from_millis(100)).await;
    state.ws_event_bus.emit(recorded("a2")).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(relay.requests().is_empty());
}

#[tokio::test]
async fn a_host_reads_the_relay_from_its_builder() {
    let host = hosted("alert-relay-builder").await;
    assert_eq!(host.state.push_relay_url().await, None);
    let state = host
        .state
        .clone()
        .with_push_relay("https://relay.example.org/");
    assert_eq!(
        state.push_relay_url().await.as_deref(),
        Some("https://relay.example.org")
    );
}
