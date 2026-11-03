use std::future::Future;
use std::time::Duration;

use pipedash_core::application::LocalNotification;
use pipedash_core::domain::AlertKind;

use super::store::{
    self,
    AlertHost,
};
use super::Result;
use crate::push::{
    acknowledge as acknowledge_push,
    host_push,
    relay_url,
    HostPush,
};
use crate::remote::alerts::{
    fetch,
    AlertPage,
    HostAlert,
};

const HOST_PAGE: u32 = 20;

pub(super) struct HostBatch {
    pub notices: Vec<LocalNotification>,
    pub notified_seq: i64,
    pub lowered: bool,
    pub ack: Option<i64>,
}

pub(super) struct Wake {
    pub scanned: Option<Vec<LocalNotification>>,
    pub hosted: Option<HostBatch>,
}

impl Wake {
    pub fn take_notices(&mut self) -> Vec<LocalNotification> {
        let mut notices = self
            .scanned
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default();
        if let Some(batch) = &mut self.hosted {
            notices.append(&mut batch.notices);
        }

        notices
    }
}

pub(super) async fn gather(
    scan: impl Future<Output = Option<Vec<LocalNotification>>>,
    hosted: impl Future<Output = Option<HostBatch>>,
) -> Wake {
    let (scanned, hosted) = futures::join!(scan, hosted);

    Wake { scanned, hosted }
}

fn host_notice(alert: HostAlert, standalone: bool) -> LocalNotification {
    let (key, pipeline_id, provider_id) = if standalone {
        (format!("alert-host:{}", alert.key), None, None)
    } else {
        (alert.key, alert.pipeline_id, alert.provider_id)
    };

    LocalNotification {
        key,
        title: alert.title,
        body: alert.body,
        pipeline_id,
        provider_id,
        kind: AlertKind::parse(&alert.kind).unwrap_or(AlertKind::Failed),
    }
}

fn batch(after: i64, page: AlertPage, standalone: bool) -> HostBatch {
    if after > page.latest_seq {
        return HostBatch {
            notices: Vec::new(),
            notified_seq: page.latest_seq,
            lowered: true,
            ack: None,
        };
    }
    let notified_seq = page
        .alerts
        .last()
        .map_or(page.latest_seq, |alert| alert.seq);
    let notices = page
        .alerts
        .into_iter()
        .map(|alert| host_notice(alert, standalone))
        .collect();

    HostBatch {
        notices,
        notified_seq,
        lowered: false,
        ack: None,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Plan {
    Skip,
    Fetch,
    FetchAndClear,
    FetchUndelivered(i64),
}

fn plan(status: Option<HostPush>) -> Plan {
    match status {
        Some(HostPush::Active {
            undelivered_after: None,
        }) => Plan::Skip,
        Some(HostPush::Active {
            undelivered_after: Some(after),
        }) => Plan::FetchUndelivered(after),
        Some(HostPush::RelayStopped) => Plan::FetchAndClear,
        Some(HostPush::NoRegistration) | None => Plan::Fetch,
    }
}

pub(super) async fn fetch_host(host: &AlertHost, budget: Duration) -> Option<HostBatch> {
    let started = tokio::time::Instant::now();
    let mut after = host.notified_seq;
    let mut undelivered = false;
    if host.push {
        let status = tokio::time::timeout(budget, host_push(host, relay_url()))
            .await
            .ok()
            .flatten();
        match plan(status) {
            Plan::Skip => {
                return Some(HostBatch {
                    notices: Vec::new(),
                    notified_seq: host.notified_seq,
                    lowered: false,
                    ack: None,
                })
            }
            Plan::FetchUndelivered(from) => {
                after = from;
                undelivered = true;
            }
            Plan::FetchAndClear => {
                if store::set_alert_host_push(host, false).is_err() {
                    tracing::debug!("Couldn't clear the push state of the alert host");
                }
            }
            Plan::Fetch => {}
        }
    }
    let remaining = budget.saturating_sub(started.elapsed());
    match tokio::time::timeout(remaining, fetch(host, after, HOST_PAGE)).await {
        Ok(Ok(page)) => {
            let mut batch = batch(after, page, host.standalone);
            if undelivered && !batch.notices.is_empty() {
                batch.ack = Some(batch.notified_seq);
            }
            Some(batch)
        }
        Ok(Err(error)) => {
            tracing::debug!(%error, "Couldn't check the alert host in the background");
            None
        }
        Err(_) => {
            tracing::debug!("The alert host took too long to answer in the background");
            None
        }
    }
}

pub(super) fn save_cursor(host: &AlertHost, batch: &HostBatch) -> Result<bool> {
    if batch.lowered {
        store::reset_alert_host_seq_for(host, batch.notified_seq)
    } else {
        store::advance_alert_host_for(host, batch.notified_seq)
    }
}

pub(super) async fn acknowledge(host: &AlertHost, through: i64) {
    if !acknowledge_push(host, through).await {
        tracing::debug!("Couldn't tell the alert host which push alerts were shown");
    }
}

#[cfg(test)]
mod tests {
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
            kind: AlertKind::Failed,
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
}
