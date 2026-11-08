use std::collections::HashMap;

use super::{
    fan_out,
    PushService,
};
use crate::domain::{
    AlertKind,
    DeviceAlert,
    DevicePush,
    DomainError,
    DomainResult,
    NewDeviceAlert,
    NotificationPreferences,
};
use crate::infrastructure::push_relay::{
    send_alert,
    RelayOutcome,
};

fn relay_not_configured() -> DomainError {
    DomainError::Unprocessable {
        code: "PUSH_RELAY_NOT_CONFIGURED",
        message: "This host doesn't send push alerts.".into(),
    }
}

impl PushService {
    pub async fn deliver_alert(
        &self, push: &DevicePush, alert_id: &str, seq: Option<i64>,
    ) -> DomainResult<RelayOutcome> {
        let relay_url = self.relay_url().await.ok_or_else(relay_not_configured)?;

        Ok(self
            .deliver_to_device(&relay_url, push, alert_id, seq)
            .await)
    }

    async fn deliver_to_device(
        &self, relay_url: &str, push: &DevicePush, alert_id: &str, seq: Option<i64>,
    ) -> RelayOutcome {
        let outcome =
            send_alert(&self.http, relay_url, &push.relay_subscription_id, alert_id).await;
        let pushes = &self.device_pushes;
        let result = match outcome {
            RelayOutcome::Delivered => pushes.mark_success(&push.device_id).await,
            RelayOutcome::Gone => pushes
                .delete_matching(&push.device_id, &push.relay_subscription_id)
                .await
                .map(|_| ()),
            RelayOutcome::RateLimited | RelayOutcome::Failed => {
                tracing::debug!(?outcome, "The push relay did not accept an alert");
                match seq {
                    Some(seq) => {
                        pushes
                            .mark_undelivered(&push.device_id, &push.relay_subscription_id, seq)
                            .await
                    }
                    None => Ok(()),
                }
            }
        };
        if let Err(error) = result {
            tracing::warn!(%error, "Couldn't update a device push registration");
        }

        outcome
    }

    pub async fn push_recorded_alert(&self, alert_id: &str, seq: i64) -> DomainResult<()> {
        let Some(relay_url) = self.relay_url().await else {
            return Ok(());
        };
        let pushes = self.device_pushes.list().await?;
        let alert = self.device_alerts.get(alert_id).await.ok().flatten();
        let preferences = match self.devices.notification_preferences().await {
            Ok(preferences) => preferences,
            Err(error) => {
                tracing::warn!(%error, "Couldn't load device notification preferences");
                HashMap::new()
            }
        };
        let unset = NotificationPreferences::default();
        let wanted = |push: &DevicePush| {
            alert.as_ref().is_none_or(|alert: &DeviceAlert| {
                preferences.get(&push.device_id).unwrap_or(&unset).allows(
                    alert.kind,
                    &alert.affected_providers(),
                    alert.pipeline_id.as_deref(),
                    alert.finish.as_ref(),
                )
            })
        };

        fan_out(
            pushes
                .iter()
                .filter(|push| wanted(push))
                .map(|push| self.deliver_to_device(&relay_url, push, alert_id, Some(seq)))
                .collect(),
        )
        .await;

        Ok(())
    }

    pub async fn test_device_push(&self, device_id: &str) -> DomainResult<RelayOutcome> {
        let relay_url = self.relay_url().await.ok_or_else(relay_not_configured)?;
        let push =
            self.device_pushes
                .get(device_id)
                .await?
                .ok_or_else(|| DomainError::Conflict {
                    code: "PUSH_NOT_REGISTERED",
                    message: "Turn on notifications on this device first.".into(),
                })?;
        let alert = self
            .device_alerts
            .insert(&NewDeviceAlert {
                key: format!("test:{device_id}"),
                kind: AlertKind::Test,
                title: "Pipedash test alert".into(),
                body: "Push alerts from this host work.".into(),
                pipeline_id: None,
                provider_id: None,
                provider_ids: Vec::new(),
                finish: None,
            })
            .await?;

        Ok(self
            .deliver_to_device(&relay_url, &push, &alert.id, None)
            .await)
    }
}
