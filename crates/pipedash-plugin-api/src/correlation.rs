use serde::{
    Deserialize,
    Serialize,
};

pub mod evidence;
pub mod hooks;
pub mod ids;
pub mod image;
pub mod log_scan;
pub mod oci;
pub mod path;

use self::ids::{
    ClusterId,
    CommitSha,
    Digest,
    RepoId,
    UnitId,
};
use self::image::ArtifactRepository;
use self::path::PathScope;

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Correlation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<UnitId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_parent: Option<UnitId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unit_depends_on: Vec<UnitId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_ref: Option<AppRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_label: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub produced: Vec<ArtifactObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumed: Vec<ConsumedArtifact>,
}

impl Correlation {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    pub fn single_source(repo: Option<RepoId>, commit: Option<CommitSha>) -> Self {
        Self {
            sources: repo
                .map(|repo| SourceObservation {
                    repo,
                    path_scopes: Vec::new(),
                    commit,
                })
                .into_iter()
                .collect(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceObservation {
    pub repo: RepoId,
    pub path_scopes: Vec<PathScope>,
    pub commit: Option<CommitSha>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TargetObservation {
    pub cluster: ClusterId,
    pub namespace: Option<String>,
    pub name: String,
    pub lifetime: TargetLifetime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetLifetime {
    Standing,
    ShortLived,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtifactObservation {
    pub repository: ArtifactRepository,
    pub digest: Option<Digest>,
    pub tag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConsumedArtifact {
    pub artifact: ArtifactObservation,
    pub target: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AppRef {
    pub authority: String,
    pub scope: String,
    pub id: String,
}
