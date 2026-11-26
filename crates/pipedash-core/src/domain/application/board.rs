use serde::{
    Deserialize,
    Serialize,
};

use crate::domain::pipeline::PipelineStatus;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplicationBoardQuery {
    pub search: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardCardKind {
    Pipeline,
    Target,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardCardRef {
    pub kind: BoardCardKind,
    pub key: String,
    pub application_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardCard {
    pub item: BoardCardRef,
    pub zone_id: String,
    pub drop_class: String,
    pub draggable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoardZoneKind {
    Unassigned,
    NewApplication,
    Lane,
    Environment,
    NewEnvironment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardZoneRef {
    pub kind: BoardZoneKind,
    pub application_id: Option<i64>,
    pub key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardZone {
    pub id: String,
    pub target: BoardZoneRef,
    pub drop_class: String,
    pub enabled: bool,
    pub label: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardPipeline {
    pub card: BoardCard,
    pub name: String,
    pub repository: String,
    pub status: PipelineStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardTarget {
    pub card: BoardCard,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardLane {
    pub zone: BoardZone,
    pub title: String,
    pub pipelines: Vec<BoardPipeline>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardEnvironment {
    pub zone: BoardZone,
    pub label: String,
    pub targets: Vec<BoardTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardApplication {
    pub id: i64,
    pub name: String,
    pub notice: Option<String>,
    pub build: BoardLane,
    pub deploy: BoardLane,
    pub environments: Vec<BoardEnvironment>,
    pub new_environment: Option<BoardZone>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardCopy {
    pub title: String,
    pub help: String,
    pub search_placeholder: String,
    pub back: String,
    pub applications: String,
    pub applications_empty: String,
    pub open_application: String,
    pub environments: String,
    pub environments_empty: String,
    pub new_environment: String,
    pub environment_name: String,
    pub create_environment: String,
    pub cancel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationBoard {
    pub unassigned: BoardLane,
    pub new_application: BoardZone,
    pub applications: Vec<BoardApplication>,
    pub copy: BoardCopy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardDrop {
    pub card: BoardCardRef,
    pub zone: BoardZoneRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BoardDropOutcome {
    Applied,
    NameApplication {
        pipeline_id: String,
    },
    NameEnvironment {
        application_id: i64,
        target_key: String,
    },
}
