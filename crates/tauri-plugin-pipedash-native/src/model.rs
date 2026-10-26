use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HapticKind {
    Selection,
    Light,
    Medium,
    Heavy,
    Soft,
    Rigid,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
    System,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Accessibility {
    pub reduce_transparency: bool,
    pub increase_contrast: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyCover {
    pub armed: bool,
    pub hold_for_frontend: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotificationResponse {
    pub action_id: String,
    pub key: Option<String>,
    pub pipeline_id: Option<String>,
    pub provider_id: Option<String>,
    pub run_number: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildWatchRunStatus {
    Pending,
    Running,
    Success,
    Failed,
    Cancelled,
    Skipped,
}

impl BuildWatchRunStatus {
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending | Self::Running)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildWatchMonitoring {
    Watching,
    Completed,
    Interrupted,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildWatchStopReason {
    Stopped,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildWatchSnapshot {
    pub session_id: String,
    pub pipeline_id: String,
    pub pipeline_name: String,
    pub provider_name: String,
    pub provider_type: String,
    pub run_number: i64,
    pub branch: Option<String>,
    pub status: BuildWatchRunStatus,
    pub started_at: i64,
    pub concluded_at: Option<i64>,
    pub updated_at: i64,
    pub monitoring: BuildWatchMonitoring,
    pub url: String,
}

impl BuildWatchSnapshot {
    pub fn run_url(pipeline_id: &str, run_number: i64) -> String {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut url = String::with_capacity(pipeline_id.len() + 64);
        url.push_str("pipedash://pipelines/");
        for byte in pipeline_id.bytes() {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                url.push(char::from(byte));
            } else {
                url.push('%');
                url.push(char::from(HEX[usize::from(byte >> 4)]));
                url.push(char::from(HEX[usize::from(byte & 15)]));
            }
        }
        url.push_str("/runs/");
        url.push_str(&run_number.to_string());
        url
    }

    pub fn validate(&self) -> crate::Result<()> {
        let valid_text = [
            (&self.session_id, 128),
            (&self.pipeline_id, 2048),
            (&self.pipeline_name, 512),
            (&self.provider_name, 512),
            (&self.provider_type, 128),
        ]
        .into_iter()
        .all(|(text, limit)| {
            !text.trim().is_empty() && text.len() <= limit && !text.chars().any(char::is_control)
        });
        let valid_branch = self
            .branch
            .as_ref()
            .is_none_or(|branch| branch.len() <= 1024 && !branch.chars().any(char::is_control));
        let valid_lifecycle = match self.monitoring {
            BuildWatchMonitoring::Watching => !self.status.is_terminal(),
            BuildWatchMonitoring::Completed => self.status.is_terminal(),
            BuildWatchMonitoring::Interrupted | BuildWatchMonitoring::Stopped => true,
        };
        if !valid_text
            || !valid_branch
            || !(1..=9_007_199_254_740_991).contains(&self.run_number)
            || self.started_at < 0
            || self.updated_at < 0
            || self.concluded_at.is_some_and(|end| end < self.started_at)
            || !valid_lifecycle
            || self.url.len() > 8192
            || self.url != Self::run_url(&self.pipeline_id, self.run_number)
        {
            return Err(crate::Error::Native(
                "Invalid build watch snapshot".to_string(),
            ));
        }
        if serde_json::to_vec(self)
            .map_err(|error| crate::Error::Native(error.to_string()))?
            .len()
            > 3500
        {
            return Err(crate::Error::Native(
                "The build details exceed the watch size limit".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildWatchStatus {
    pub supported: bool,
    pub available: bool,
    pub reason: Option<String>,
    pub watch: Option<BuildWatchSnapshot>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn snapshot() -> BuildWatchSnapshot {
        BuildWatchSnapshot {
            session_id: "session-1".to_string(),
            pipeline_id: "github__1__owner/repo/build.yml".to_string(),
            pipeline_name: "Build".to_string(),
            provider_name: "GitHub".to_string(),
            provider_type: "github".to_string(),
            run_number: 7,
            branch: None,
            status: BuildWatchRunStatus::Running,
            started_at: 100,
            concluded_at: None,
            updated_at: 110,
            monitoring: BuildWatchMonitoring::Watching,
            url: BuildWatchSnapshot::run_url("github__1__owner/repo/build.yml", 7),
        }
    }

    #[test]
    fn build_watch_serialization_preserves_nulls_and_camel_case() {
        let value = serde_json::to_value(snapshot()).unwrap();
        assert_eq!(value["sessionId"], "session-1");
        assert_eq!(value["runNumber"], 7);
        assert_eq!(value["monitoring"], "watching");
        assert_eq!(value["branch"], serde_json::Value::Null);
        assert_eq!(value["concludedAt"], serde_json::Value::Null);
        assert!(value.get("session_id").is_none());
    }

    #[test]
    fn run_links_encode_pipeline_ids_as_one_path_segment() {
        assert_eq!(
            BuildWatchSnapshot::run_url("github__1__é/a%?#", 7),
            "pipedash://pipelines/github__1__%C3%A9%2Fa%25%3F%23/runs/7"
        );
    }

    #[test]
    fn build_watch_rejects_invalid_identity_and_untrusted_urls() {
        let mut watch = snapshot();
        assert!(watch.validate().is_ok());
        watch.run_number = 9_007_199_254_740_992;
        assert!(watch.validate().is_err());
        watch = snapshot();
        watch.url = "https://example.com/token".to_string();
        assert!(watch.validate().is_err());
        watch = snapshot();
        watch.pipeline_id.clear();
        assert!(watch.validate().is_err());
    }

    #[test]
    fn build_watch_rejects_oversized_activity_content() {
        let mut watch = snapshot();
        watch.pipeline_id = "a".repeat(1600);
        watch.url = BuildWatchSnapshot::run_url(&watch.pipeline_id, watch.run_number);
        watch.branch = Some("b".repeat(1024));
        assert!(watch.validate().is_err());
    }

    #[test]
    fn completed_build_watch_requires_a_terminal_provider_status() {
        let mut watch = snapshot();
        watch.monitoring = BuildWatchMonitoring::Completed;
        assert!(watch.validate().is_err());
        watch.status = BuildWatchRunStatus::Success;
        watch.concluded_at = Some(120);
        assert!(watch.validate().is_ok());
        watch.monitoring = BuildWatchMonitoring::Watching;
        assert!(watch.validate().is_err());
        watch.monitoring = BuildWatchMonitoring::Interrupted;
        assert!(watch.validate().is_ok());
    }

    #[test]
    fn haptic_kinds_and_appearances_use_the_lowercase_names_the_frontend_sends() {
        assert_eq!(
            serde_json::from_value::<HapticKind>(json!("selection")).unwrap(),
            HapticKind::Selection
        );
        assert_eq!(
            serde_json::from_value::<HapticKind>(json!("rigid")).unwrap(),
            HapticKind::Rigid
        );
        assert!(serde_json::from_value::<HapticKind>(json!("buzz")).is_err());
        assert_eq!(
            serde_json::from_value::<Appearance>(json!("system")).unwrap(),
            Appearance::System
        );
        assert!(serde_json::from_value::<Appearance>(json!("auto")).is_err());
    }

    #[test]
    fn accessibility_flags_serialize_in_camel_case() {
        assert_eq!(
            serde_json::to_value(Accessibility {
                reduce_transparency: true,
                increase_contrast: false,
            })
            .unwrap(),
            json!({ "reduceTransparency": true, "increaseContrast": false })
        );
    }

    #[test]
    fn privacy_cover_arguments_match_the_swift_decoder() {
        assert_eq!(
            serde_json::to_value(PrivacyCover {
                armed: true,
                hold_for_frontend: false,
            })
            .unwrap(),
            json!({ "armed": true, "holdForFrontend": false })
        );
    }

    #[test]
    fn a_notification_response_tolerates_missing_fields() {
        let response: NotificationResponse = serde_json::from_value(json!({
            "actionId": "open-run",
            "pipelineId": "github__1__a/b/c.yml",
            "runNumber": "7",
        }))
        .unwrap();

        assert_eq!(response.action_id, "open-run");
        assert_eq!(
            response.pipeline_id.as_deref(),
            Some("github__1__a/b/c.yml")
        );
        assert_eq!(response.run_number.as_deref(), Some("7"));
        assert_eq!(response.key, None);
        assert_eq!(response.provider_id, None);
    }
}
