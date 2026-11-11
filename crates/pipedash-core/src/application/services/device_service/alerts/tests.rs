use std::sync::Arc;

use super::*;
use crate::domain::{
    AlertKind,
    NewDeviceAlert,
};
use crate::tests::context;
use crate::MemoryTokenStore;

fn new_alert(kind: AlertKind, key: &str) -> NewDeviceAlert {
    NewDeviceAlert {
        key: key.into(),
        kind,
        title: key.into(),
        body: key.into(),
        pipeline_id: None,
        provider_id: None,
        provider_ids: Vec::new(),
        finish: None,
    }
}

#[tokio::test]
async fn an_alert_recorded_during_the_read_is_never_hidden_behind_latest_seq() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path(), Arc::new(MemoryTokenStore::new())).await;
    let alerts = core.device_alerts();
    let first = alerts
        .insert(&new_alert(AlertKind::Failed, "pipeline:a"))
        .await
        .unwrap();

    let page = read_page(
        async {
            let latest = alerts.latest_seq().await;
            alerts
                .insert(&new_alert(AlertKind::Failed, "pipeline:b"))
                .await
                .unwrap();
            latest
        },
        alerts.after(0, 100),
    )
    .await
    .unwrap();

    assert_eq!(page.latest_seq, first.seq);
    assert!(page.alerts.iter().any(|view| view.alert.seq == first.seq));
    let listed = page.alerts.iter().map(|view| view.alert.seq).max().unwrap();
    assert!(page.latest_seq <= listed);
}

#[tokio::test]
async fn a_device_only_sees_the_alerts_its_preferences_allow() {
    let dir = tempfile::tempdir().unwrap();
    let core = context(dir.path(), Arc::new(MemoryTokenStore::new())).await;
    let (device, _) = core.pair_device("Pixel", "ios").await.unwrap();
    core.device_alerts()
        .insert(&new_alert(AlertKind::Failed, "pipeline:a"))
        .await
        .unwrap();
    let recovered = core
        .device_alerts()
        .insert(&new_alert(AlertKind::Recovered, "pipeline:b"))
        .await
        .unwrap();
    core.devices()
        .set_notification_preferences(
            &device.id,
            &crate::domain::NotificationPreferences {
                failed: false,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let everything = core.alert_page(None, None, None).await.unwrap();
    let shown = core.alert_page(Some(&device.id), None, None).await.unwrap();

    assert_eq!(everything.alerts.len(), 2);
    assert_eq!(shown.alerts.len(), 1);
    assert_eq!(shown.alerts[0].alert.id, recovered.id);
    assert_eq!(shown.latest_seq, recovered.seq);
}
