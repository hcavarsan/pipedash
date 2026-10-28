use std::collections::BTreeMap;

use chrono::{
    DateTime,
    Utc,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct ListResponse<T> {
    #[serde(default = "Vec::new")]
    pub value: Vec<T>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Project {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Identity {
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Link {
    #[serde(default)]
    pub href: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Links {
    #[serde(default)]
    pub web: Option<Link>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DefinitionRef {
    #[serde(default)]
    pub id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Build {
    pub id: i64,
    #[serde(default)]
    pub build_number: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub source_branch: Option<String>,
    #[serde(default)]
    pub source_version: Option<String>,
    #[serde(default)]
    pub queue_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub start_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finish_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub requested_for: Option<Identity>,
    #[serde(default)]
    pub parameters: Option<String>,
    #[serde(default)]
    pub definition: Option<DefinitionRef>,
    #[serde(default, rename = "_links")]
    pub links: Option<Links>,
}

impl Build {
    pub fn web_link(&self) -> Option<&str> {
        self.links
            .as_ref()
            .and_then(|links| links.web.as_ref())
            .and_then(|web| web.href.as_deref())
            .filter(|href| !href.is_empty())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Definition {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub latest_build: Option<Build>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Repository {
    #[serde(default)]
    pub default_branch: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Variable {
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub allow_override: bool,
    #[serde(default)]
    pub is_secret: bool,
}

impl Variable {
    pub fn is_overridable(&self) -> bool {
        self.allow_override && !self.is_secret
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct DefinitionDetail {
    #[serde(default)]
    pub repository: Option<Repository>,
    #[serde(default)]
    pub variables: Option<BTreeMap<String, Variable>>,
}
