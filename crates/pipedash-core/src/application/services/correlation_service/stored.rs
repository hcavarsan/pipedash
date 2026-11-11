use std::collections::{
    BTreeMap,
    BTreeSet,
};

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::vocab::commit_node_id;
use crate::domain::application::overrides::OverrideKind;
use crate::domain::application::{
    IdentityKind,
    Stage,
};
use crate::infrastructure::database::{
    ApplicationOverrideRow,
    ApplicationRow,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct Identity {
    pub kind: IdentityKind,
    pub key: String,
}

impl Identity {
    pub(super) fn new(kind: IdentityKind, key: impl Into<String>) -> Self {
        Self {
            kind,
            key: key.into(),
        }
    }

    pub(super) fn encode(&self) -> String {
        format!("{}|{}", self.kind.as_str(), self.key)
    }

    pub(super) fn decode(value: &str) -> Option<Self> {
        let (kind, key) = value.split_once('|')?;
        Some(Self::new(IdentityKind::parse(kind)?, key))
    }

    pub(super) fn of_row(row: &ApplicationRow) -> Option<Self> {
        Some(Self::new(
            IdentityKind::parse(&row.identity_kind)?,
            row.identity_key.clone(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct MemberSpec {
    pub pipeline_id: String,
    pub stage: Option<Stage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(super) enum Op {
    Link {
        application: String,
        pipeline_id: String,
        stage: Option<Stage>,
    },
    Unlink {
        application: String,
        pipeline_id: String,
    },
    Reject {
        suggestion_id: String,
    },
    Confirm {
        suggestion_id: String,
        digest: String,
        repo: String,
        commit: String,
    },
    Create {
        key: String,
        name: String,
        members: Vec<MemberSpec>,
    },
    Merge {
        into: String,
        from: String,
    },
    Split {
        key: String,
        name: String,
        pipeline_ids: Vec<String>,
    },
    Rename {
        application: String,
        name: String,
    },
    Hide {
        application: String,
        hidden: bool,
    },
    Stage {
        application: String,
        pipeline_id: String,
        stage: Stage,
    },
    TargetName {
        application: String,
        group_key: String,
        name: String,
    },
    TargetGroup {
        application: String,
        target_keys: Vec<String>,
        group_name: Option<String>,
    },
    Direction {
        application: String,
        from_group: String,
        to_group: String,
        present: bool,
    },
}

impl Op {
    pub(super) fn kind(&self) -> OverrideKind {
        match self {
            Op::Link { .. } | Op::Confirm { .. } => OverrideKind::Link,
            Op::Unlink { .. } | Op::Reject { .. } => OverrideKind::Unlink,
            Op::Create { .. } => OverrideKind::Create,
            Op::Merge { .. } => OverrideKind::Merge,
            Op::Split { .. } => OverrideKind::Split,
            Op::Rename { .. } => OverrideKind::Rename,
            Op::Hide { .. } => OverrideKind::Hide,
            Op::Stage { .. } => OverrideKind::Stage,
            Op::TargetName { .. } => OverrideKind::TargetName,
            Op::TargetGroup { .. } => OverrideKind::TargetGroup,
            Op::Direction { .. } => OverrideKind::Direction,
        }
    }

    pub(super) fn application(&self) -> Option<&str> {
        match self {
            Op::Link { application, .. }
            | Op::Unlink { application, .. }
            | Op::Rename { application, .. }
            | Op::Hide { application, .. }
            | Op::Stage { application, .. }
            | Op::TargetName { application, .. }
            | Op::TargetGroup { application, .. }
            | Op::Direction { application, .. } => Some(application),
            Op::Merge { into, .. } => Some(into),
            Op::Reject { .. } | Op::Confirm { .. } | Op::Create { .. } | Op::Split { .. } => None,
        }
    }

    pub(super) fn pipelines(&self) -> Vec<&str> {
        match self {
            Op::Link { pipeline_id, .. }
            | Op::Unlink { pipeline_id, .. }
            | Op::Stage { pipeline_id, .. } => vec![pipeline_id],
            Op::Create { members, .. } => members.iter().map(|m| m.pipeline_id.as_str()).collect(),
            Op::Split { pipeline_ids, .. } => pipeline_ids.iter().map(String::as_str).collect(),
            _ => Vec::new(),
        }
    }

    pub(super) fn owned_key(&self) -> Option<&str> {
        match self {
            Op::Create { key, .. } | Op::Split { key, .. } => Some(key),
            _ => None,
        }
    }

    fn subject(&self) -> Option<String> {
        match self {
            Op::Reject { suggestion_id } | Op::Confirm { suggestion_id, .. } => {
                Some(suggestion_id.clone())
            }
            Op::Merge { from, .. } => Some(from.clone()),
            other => other
                .pipelines()
                .first()
                .map(|p| (*p).to_string())
                .or_else(|| other.owned_key().map(str::to_string)),
        }
    }

    pub(super) fn to_row(
        &self, application_id: Option<i64>, author: Option<String>, now: DateTime<Utc>,
    ) -> ApplicationOverrideRow {
        ApplicationOverrideRow {
            id: 0,
            application_id,
            kind: self.kind().as_str().to_string(),
            subject: self.subject(),
            target: self.application().map(str::to_string),
            value_json: serde_json::to_string(self).unwrap_or_default(),
            author,
            created_at: now,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct StoredOverride {
    pub id: i64,
    pub application_id: Option<i64>,
    pub op: Op,
    pub author: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl StoredOverride {
    pub(super) fn parse(row: &ApplicationOverrideRow) -> Option<Self> {
        let op = serde_json::from_str(&row.value_json).ok()?;
        Some(Self {
            id: row.id,
            application_id: row.application_id,
            op,
            author: row.author.clone(),
            created_at: row.created_at,
        })
    }
}

pub(super) fn confirmed(overrides: &[StoredOverride]) -> BTreeMap<String, BTreeSet<String>> {
    let mut found: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for stored in overrides {
        if let Op::Confirm {
            digest,
            repo,
            commit,
            ..
        } = &stored.op
        {
            let node = commit_node_id(repo, commit);
            found.entry(digest.clone()).or_default().insert(node);
        }
    }
    found
}
