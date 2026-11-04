use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    Failed,
    Degraded,
    LongRun,
    Outage,
    Recovered,
    Test,
}

impl AlertKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "failed",
            Self::Degraded => "degraded",
            Self::LongRun => "long_run",
            Self::Outage => "outage",
            Self::Recovered => "recovered",
            Self::Test => "test",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "failed" => Self::Failed,
            "degraded" => Self::Degraded,
            "long_run" => Self::LongRun,
            "outage" => Self::Outage,
            "recovered" => Self::Recovered,
            "test" => Self::Test,
            _ => return None,
        })
    }

    pub fn tone(self) -> &'static str {
        match self {
            Self::Failed | Self::Degraded => "danger",
            Self::LongRun | Self::Outage => "warning",
            Self::Recovered | Self::Test => "success",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationPreferences {
    pub failed: bool,
    pub degraded: bool,
    pub long_run: bool,
    pub outage: bool,
    pub recovered: bool,
    pub muted_providers: Vec<i64>,
    pub muted_pipelines: Vec<String>,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            failed: true,
            degraded: true,
            long_run: true,
            outage: true,
            recovered: true,
            muted_providers: Vec::new(),
            muted_pipelines: Vec::new(),
        }
    }
}

impl NotificationPreferences {
    pub fn allows(&self, kind: AlertKind, provider_ids: &[i64], pipeline_id: Option<&str>) -> bool {
        let wanted = match kind {
            AlertKind::Failed => self.failed,
            AlertKind::Degraded => self.degraded,
            AlertKind::LongRun => self.long_run,
            AlertKind::Outage => self.outage,
            AlertKind::Recovered => self.recovered,
            AlertKind::Test => return true,
        };

        let providers_muted = !provider_ids.is_empty()
            && provider_ids
                .iter()
                .all(|id| self.muted_providers.contains(id));

        wanted
            && !providers_muted
            && pipeline_id.is_none_or(|id| !self.muted_pipelines.iter().any(|muted| muted == id))
    }

    pub fn kinds_only(&self) -> Self {
        Self {
            muted_providers: Vec::new(),
            muted_pipelines: Vec::new(),
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceAlert {
    pub seq: i64,
    pub id: String,
    pub key: String,
    pub kind: AlertKind,
    pub title: String,
    pub body: String,
    pub pipeline_id: Option<String>,
    pub provider_id: Option<i64>,
    #[serde(default)]
    pub provider_ids: Vec<i64>,
    pub created_at: DateTime<Utc>,
}

impl DeviceAlert {
    pub fn affected_providers(&self) -> Vec<i64> {
        if self.provider_ids.is_empty() {
            self.provider_id.into_iter().collect()
        } else {
            self.provider_ids.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDeviceAlert {
    pub key: String,
    pub kind: AlertKind,
    pub title: String,
    pub body: String,
    pub pipeline_id: Option<String>,
    pub provider_id: Option<i64>,
    pub provider_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePush {
    pub device_id: String,
    pub relay_subscription_id: String,
    pub created_at: DateTime<Utc>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub undelivered_seq: Option<i64>,
    pub undelivered_max_seq: Option<i64>,
}

pub fn new_alert_id() -> String {
    format!("{:032x}", rand::random::<u128>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_round_trip_through_their_wire_names() {
        for kind in [
            AlertKind::Failed,
            AlertKind::Degraded,
            AlertKind::LongRun,
            AlertKind::Outage,
            AlertKind::Recovered,
            AlertKind::Test,
        ] {
            assert_eq!(AlertKind::parse(kind.as_str()), Some(kind));
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::json!(kind.as_str())
            );
        }
        assert_eq!(AlertKind::parse("nope"), None);
    }

    #[test]
    fn tones_group_kinds_by_severity() {
        assert_eq!(AlertKind::Failed.tone(), "danger");
        assert_eq!(AlertKind::Degraded.tone(), "danger");
        assert_eq!(AlertKind::LongRun.tone(), "warning");
        assert_eq!(AlertKind::Outage.tone(), "warning");
        assert_eq!(AlertKind::Recovered.tone(), "success");
        assert_eq!(AlertKind::Test.tone(), "success");
    }

    #[test]
    fn preferences_filter_by_kind_provider_and_pipeline() {
        let preferences = NotificationPreferences {
            long_run: false,
            muted_providers: vec![2],
            muted_pipelines: vec!["github__1__api".into()],
            ..NotificationPreferences::default()
        };

        assert!(preferences.allows(AlertKind::Failed, &[1], Some("github__1__web")));
        assert!(!preferences.allows(AlertKind::LongRun, &[1], Some("github__1__web")));
        assert!(!preferences.allows(AlertKind::Failed, &[1], Some("github__1__api")));
        assert!(!preferences.allows(AlertKind::Outage, &[2], None));
        assert!(preferences.allows(AlertKind::Outage, &[2, 3], None));
        assert!(preferences.allows(AlertKind::Outage, &[3], None));
        assert!(preferences.allows(AlertKind::Outage, &[], None));
        assert!(preferences.allows(AlertKind::Test, &[2], Some("github__1__api")));
        assert!(preferences
            .kinds_only()
            .allows(AlertKind::Failed, &[2], Some("github__1__api")));
    }

    #[test]
    fn missing_preference_fields_keep_their_defaults() {
        let preferences: NotificationPreferences =
            serde_json::from_value(serde_json::json!({ "outage": false, "future": true })).unwrap();

        assert_eq!(
            preferences,
            NotificationPreferences {
                outage: false,
                ..NotificationPreferences::default()
            }
        );
    }

    #[test]
    fn alert_ids_are_32_hex_characters() {
        let id = new_alert_id();
        assert_eq!(id.len(), 32);
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(id, new_alert_id());
    }
}
