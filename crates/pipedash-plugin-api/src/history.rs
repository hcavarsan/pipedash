use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunHistoryOrder {
    #[default]
    RunNumber,
    StartedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunHistoryPolicy {
    pub bounded: bool,
    pub order: RunHistoryOrder,
    pub tracks_overdue: bool,
}

impl Default for RunHistoryPolicy {
    fn default() -> Self {
        Self {
            bounded: false,
            order: RunHistoryOrder::RunNumber,
            tracks_overdue: true,
        }
    }
}
