use std::future::Future;
use std::time::Duration;

use pipedash_core::application::{
    refresh_notifications,
    LocalNotification,
    NotificationState,
};
use pipedash_core::domain::AlertKind;

use super::store::{
    self,
    AlertHost,
    Authorization,
};
use super::{
    Result,
    REFRESH_BUDGET,
};
use crate::push::flow::{
    host_push,
    HostPush,
};
use crate::push::{
    acknowledge as acknowledge_push,
    relay_url,
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

pub(super) async fn scan_providers(
    authorization: Option<&Authorization>, observations: &mut NotificationState,
) -> Option<Vec<LocalNotification>> {
    let authorization = authorization?;

    refresh_notifications(
        &authorization.providers,
        &authorization.proxy_url,
        authorization.status_pages_enabled,
        observations,
        REFRESH_BUDGET,
        crate::plugin_registry::plugin_registry(),
    )
    .await
    .ok()
}

pub(super) async fn fetch_hosted(host: Option<&AlertHost>) -> Option<HostBatch> {
    fetch_host(host?, REFRESH_BUDGET).await
}

fn host_notice(alert: HostAlert, standalone: bool) -> LocalNotification {
    let provider_ids = alert.affected_providers();
    let (key, pipeline_id, provider_id, provider_ids, finish) = if standalone {
        (
            format!("alert-host:{}", alert.key),
            None,
            None,
            Vec::new(),
            None,
        )
    } else {
        (
            alert.key,
            alert.pipeline_id,
            alert.provider_id,
            provider_ids,
            alert.finish,
        )
    };

    LocalNotification {
        key,
        title: alert.title,
        body: alert.body,
        pipeline_id,
        provider_id,
        provider_ids,
        kind: AlertKind::parse(&alert.kind).unwrap_or(AlertKind::Failed),
        finish,
        level: None,
        link: None,
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
mod tests;
