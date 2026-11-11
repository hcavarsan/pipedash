use serde::{
    Deserialize,
    Serialize,
};

pub mod dto;
pub mod metrics;
pub mod overrides;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityKind {
    Unit,
    AppRef,
    User,
    Config,
}

impl IdentityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::AppRef => "app_ref",
            Self::User => "user",
            Self::Config => "config",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Unit, Self::AppRef, Self::User, Self::Config]
            .into_iter()
            .find(|kind| kind.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Build,
    Deploy,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Deploy => "deploy",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Build, Self::Deploy]
            .into_iter()
            .find(|stage| stage.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberSource {
    Exact,
    Override,
    Config,
}

impl MemberSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Override => "override",
            Self::Config => "config",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Exact, Self::Override, Self::Config]
            .into_iter()
            .find(|source| source.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Exact,
    Suggested,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Suggested => "suggested",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationCapabilities {
    pub edit: bool,
    pub delete: bool,
}

impl ApplicationCapabilities {
    pub fn for_identity(kind: IdentityKind) -> Self {
        Self {
            edit: kind != IdentityKind::Config,
            delete: kind == IdentityKind::User,
        }
    }
}
