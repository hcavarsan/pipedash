use super::*;

fn alert(seq: i64, key: &str, kind: &str) -> HostAlert {
    HostAlert {
        id: format!("id{seq}"),
        seq,
        key: key.to_owned(),
        kind: kind.to_owned(),
        tone: "danger".to_string(),
        title: format!("{key} title"),
        body: "body".to_owned(),
        pipeline_id: Some("p1".to_owned()),
        provider_id: Some(3),
        provider_ids: Vec::new(),
        finish: None,
        created_at: "2026-11-03T00:00:00Z".to_owned(),
    }
}

fn notice(key: &str) -> LocalNotification {
    LocalNotification {
        key: key.to_owned(),
        title: key.to_owned(),
        body: String::new(),
        pipeline_id: None,
        provider_id: None,
        provider_ids: Vec::new(),
        kind: AlertKind::Failed,
        finish: None,
        level: None,
        link: None,
    }
}

fn hosted(key: &str) -> HostBatch {
    HostBatch {
        notices: vec![notice(key)],
        notified_seq: 9,
        lowered: false,
        ack: None,
    }
}

#[test]
fn only_a_host_that_still_pushes_to_this_device_is_skipped() {
    let active = |undelivered_after| HostPush::Active { undelivered_after };
    assert_eq!(plan(Some(active(None))), Plan::Skip);
    assert_eq!(plan(Some(active(Some(7)))), Plan::FetchUndelivered(7));
    assert_eq!(plan(Some(HostPush::RelayStopped)), Plan::FetchAndClear);
    assert_eq!(plan(Some(HostPush::NoRegistration)), Plan::Fetch);
    assert_eq!(plan(None), Plan::Fetch);
}

fn keys(notices: &[LocalNotification]) -> Vec<&str> {
    notices.iter().map(|n| n.key.as_str()).collect()
}

#[test]
fn host_alerts_become_notifications_and_move_the_cursor() {
    let page = AlertPage {
        alerts: vec![
            alert(4, "pipeline:1", "recovered"),
            alert(6, "x", "unknown"),
        ],
        latest_seq: 6,
    };

    let batch = batch(3, page, false);

    assert_eq!(batch.notified_seq, 6);
    assert!(!batch.lowered);
    assert_eq!(keys(&batch.notices), ["pipeline:1", "x"]);
    assert_eq!(batch.notices[0].kind, AlertKind::Recovered);
    assert_eq!(batch.notices[1].kind, AlertKind::Failed);
    assert_eq!(batch.notices[0].provider_id, Some(3));
}

#[test]
fn a_cursor_ahead_of_the_host_is_lowered_without_notifying() {
    let page = AlertPage {
        alerts: vec![alert(1, "old", "failed")],
        latest_seq: 2,
    };

    let batch = batch(10, page, false);

    assert!(batch.notices.is_empty());
    assert_eq!(batch.notified_seq, 2);
    assert!(batch.lowered);
}

#[test]
fn an_empty_page_skips_to_the_latest_alert() {
    let page = AlertPage {
        alerts: Vec::new(),
        latest_seq: 8,
    };

    assert_eq!(batch(5, page, false).notified_seq, 8);
}

fn standalone_page() -> AlertPage {
    AlertPage {
        alerts: vec![alert(4, "pipeline:p1", "failed")],
        latest_seq: 4,
    }
}

#[test]
fn a_standalone_phone_keeps_server_ids_out_of_its_notifications() {
    let batch = batch(3, standalone_page(), true);

    let notice = &batch.notices[0];
    assert_eq!(notice.key, "alert-host:pipeline:p1");
    assert_eq!(notice.pipeline_id, None);
    assert_eq!(notice.provider_id, None);
    assert_eq!(notice.kind, AlertKind::Failed);
    assert_eq!(notice.title, "pipeline:p1 title");
}

