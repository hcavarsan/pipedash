use serde::{
    Deserialize,
    Serialize,
};

use super::ids::Digest;
use super::ArtifactObservation;

const DEFAULT_REGISTRY: &str = "docker.io";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtifactRepository(String);

impl ArtifactRepository {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn host(&self) -> &str {
        self.0.split('/').next().unwrap_or_default()
    }

    pub fn path(&self) -> &str {
        self.0.split_once('/').map_or("", |(_, path)| path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImageRef {
    pub repository: ArtifactRepository,
    pub tag: Option<String>,
    pub digest: Option<Digest>,
}

impl ImageRef {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let s = s.split_once("://").map_or(s, |(_, rest)| rest);
        let (name, digest) = match s.split_once('@') {
            Some((name, digest)) => (name, Some(Digest::parse(digest)?)),
            None => (s, None),
        };
        let last_slash = name.rfind('/').map_or(0, |index| index + 1);
        let (name, tag) = match name[last_slash..].rfind(':') {
            Some(colon) => {
                let split = last_slash + colon;
                (&name[..split], Some(&name[split + 1..]))
            }
            None => (name, None),
        };
        let tag = tag.filter(|tag| !tag.is_empty()).map(str::to_string);
        if name.is_empty() || name.contains(char::is_whitespace) || name.ends_with('/') {
            return None;
        }
        let (host, path) = match name.split_once('/') {
            Some((first, rest)) if is_registry_host(first) => {
                (first.to_ascii_lowercase(), rest.to_string())
            }
            Some(_) => (DEFAULT_REGISTRY.to_string(), name.to_string()),
            None => (DEFAULT_REGISTRY.to_string(), format!("library/{name}")),
        };
        if path.is_empty() || path.split('/').any(str::is_empty) {
            return None;
        }
        Some(Self {
            repository: ArtifactRepository(format!("{host}/{path}")),
            tag,
            digest,
        })
    }

    pub fn into_observation(self) -> ArtifactObservation {
        ArtifactObservation {
            repository: self.repository,
            digest: self.digest,
            tag: self.tag,
        }
    }
}

fn is_registry_host(segment: &str) -> bool {
    segment.contains(['.', ':']) || segment == "localhost"
}

#[cfg(test)]
mod tests;
