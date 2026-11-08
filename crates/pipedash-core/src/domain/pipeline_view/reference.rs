use std::fmt;
use std::str::FromStr;

use serde::{
    Deserialize,
    Serialize,
};

use crate::domain::error::DomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ViewRef {
    #[default]
    All,
    Pinned,
    Custom(i64),
    Config(String),
}

impl fmt::Display for ViewRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViewRef::All => f.write_str("all"),
            ViewRef::Pinned => f.write_str("pinned"),
            ViewRef::Custom(id) => write!(f, "{id}"),
            ViewRef::Config(key) => write!(f, "config:{key}"),
        }
    }
}

impl FromStr for ViewRef {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let unknown = || DomainError::InvalidConfig(format!("Unknown pipeline view id: {s}"));
        match s {
            "all" => Ok(ViewRef::All),
            "pinned" => Ok(ViewRef::Pinned),
            _ => {
                if let Some(key) = s.strip_prefix("config:") {
                    if key.is_empty() {
                        return Err(unknown());
                    }
                    return Ok(ViewRef::Config(key.to_string()));
                }
                if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
                    return s.parse::<i64>().map(ViewRef::Custom).map_err(|_| unknown());
                }
                Err(unknown())
            }
        }
    }
}

impl TryFrom<String> for ViewRef {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<ViewRef> for String {
    fn from(value: ViewRef) -> Self {
        value.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    All,
    Pinned,
    Hidden,
    Custom,
    Config,
}

impl ViewKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ViewKind::All => "all",
            ViewKind::Pinned => "pinned",
            ViewKind::Hidden => "hidden",
            ViewKind::Custom => "custom",
            ViewKind::Config => "config",
        }
    }
}
