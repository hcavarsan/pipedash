use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::{
    null_as_empty,
    HealthStatus,
};

#[derive(Debug, Deserialize)]
pub struct ApplicationTree {
    #[serde(default, deserialize_with = "null_as_empty")]
    pub nodes: Vec<ResourceNode>,
}

#[derive(Debug, Deserialize)]
pub struct ResourceNode {
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub namespace: String,
    #[serde(default)]
    pub name: String,
    pub health: Option<HealthStatus>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub info: Vec<InfoItem>,
}

#[derive(Debug, Deserialize)]
pub struct InfoItem {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct EventList {
    #[serde(default, deserialize_with = "null_as_empty")]
    pub items: Vec<Event>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    #[serde(rename = "type", default)]
    pub event_type: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub count: i64,
    pub first_timestamp: Option<DateTime<Utc>>,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub event_time: Option<DateTime<Utc>>,
}

impl Event {
    pub fn time(&self) -> Option<DateTime<Utc>> {
        self.last_timestamp
            .or(self.event_time)
            .or(self.first_timestamp)
    }
}

#[derive(Debug, Deserialize)]
pub struct PodLogMessage {
    pub result: Option<PodLogEntry>,
    pub error: Option<StreamError>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodLogEntry {
    #[serde(default)]
    pub content: String,
    pub time_stamp_str: Option<String>,
    #[serde(default)]
    pub last: bool,
}

#[derive(Debug, Deserialize)]
pub struct StreamError {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub http_code: u16,
}

#[derive(Debug, Deserialize)]
pub struct ResourceManifest {
    pub manifest: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Pod {
    pub metadata: PodMetadata,
    pub spec: PodSpec,
    pub status: PodStatus,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PodMetadata {
    pub name: String,
    pub namespace: String,
    pub uid: String,
    pub owner_references: Vec<OwnerReference>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OwnerReference {
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PodSpec {
    pub node_name: Option<String>,
    pub init_containers: Vec<PodContainer>,
    pub containers: Vec<PodContainer>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PodContainer {
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PodStatus {
    pub phase: String,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub init_container_statuses: Vec<ContainerStatus>,
    pub container_statuses: Vec<ContainerStatus>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContainerStatus {
    pub name: String,
    pub restart_count: i64,
    pub ready: bool,
    pub state: ContainerState,
    pub last_state: ContainerState,
    pub image: Option<String>,
    #[serde(rename = "imageID")]
    pub image_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ContainerState {
    pub waiting: Option<ContainerWaiting>,
    pub terminated: Option<ContainerTerminated>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ContainerWaiting {
    pub reason: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContainerTerminated {
    pub exit_code: i64,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub finished_at: Option<DateTime<Utc>>,
}
