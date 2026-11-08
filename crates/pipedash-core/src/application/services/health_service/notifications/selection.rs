use super::LocalNotification;
use crate::domain::NotificationPreferences;

pub fn wanted_notifications(
    preferences: &NotificationPreferences, notices: Vec<LocalNotification>,
) -> Vec<LocalNotification> {
    notices
        .into_iter()
        .filter(|notice| {
            preferences.allows(
                notice.kind,
                &notice.provider_ids,
                notice.pipeline_id.as_deref(),
                notice.finish.as_ref(),
            )
        })
        .collect()
}
