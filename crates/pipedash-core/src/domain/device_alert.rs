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
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDeviceAlert {
    pub key: String,
    pub kind: AlertKind,
    pub title: String,
    pub body: String,
    pub pipeline_id: Option<String>,
    pub provider_id: Option<i64>,
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
    fn alert_ids_are_32_hex_characters() {
        let id = new_alert_id();
        assert_eq!(id.len(), 32);
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(id, new_alert_id());
    }
}
