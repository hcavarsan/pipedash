use std::collections::HashMap;

use base64::engine::general_purpose::{
    STANDARD,
    URL_SAFE,
};
use base64::Engine as _;
use bytes::Bytes;
use reqwest::{
    Response,
    StatusCode,
    Url,
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
};
use serde_json::Value;
use sha2::{
    Digest as _,
    Sha256,
};

use super::hooks::RegistryAuth;
use super::ids::{
    CommitSha,
    Digest,
    RepoId,
};
use super::image::ArtifactRepository;
use crate::error::{
    PluginError,
    PluginResult,
};
use crate::http::{
    api_url,
    check_response,
    map_send_error,
    read_prefix,
};

mod auth;
mod provenance;

use self::provenance::{
    provenance_v02,
    provenance_v1,
};

const MANIFEST_ACCEPT: &str = "application/vnd.oci.image.manifest.v1+json, \
     application/vnd.oci.image.index.v1+json, \
     application/vnd.docker.distribution.manifest.v2+json, \
     application/vnd.docker.distribution.manifest.list.v2+json";
const INDEX_ACCEPT: &str = "application/vnd.oci.image.index.v1+json";
const DIGEST_HEADER: &str = "docker-content-digest";
const MAX_MANIFEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_BLOB_BYTES: usize = 16 * 1024 * 1024;
const DOCKER_HUB_HOSTS: [&str; 2] = ["docker.io", "index.docker.io"];
const DOCKER_HUB_REGISTRY: &str = "registry-1.docker.io";
const SLSA_PROVENANCE: &str = "https://slsa.dev/provenance/";
const LABEL: &str = "Container registry";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Platform {
    pub os: String,
    pub architecture: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    pub media_type: String,
    #[serde(deserialize_with = "valid_digest")]
    pub digest: Digest,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_type: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub annotations: HashMap<String, String>,
    pub config: Descriptor,
    #[serde(default)]
    pub layers: Vec<Descriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlsaProvenance {
    pub repo: Option<RepoId>,
    pub commit: Option<CommitSha>,
    pub invocation: Option<String>,
}

#[derive(Deserialize)]
struct Index {
    manifests: Vec<Descriptor>,
}

#[derive(Deserialize)]
struct ImageConfig {
    #[serde(default)]
    config: Option<ContainerConfig>,
}

#[derive(Deserialize)]
struct ContainerConfig {
    #[serde(default, rename = "Labels")]
    labels: Option<HashMap<String, String>>,
}

fn valid_digest<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Digest, D::Error> {
    let raw = String::deserialize(deserializer)?;
    Digest::parse(&raw).ok_or_else(|| serde::de::Error::custom("unsupported digest"))
}

#[derive(Clone)]
pub struct OciClient {
    http: reqwest::Client,
}

impl OciClient {
    pub fn new(http: reqwest::Client) -> Self {
        Self { http }
    }

    pub async fn manifest(
        &self, repository: &ArtifactRepository, reference: &str, auth: Option<&RegistryAuth>,
    ) -> PluginResult<(Digest, Manifest)> {
        let (top, body, children) = self.top_level(repository, reference, auth).await?;
        let Some(children) = children else {
            return Ok((top, serde_json::from_slice(&body)?));
        };
        let entry = resolve_index(&children).ok_or_else(|| {
            PluginError::ApiError(format!("Empty image index for {}", repository.as_str()))
        })?;
        let (digest, body) = self
            .manifest_body(repository, entry.digest.as_str(), auth)
            .await?;
        Ok((digest, serde_json::from_slice(&body)?))
    }

    pub async fn tag_digests(
        &self, repository: &ArtifactRepository, reference: &str, auth: Option<&RegistryAuth>,
    ) -> PluginResult<Vec<Digest>> {
        let (top, _, children) = self.top_level(repository, reference, auth).await?;
        let mut digests = vec![top];
        for child in children.into_iter().flatten() {
            if !digests.contains(&child.digest) {
                digests.push(child.digest);
            }
        }
        Ok(digests)
    }

    pub async fn config_labels(
        &self, repository: &ArtifactRepository, manifest: &Manifest, auth: Option<&RegistryAuth>,
    ) -> PluginResult<HashMap<String, String>> {
        let blob = self.blob(repository, &manifest.config.digest, auth).await?;
        let config: ImageConfig = serde_json::from_slice(&blob)?;
        Ok(config
            .config
            .and_then(|config| config.labels)
            .unwrap_or_default())
    }

    pub async fn referrers(
        &self, repository: &ArtifactRepository, digest: &Digest, auth: Option<&RegistryAuth>,
    ) -> PluginResult<Vec<Descriptor>> {
        let url = registry_url(repository, "referrers", digest.as_str())?;
        let response = self.authed_get(&url, Some(INDEX_ACCEPT), auth).await?;
        if response.status() != StatusCode::NOT_FOUND {
            return read_index(check_response(response).await?, &url).await;
        }
        let tag = digest.as_str().replace(':', "-");
        let url = registry_url(repository, "manifests", &tag)?;
        let response = self.authed_get(&url, Some(INDEX_ACCEPT), auth).await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        read_index(check_response(response).await?, &url).await
    }

    pub async fn blob(
        &self, repository: &ArtifactRepository, digest: &Digest, auth: Option<&RegistryAuth>,
    ) -> PluginResult<Bytes> {
        let url = registry_url(repository, "blobs", digest.as_str())?;
        let response = check_response(self.authed_get(&url, None, auth).await?).await?;
        let body = read_limited(response, &url, MAX_BLOB_BYTES).await?;
        verified_digest(&body, None, Some(digest))?;
        Ok(body)
    }

    pub async fn ping(&self, host: &str, auth: Option<&RegistryAuth>) -> PluginResult<bool> {
        let url = api_url(&registry_base(host)?, "/v2/", &[], LABEL)?;
        let first = self.probe(&url, None, auth).await?;
        if first.status().is_success() {
            return Ok(true);
        }
        match self.answer(&url, &first, auth).await {
            Ok(Some(grant)) => Ok(self
                .request(&url, None, Some(&grant))
                .await?
                .status()
                .is_success()),
            Ok(None) | Err(PluginError::AuthenticationFailed(_)) => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn manifest_body(
        &self, repository: &ArtifactRepository, reference: &str, auth: Option<&RegistryAuth>,
    ) -> PluginResult<(Digest, Bytes)> {
        let url = registry_url(repository, "manifests", reference)?;
        let response =
            check_response(self.authed_get(&url, Some(MANIFEST_ACCEPT), auth).await?).await?;
        let advertised = response
            .headers()
            .get(DIGEST_HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(Digest::parse);
        let body = read_limited(response, &url, MAX_MANIFEST_BYTES).await?;
        let digest = verified_digest(
            &body,
            advertised.as_ref(),
            Digest::parse(reference).as_ref(),
        )?;
        Ok((digest, body))
    }

    async fn top_level(
        &self, repository: &ArtifactRepository, reference: &str, auth: Option<&RegistryAuth>,
    ) -> PluginResult<(Digest, Bytes, Option<Vec<Descriptor>>)> {
        let (digest, body) = self.manifest_body(repository, reference, auth).await?;
        let children = serde_json::from_slice::<Index>(&body)
            .ok()
            .map(|index| index.manifests);
        Ok((digest, body, children))
    }
}

pub fn dsse_provenance(bundle: &[u8]) -> Option<SlsaProvenance> {
    let document: Value = serde_json::from_slice(bundle).ok()?;
    let envelope = document.get("dsseEnvelope").unwrap_or(&document);
    let payload = envelope.get("payload")?.as_str()?.trim();
    let decoded = STANDARD
        .decode(payload)
        .or_else(|_| URL_SAFE.decode(payload))
        .ok()?;
    let statement: Value = serde_json::from_slice(&decoded).ok()?;
    if !statement
        .get("predicateType")?
        .as_str()?
        .starts_with(SLSA_PROVENANCE)
    {
        return None;
    }
    let predicate = statement.get("predicate")?;
    Some(if predicate.get("buildDefinition").is_some() {
        provenance_v1(predicate)
    } else {
        provenance_v02(predicate)
    })
}

fn resolve_index(entries: &[Descriptor]) -> Option<&Descriptor> {
    entries
        .iter()
        .find(|entry| {
            entry
                .platform
                .as_ref()
                .is_some_and(|platform| platform.os == "linux" && platform.architecture == "amd64")
        })
        .or_else(|| entries.first())
}

fn verified_digest(
    body: &[u8], advertised: Option<&Digest>, requested: Option<&Digest>,
) -> PluginResult<Digest> {
    let hex: String = Sha256::digest(body)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let actual = Digest::parse(&format!("sha256:{hex}"))
        .ok_or_else(|| PluginError::Internal("Unable to compute a sha256 digest".into()))?;
    for expected in [advertised, requested].into_iter().flatten() {
        if *expected != actual {
            return Err(PluginError::ApiError(format!(
                "Registry returned content with digest {} instead of {}",
                actual.as_str(),
                expected.as_str()
            )));
        }
    }
    Ok(actual)
}

async fn read_limited(response: Response, url: &Url, limit: usize) -> PluginResult<Bytes> {
    let body = read_prefix(response, limit + 1)
        .await
        .map_err(|error| map_send_error(error, url))?;
    if body.len() > limit {
        return Err(PluginError::ApiError(format!(
            "Registry response from {} exceeds {limit} bytes",
            url.host_str().unwrap_or_default()
        )));
    }
    Ok(Bytes::from(body))
}

async fn read_index(response: Response, url: &Url) -> PluginResult<Vec<Descriptor>> {
    let body = read_limited(response, url, MAX_MANIFEST_BYTES).await?;
    Ok(serde_json::from_slice::<Index>(&body)?.manifests)
}

fn is_loopback_host(host: &str) -> bool {
    let name = host
        .rsplit_once(':')
        .filter(|(_, port)| port.bytes().all(|byte| byte.is_ascii_digit()))
        .map_or(host, |(name, _)| name);
    matches!(name, "localhost" | "127.0.0.1" | "[::1]")
}

fn registry_base(host: &str) -> PluginResult<String> {
    let host = host.trim().to_ascii_lowercase();
    let valid = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':' | b'[' | b']'));
    if !valid {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid registry host: {host}"
        )));
    }
    let host = if DOCKER_HUB_HOSTS.contains(&host.as_str()) {
        DOCKER_HUB_REGISTRY.to_string()
    } else {
        host
    };
    let scheme = if is_loopback_host(&host) {
        "http"
    } else {
        "https"
    };
    Ok(format!("{scheme}://{host}"))
}

fn registry_url(repository: &ArtifactRepository, kind: &str, reference: &str) -> PluginResult<Url> {
    let name_ok = repository.path().split('/').all(|segment| {
        !matches!(segment, "" | "." | "..")
            && segment
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    });
    let reference_ok = !reference.is_empty()
        && reference
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':' | b'+'));
    if !name_ok || !reference_ok {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid registry reference {}@{reference}",
            repository.as_str()
        )));
    }
    api_url(
        &registry_base(repository.host())?,
        &format!("/v2/{}/{kind}/{reference}", repository.path()),
        &[],
        LABEL,
    )
}

#[cfg(test)]
mod tests;
