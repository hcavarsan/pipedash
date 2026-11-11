use std::collections::HashMap;

use crate::domain::{
    DeviceAlert,
    NotificationPreferences,
    OnceWatch,
};
use crate::infrastructure::config::PipedashConfig;

pub(super) fn remap_pipeline_id(
    pipeline_id: &str, provider_ids: &HashMap<i64, i64>,
) -> Option<String> {
    let mut parts = pipeline_id.splitn(3, "__");
    let (Some(provider_type), Some(provider_id), Some(rest)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let new_id = provider_ids.get(&provider_id.parse().ok()?)?;

    Some(format!("{provider_type}__{new_id}__{rest}"))
}

pub fn remap_preferences(
    mut preferences: NotificationPreferences, providers: &HashMap<i64, i64>,
) -> NotificationPreferences {
    preferences.muted_providers = preferences
        .muted_providers
        .iter()
        .filter_map(|id| providers.get(id).copied())
        .collect();
    let pipelines = |ids: &[String]| -> Vec<String> {
        ids.iter()
            .filter_map(|id| remap_pipeline_id(id, providers))
            .collect()
    };
    preferences.muted_pipelines = pipelines(&preferences.muted_pipelines);
    preferences.watched_pipelines = pipelines(&preferences.watched_pipelines);
    preferences.watch_once = preferences
        .watch_once
        .into_iter()
        .filter_map(|once| {
            Some(OnceWatch {
                pipeline_id: remap_pipeline_id(&once.pipeline_id, providers)?,
                ..once
            })
        })
        .collect();

    preferences
}

pub(super) fn remap_alert(
    mut alert: DeviceAlert, providers: &HashMap<i64, i64>,
) -> Option<DeviceAlert> {
    if let Some(provider_id) = alert.provider_id {
        alert.provider_id = Some(*providers.get(&provider_id)?);
    }
    alert.provider_ids = alert
        .provider_ids
        .iter()
        .filter_map(|id| providers.get(id).copied())
        .collect();
    if let Some(pipeline_id) = alert.pipeline_id.take() {
        let next = remap_pipeline_id(&pipeline_id, providers)?;
        if alert.key == format!("pipeline:{pipeline_id}") {
            alert.key = format!("pipeline:{next}");
        }
        alert.pipeline_id = Some(next);
    }
    for prefix in ["connection:", "instance:"] {
        if let Some(id) = alert.key.strip_prefix(prefix) {
            let next = providers.get(&id.parse().ok()?)?;
            alert.key = format!("{prefix}{next}");
            break;
        }
    }

    Some(alert)
}

pub(in crate::infrastructure) fn same_database(from: &PipedashConfig, to: &PipedashConfig) -> bool {
    from.storage.backend == to.storage.backend
        && if to.storage.backend.is_sqlite() {
            from.db_path() == to.db_path()
        } else {
            from.storage.postgres.connection_string == to.storage.postgres.connection_string
        }
}

#[cfg(test)]
mod tests;
