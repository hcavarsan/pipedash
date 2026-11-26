use serde::{
    Deserialize,
    Serialize,
};

const REMOTE_SCHEMES: [&str; 4] = ["https", "http", "ssh", "git"];
const DEFAULT_PORTS: [&str; 3] = ["80", "443", "22"];
const IN_CLUSTER_SERVER: &str = "kubernetes.default.svc";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RepoId(String);

impl RepoId {
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        let (authority, path) = match url.split_once("://") {
            Some((scheme, rest)) => {
                if !REMOTE_SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) {
                    return None;
                }
                rest.split_once('/')?
            }
            None => {
                let (authority, path) = url.split_once(':')?;
                if !authority.contains('@') || authority.contains('/') {
                    return None;
                }
                (authority, path)
            }
        };
        Self::from_host_path(&host_port(authority, &DEFAULT_PORTS)?, path)
    }

    pub fn from_host_path(host: &str, path: &str) -> Option<Self> {
        let host = host_port(host, &DEFAULT_PORTS)?;
        let path = path.split(['?', '#']).next().unwrap_or_default();
        let path = path.trim_end_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segments.is_empty() || segments.iter().any(|s| s.contains(char::is_whitespace)) {
            return None;
        }
        Some(Self(format!("{host}/{}", segments.join("/"))))
    }

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

fn host_port(authority: &str, default_ports: &[&str]) -> Option<String> {
    let authority = authority.trim().rsplit('@').next().unwrap_or_default();
    let authority = authority.to_ascii_lowercase();
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
            (host, Some(port))
        }
        Some((host, "")) => (host, None),
        _ => (authority.as_str(), None),
    };
    if host.is_empty() || host.contains(['/', ' ']) {
        return None;
    }
    match port {
        Some(port) if !default_ports.contains(&port) => Some(format!("{host}:{port}")),
        _ => Some(host.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommitSha(String);

impl CommitSha {
    pub fn parse(s: &str) -> Option<Self> {
        let sha = s.trim().to_ascii_lowercase();
        let valid = matches!(sha.len(), 40 | 64) && is_lower_hex(&sha);
        valid.then_some(Self(sha))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn short(&self) -> &str {
        &self.0[..7]
    }
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Digest(String);

impl Digest {
    pub fn parse(s: &str) -> Option<Self> {
        let digest = s.trim().to_ascii_lowercase();
        let hex = digest.strip_prefix("sha256:")?;
        let valid = hex.len() == 64 && is_lower_hex(hex);
        valid.then_some(Self(digest))
    }

    pub fn from_image_id(s: &str) -> Option<Self> {
        Self::parse(s.rsplit('@').next()?)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClusterId(String);

impl ClusterId {
    pub fn from_uid(uid: &str) -> Option<Self> {
        let uid = uid.trim().to_ascii_lowercase();
        (!uid.is_empty()).then(|| Self(format!("uid:{uid}")))
    }

    pub fn from_server(url: &str) -> Option<Self> {
        let (_, rest) = url.trim().split_once("://")?;
        let authority = rest.split(['/', '?', '#']).next()?;
        let host = host_port(authority, &["443"])?;
        (host != IN_CLUSTER_SERVER).then(|| Self(format!("server:{host}")))
    }

    pub fn from_host(host: &str) -> Option<Self> {
        let host = host.trim().to_ascii_lowercase();
        (!host.is_empty()).then(|| Self(format!("host:{host}")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnitId(String);

impl UnitId {
    pub fn new(local: &str) -> Option<Self> {
        let local = local.trim();
        (!local.is_empty()).then(|| Self(local.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests;