#[test]
fn a_client_phone_keeps_the_server_ids_and_key() {
    let batch = batch(3, standalone_page(), false);

    let notice = &batch.notices[0];
    assert_eq!(notice.key, "pipeline:p1");
    assert_eq!(notice.pipeline_id.as_deref(), Some("p1"));
    assert_eq!(notice.provider_id, Some(3));
}

#[tokio::test]
async fn a_wake_delivers_the_provider_scan_and_the_host_alerts_together() {
    let mut wake = gather(async { Some(vec![notice("pipeline:scan")]) }, async {
        Some(hosted("pipeline:host"))
    })
    .await;

    assert_eq!(
        keys(&wake.take_notices()),
        ["pipeline:scan", "pipeline:host"]
    );
    assert_eq!(wake.hosted.unwrap().notified_seq, 9);
}

#[tokio::test]
async fn a_host_failure_keeps_the_provider_notifications() {
    let mut wake = gather(async { Some(vec![notice("pipeline:scan")]) }, async {
        None
    })
    .await;

    assert_eq!(keys(&wake.take_notices()), ["pipeline:scan"]);
    assert!(wake.scanned.is_some());
    assert!(wake.hosted.is_none());
}

#[tokio::test]
async fn a_provider_failure_keeps_the_host_alerts() {
    let mut wake = gather(async { None }, async { Some(hosted("pipeline:host")) }).await;

    assert_eq!(keys(&wake.take_notices()), ["pipeline:host"]);
    assert!(wake.scanned.is_none());
}

async fn wake_once(host: &AlertHost) -> (Vec<String>, Option<i64>) {
    let mut batch = fetch_host(host, Duration::from_secs(10)).await.unwrap();
    if let Some(through) = batch.ack {
        assert!(acknowledge_push(host, through).await);
    }
    let delivered = batch.notices.drain(..).map(|notice| notice.key).collect();

    (delivered, batch.ack)
}

#[tokio::test]
async fn undelivered_push_alerts_are_delivered_in_pages_until_the_host_reports_none() {
    let host = crate::remote::test_host::start_host_with_relay(Some(relay_url())).await;
    let code = pipedash_web::pairing::create_pairing(&host.state, &host.url.url)
        .unwrap()
        .code;
    let paired = crate::remote::alerts::claim_alert_host(host.url.clone(), &code, true)
        .await
        .unwrap();
    let alert_host = AlertHost {
        push: true,
        ..paired
    };
    host.core
        .device_pushes()
        .set(&alert_host.device_id, "sub-1")
        .await
        .unwrap();
    let mut seqs = Vec::new();
    for n in 0..25 {
        let alert = host
            .core
            .device_alerts()
            .insert(&pipedash_core::domain::NewDeviceAlert {
                key: format!("pipeline:{n}"),
                kind: AlertKind::Failed,
                title: format!("run {n} failed"),
                body: "body".to_owned(),
                pipeline_id: None,
                provider_id: None,
                provider_ids: Vec::new(),
                finish: None,
            })
            .await
            .unwrap();
        host.core
            .device_pushes()
            .mark_undelivered(&alert_host.device_id, "sub-1", alert.seq)
            .await
            .unwrap();
        seqs.push(alert.seq);
    }
    let keys_of = |range: std::ops::Range<usize>| -> Vec<String> {
        range.map(|n| format!("alert-host:pipeline:{n}")).collect()
    };

    let (first, first_ack) = wake_once(&alert_host).await;
    assert_eq!(first, keys_of(0..20));
    assert_eq!(first_ack, Some(seqs[19]));

    let (second, second_ack) = wake_once(&alert_host).await;
    assert_eq!(second, keys_of(20..25));
    assert_eq!(second_ack, Some(seqs[24]));

    let (third, third_ack) = wake_once(&alert_host).await;
    assert!(third.is_empty());
    assert_eq!(third_ack, None);
    assert_eq!(
        host_push(&alert_host, relay_url()).await,
        Some(HostPush::Active {
            undelivered_after: None
        })
    );
}
