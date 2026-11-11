use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Observed {
    Recorded,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunEvidence {
    pub outcome: Observed,
    pub timing: Observed,
    pub attempt: bool,
}

impl Default for RunEvidence {
    fn default() -> Self {
        Self {
            outcome: Observed::Recorded,
            timing: Observed::Recorded,
            attempt: true,
        }
    }
}

impl RunEvidence {
    pub fn success_only() -> Self {
        Self {
            outcome: Observed::Absent,
            timing: Observed::Absent,
            attempt: false,
        }
    }
}
