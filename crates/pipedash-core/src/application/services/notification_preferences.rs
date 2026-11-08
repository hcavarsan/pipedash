use chrono::Utc;

use crate::domain::{
    change_preferences,
    DomainError,
    DomainResult,
    NotificationPreferenceSnapshot,
    PreferenceChange,
};
use crate::infrastructure::database::DeviceRepository;
use crate::CoreContext;

pub struct NotificationPreferenceService {
    devices: DeviceRepository,
    device_id: String,
}

impl CoreContext {
    pub fn notification_preferences(&self, device_id: &str) -> NotificationPreferenceService {
        NotificationPreferenceService {
            devices: self.devices(),
            device_id: device_id.to_owned(),
        }
    }
}

fn device_not_found() -> DomainError {
    DomainError::NotFound("This device is no longer paired.".into())
}

impl NotificationPreferenceService {
    pub async fn change(
        &self, change: PreferenceChange,
    ) -> DomainResult<NotificationPreferenceSnapshot> {
        let now = Utc::now();
        let mut failure = None;
        let saved = self
            .devices
            .update_notification_preferences(&self.device_id, |stored| {
                match change_preferences(stored, change.clone(), now) {
                    Ok(()) => true,
                    Err(error) => {
                        failure = Some(error);
                        false
                    }
                }
            })
            .await?;
        if let Some(error) = failure {
            return Err(error);
        }

        saved
            .map(NotificationPreferenceSnapshot::new)
            .ok_or_else(device_not_found)
    }

    pub async fn snapshot(&self) -> DomainResult<NotificationPreferenceSnapshot> {
        let now = Utc::now();

        self.devices
            .update_notification_preferences(&self.device_id, |stored| {
                stored.drop_stale_watches(now)
            })
            .await?
            .map(NotificationPreferenceSnapshot::new)
            .ok_or_else(device_not_found)
    }
}

#[cfg(test)]
mod tests;
