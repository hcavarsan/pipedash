pub mod alerts;

use std::collections::HashSet;

use chrono::{
    DateTime,
    Utc,
};
use serde::Serialize;

use super::push_service::subscriptions::is_relay_subscription_id;
use crate::domain::{
    DomainError,
    DomainResult,
    NotificationPreferences,
    OnceWatch,
};
use crate::CoreContext;

#[derive(Debug, Clone, Serialize)]
pub struct DeviceView {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub current: bool,
    pub removable: bool,
    pub push: bool,
}

impl CoreContext {
    pub async fn list_devices(
        &self, current: Option<&str>, removable: bool,
    ) -> DomainResult<Vec<DeviceView>> {
        let devices = self.devices().list().await?;
        let pushes: HashSet<String> = self
            .device_pushes()
            .list()
            .await?
            .into_iter()
            .map(|push| push.device_id)
            .collect();

        Ok(devices
            .into_iter()
            .map(|device| {
                let is_current = current == Some(device.id.as_str());
                DeviceView {
                    removable: removable && !is_current,
                    current: is_current,
                    push: pushes.contains(&device.id),
                    id: device.id,
                    name: device.name,
                    platform: device.platform,
                    created_at: device.created_at,
                    last_seen_at: device.last_seen_at,
                }
            })
            .collect())
    }

    pub async fn set_device_notification_preferences(
        &self, device_id: &str, incoming: NotificationPreferences,
    ) -> DomainResult<Vec<OnceWatch>> {
        let saved = self
            .devices()
            .update_notification_preferences(device_id, |stored| {
                stored.merge(&incoming);
                true
            })
            .await?
            .unwrap_or_default();

        Ok(saved.fired_watches())
    }

    pub async fn set_device_push(
        &self, device_id: &str, subscription_id: &str, relay_available: bool,
    ) -> DomainResult<()> {
        if !relay_available {
            return Err(DomainError::Unprocessable {
                code: "PUSH_RELAY_NOT_CONFIGURED",
                message: "This host doesn't send push alerts.".into(),
            });
        }
        if !is_relay_subscription_id(subscription_id) {
            return Err(DomainError::Unprocessable {
                code: "INVALID_SUBSCRIPTION",
                message: "This device's push registration isn't valid.".into(),
            });
        }

        self.device_pushes().set(device_id, subscription_id).await
    }
}
