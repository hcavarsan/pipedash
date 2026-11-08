use serde::{
    Deserialize,
    Serialize,
};

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

    pub fn validate(&self) -> Result<(), String> {
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
            return Err("Invalid build watch snapshot".to_string());
        }
        let size = serde_json::to_vec(self)
            .map_err(|error| error.to_string())?
            .len();
        if size > 3500 {
            return Err("The build details exceed the watch size limit".to_string());
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
mod tests;
