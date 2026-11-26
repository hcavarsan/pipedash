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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardPipeline {
    pub pipeline_id: String,
    pub name: String,
    pub repository: String,
    pub status: PipelineStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardTarget {
    pub target_key: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardEnvironment {
    pub group_key: String,
    pub label: String,
    pub targets: Vec<BoardTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardApplication {
    pub id: i64,
    pub name: String,
    pub editable: bool,
    pub notice: Option<String>,
    pub build: Vec<BoardPipeline>,
    pub deploy: Vec<BoardPipeline>,
    pub environments: Vec<BoardEnvironment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardCopy {
    pub title: String,
    pub help: String,
    pub search_placeholder: String,
    pub back: String,
    pub unassigned: String,
    pub unassigned_empty: String,
    pub unassigned_drop: String,
    pub applications: String,
    pub applications_empty: String,
    pub new_application_drop: String,
    pub open_application: String,
    pub build: String,
    pub deploy: String,
    pub lane_empty: String,
    pub environments: String,
    pub environments_empty: String,
    pub new_environment_drop: String,
    pub new_environment: String,
    pub environment_name: String,
    pub create_environment: String,
    pub cancel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationBoard {
    pub unassigned: Vec<BoardPipeline>,
    pub applications: Vec<BoardApplication>,
    pub copy: BoardCopy,
}
