pub mod preferences;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::notification_center::CenterTone;

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
        CenterTone::from(self).as_str()
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
mod tests;
