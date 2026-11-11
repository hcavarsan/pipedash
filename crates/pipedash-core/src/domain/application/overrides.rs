use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::Stage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideKind {
    Link,
    Unlink,
    Create,
    Merge,
    Split,
    Rename,
    Hide,
    Stage,
    TargetName,
    TargetGroup,
    Direction,
}

impl OverrideKind {
    const ALL: [Self; 11] = [
        Self::Link,
        Self::Unlink,
        Self::Create,
        Self::Merge,
        Self::Split,
        Self::Rename,
        Self::Hide,
        Self::Stage,
        Self::TargetName,
        Self::TargetGroup,
        Self::Direction,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Unlink => "unlink",
            Self::Create => "create",
            Self::Merge => "merge",
            Self::Split => "split",
            Self::Rename => "rename",
            Self::Hide => "hide",
            Self::Stage => "stage",
            Self::TargetName => "target_name",
            Self::TargetGroup => "target_group",
            Self::Direction => "direction",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverrideEntry {
    pub id: i64,
    pub kind: OverrideKind,
    pub description: String,
    pub author: Option<String>,
    pub created_at: DateTime<Utc>,
    pub shadowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApplicationEdit {
    Link {
        pipeline_id: String,
        #[serde(default)]
        stage: Option<Stage>,
    },
    Unlink {
        pipeline_id: String,
    },
    RejectSuggestion {
        suggestion_id: String,
    },
    AcceptSuggestion {
        suggestion_id: String,
    },
    Merge {
        other_application_id: i64,
    },
    Split {
        pipeline_ids: Vec<String>,
        name: String,
    },
    Rename {
        name: String,
    },
    Hide {
        hidden: bool,
    },
    Stage {
        pipeline_id: String,
        stage: Stage,
    },
    TargetName {
        group_key: String,
        name: String,
    },
    TargetGroup {
        target_keys: Vec<String>,
        #[serde(default)]
        group_name: Option<String>,
    },
    Direction {
        from_group: String,
        to_group: String,
        present: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateApplicationInput {
    pub name: String,
    pub members: Vec<CreateApplicationMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateApplicationMember {
    pub pipeline_id: String,
    #[serde(default)]
    pub stage: Option<Stage>,
}
