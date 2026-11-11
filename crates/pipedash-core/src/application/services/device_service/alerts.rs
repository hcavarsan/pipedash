use std::future::Future;

use serde::Serialize;

use crate::domain::{
    DeviceAlert,
    DomainResult,
    NotificationPreferences,
};
use crate::infrastructure::database::DeviceAlertRepository;
use crate::CoreContext;

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 100;

#[derive(Debug, Serialize)]
pub struct AlertView {
    #[serde(flatten)]
    pub alert: DeviceAlert,
    pub tone: &'static str,
}

impl From<DeviceAlert> for AlertView {
    fn from(alert: DeviceAlert) -> Self {
        let tone = alert.kind.tone();

        Self { alert, tone }
    }
}

#[derive(Debug, Serialize)]
pub struct AlertPage {
    pub alerts: Vec<AlertView>,
    pub latest_seq: i64,
}

async fn read_page(
    latest_seq: impl Future<Output = DomainResult<i64>>,
    page: impl Future<Output = DomainResult<Vec<DeviceAlert>>>,
) -> DomainResult<AlertPage> {
    let latest_seq = latest_seq.await?;
    let alerts = page.await?;

    Ok(AlertPage {
        alerts: alerts.into_iter().map(AlertView::from).collect(),
        latest_seq,
    })
}

async fn shown_after(
    alerts: &DeviceAlertRepository, preferences: &NotificationPreferences, after: i64, limit: i64,
) -> DomainResult<Vec<DeviceAlert>> {
    let size = usize::try_from(limit).unwrap_or(1);
    let mut cursor = after;
    let mut shown = Vec::new();
    loop {
        let page = alerts.after(cursor, limit).await?;
        let exhausted = page.len() < size;
        cursor = page.last().map_or(cursor, |alert| alert.seq);
        shown.extend(page.into_iter().filter(|alert| {
            preferences.allows(
                alert.kind,
                &alert.affected_providers(),
                alert.pipeline_id.as_deref(),
                alert.finish.as_ref(),
            )
        }));
        if exhausted || shown.len() >= size {
            shown.truncate(size);
            return Ok(shown);
        }
    }
}

impl CoreContext {
    pub async fn alert_page(
        &self, device_id: Option<&str>, after: Option<i64>, limit: Option<i64>,
    ) -> DomainResult<AlertPage> {
        let alerts = self.device_alerts();
        let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
        let after = after.unwrap_or(0);
        let Some(device_id) = device_id else {
            return read_page(alerts.latest_seq(), alerts.after(after, limit)).await;
        };
        let preferences = self
            .devices()
            .notification_preferences()
            .await?
            .remove(device_id)
            .unwrap_or_default();

        read_page(
            alerts.latest_seq(),
            shown_after(&alerts, &preferences, after, limit),
        )
        .await
    }
}

#[cfg(test)]
mod tests;
