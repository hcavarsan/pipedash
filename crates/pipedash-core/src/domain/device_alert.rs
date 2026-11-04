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
    Finished,
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
            Self::Finished => "finished",
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
            "finished" => Self::Finished,
            "test" => Self::Test,
            _ => return None,
        })
    }

    pub fn tone(self) -> &'static str {
        match self {
            Self::Failed | Self::Degraded => "danger",
            Self::LongRun | Self::Outage => "warning",
            Self::Recovered | Self::Finished | Self::Test => "success",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFinish {
    pub at: DateTime<Utc>,
    pub previous: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnceWatch {
    pub pipeline_id: String,
    pub since: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fired_at: Option<DateTime<Utc>>,
}

impl OnceWatch {
    fn matches(&self, pipeline_id: &str, finish: &RunFinish) -> bool {
        self.pipeline_id == pipeline_id
            && match self.fired_at {
                Some(fired_at) => finish.at == fired_at,
                None => {
                    finish.at >= self.since
                        && finish.previous.is_none_or(|previous| previous < self.since)
                }
            }
    }

    fn same_request(&self, other: &Self) -> bool {
        self.pipeline_id == other.pipeline_id && self.since == other.since
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
    pub watched_pipelines: Vec<String>,
    pub watch_once: Vec<OnceWatch>,
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
            watched_pipelines: Vec::new(),
            watch_once: Vec::new(),
        }
    }
}

impl NotificationPreferences {
    pub fn allows(
        &self, kind: AlertKind, provider_ids: &[i64], pipeline_id: Option<&str>,
        finish: Option<&RunFinish>,
    ) -> bool {
        if let Some(id) = pipeline_id {
            if kind != AlertKind::Test && self.muted_pipelines.iter().any(|muted| muted == id) {
                return false;
            }
            if self.watched_pipelines.iter().any(|watched| watched == id)
                || finish.is_some_and(|finish| {
                    self.watch_once.iter().any(|once| once.matches(id, finish))
                })
            {
                return true;
            }
        }
        let wanted = match kind {
            AlertKind::Failed => self.failed,
            AlertKind::Degraded => self.degraded,
            AlertKind::LongRun => self.long_run,
            AlertKind::Outage => self.outage,
            AlertKind::Recovered => self.recovered,
            AlertKind::Finished => false,
            AlertKind::Test => return true,
        };

        let providers_muted = !provider_ids.is_empty()
            && provider_ids
                .iter()
                .all(|id| self.muted_providers.contains(id));

        wanted && !providers_muted
    }

    pub fn fire_once(&mut self, pipeline_id: &str, finish: &RunFinish) -> Vec<OnceWatch> {
        self.watch_once
            .iter_mut()
            .filter(|once| once.fired_at.is_none() && once.matches(pipeline_id, finish))
            .map(|once| {
                once.fired_at = Some(finish.at);
                once.clone()
            })
            .collect()
    }

    pub fn keep_fired(&mut self, stored: &Self) {
        for once in &mut self.watch_once {
            if let Some(fired) = stored
                .watch_once
                .iter()
                .find(|saved| saved.same_request(once))
            {
                once.fired_at = once.fired_at.or(fired.fired_at);
            }
        }
    }

    pub fn fired_watches(&self) -> Vec<OnceWatch> {
        self.watch_once
            .iter()
            .filter(|once| once.fired_at.is_some())
            .cloned()
            .collect()
    }

    pub fn kinds_only(&self) -> Self {
        Self {
            muted_providers: Vec::new(),
            muted_pipelines: Vec::new(),
            watched_pipelines: Vec::new(),
            watch_once: Vec::new(),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<RunFinish>,
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
    pub finish: Option<RunFinish>,
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
            AlertKind::Finished,
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

        assert!(preferences.allows(AlertKind::Failed, &[1], Some("github__1__web"), None));
        assert!(!preferences.allows(AlertKind::LongRun, &[1], Some("github__1__web"), None));
        assert!(!preferences.allows(AlertKind::Failed, &[1], Some("github__1__api"), None));
        assert!(!preferences.allows(AlertKind::Outage, &[2], None, None));
        assert!(preferences.allows(AlertKind::Outage, &[2, 3], None, None));
        assert!(preferences.allows(AlertKind::Outage, &[3], None, None));
        assert!(preferences.allows(AlertKind::Outage, &[], None, None));
        assert!(preferences.allows(AlertKind::Test, &[2], Some("github__1__api"), None));
        assert!(preferences.kinds_only().allows(
            AlertKind::Failed,
            &[2],
            Some("github__1__api"),
            None
        ));
    }

    #[test]
    fn a_watched_pipeline_beats_provider_mutes_and_switches_but_not_its_own_mute() {
        let preferences = NotificationPreferences {
            failed: false,
            long_run: false,
            muted_providers: vec![2],
            muted_pipelines: vec!["github__2__muted".into()],
            watched_pipelines: vec!["github__2__api".into(), "github__2__muted".into()],
            ..NotificationPreferences::default()
        };
        let watched = Some("github__2__api");

        for kind in [AlertKind::Failed, AlertKind::LongRun, AlertKind::Finished] {
            assert!(preferences.allows(kind, &[2], watched, None), "{kind:?}");
        }
        assert!(!preferences.allows(AlertKind::Finished, &[1], Some("github__1__web"), None));
        assert!(!preferences.allows(AlertKind::Finished, &[], None, None));
        assert!(!preferences.allows(AlertKind::Finished, &[2], Some("github__2__muted"), None));
        assert!(!preferences
            .kinds_only()
            .allows(AlertKind::Finished, &[2], watched, None));
    }

    #[test]
    fn a_one_time_watch_lets_only_the_first_finish_after_it_through() {
        let since = DateTime::parse_from_rfc3339("2026-11-04T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let minutes = |m: i64| since + chrono::Duration::minutes(m);
        let preferences = NotificationPreferences {
            failed: false,
            muted_providers: vec![1],
            watch_once: vec![OnceWatch {
                pipeline_id: "github__1__deploy".into(),
                since,
                fired_at: None,
            }],
            ..NotificationPreferences::default()
        };
        let allows = |kind, finish: RunFinish| {
            preferences.allows(kind, &[1], Some("github__1__deploy"), Some(&finish))
        };

        assert!(allows(
            AlertKind::Finished,
            RunFinish {
                at: minutes(5),
                previous: Some(minutes(-30)),
            }
        ));
        assert!(allows(
            AlertKind::Failed,
            RunFinish {
                at: minutes(5),
                previous: None,
            }
        ));
        assert!(!allows(
            AlertKind::Finished,
            RunFinish {
                at: minutes(9),
                previous: Some(minutes(5)),
            }
        ));
        assert!(!allows(
            AlertKind::Finished,
            RunFinish {
                at: minutes(-1),
                previous: None,
            }
        ));
        assert!(!preferences.allows(AlertKind::Finished, &[1], Some("github__1__deploy"), None));
        assert!(!preferences.allows(
            AlertKind::Finished,
            &[1],
            Some("github__1__other"),
            Some(&RunFinish {
                at: minutes(5),
                previous: None,
            })
        ));
    }

    #[test]
    fn a_used_one_time_watch_only_lets_its_own_finish_through_even_after_a_restart() {
        let since = DateTime::parse_from_rfc3339("2026-11-04T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let minutes = |m: i64| since + chrono::Duration::minutes(m);
        let request = OnceWatch {
            pipeline_id: "github__1__deploy".into(),
            since,
            fired_at: None,
        };
        let mut stored = NotificationPreferences {
            watch_once: vec![request.clone()],
            ..NotificationPreferences::default()
        };
        let first = RunFinish {
            at: minutes(5),
            previous: None,
        };
        let after_restart = RunFinish {
            at: minutes(40),
            previous: None,
        };

        assert_eq!(stored.fire_once("github__1__deploy", &first).len(), 1);
        assert!(stored
            .fire_once("github__1__deploy", &after_restart)
            .is_empty());

        let mut resent = NotificationPreferences {
            watch_once: vec![
                request,
                OnceWatch {
                    pipeline_id: "github__1__deploy".into(),
                    since: minutes(50),
                    fired_at: None,
                },
            ],
            ..NotificationPreferences::default()
        };
        resent.keep_fired(&stored);
        let allows = |finish: &RunFinish| {
            resent.watch_once[..1]
                .iter()
                .any(|once| once.matches("github__1__deploy", finish))
        };

        assert!(allows(&first));
        assert!(!allows(&after_restart));
        assert_eq!(resent.watch_once[1].fired_at, None);
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
