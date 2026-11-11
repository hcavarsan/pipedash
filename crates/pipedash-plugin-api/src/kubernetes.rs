use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::time::Duration;

use kube::config::{
    KubeConfigOptions,
    Kubeconfig,
};
use serde::de::DeserializeOwned;

use crate::error::{
    PluginError,
    PluginResult,
};
use crate::http::from_status;

const MAX_RETRY_AFTER_SECONDS: u32 = 3600;

pub const KUBECONFIG_CONTENT_KEY: &str = "token";
pub const NAMESPACE_LIST_REGEX: &str =
    r"^$|^[a-z0-9]([-a-z0-9]{0,61}[a-z0-9])?(\s*,\s*[a-z0-9]([-a-z0-9]{0,61}[a-z0-9])?)*$";
pub const NAMESPACE_LIST_MESSAGE: &str = "Use comma-separated Kubernetes namespace names: lowercase letters, numbers and hyphens, starting and ending with a letter or number, up to 63 characters each.";

fn is_dns_label(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub fn is_namespace(value: &str) -> bool {
    value.len() <= 63 && is_dns_label(value)
}

pub fn is_resource_name(value: &str) -> bool {
    value.len() <= 253 && value.split('.').all(is_dns_label)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectKey<'a> {
    pub kind: &'a str,
    pub namespace: &'a str,
    pub name: &'a str,
}

pub trait OwnedObject {
    fn namespace(&self) -> &str;
    fn name(&self) -> &str;
    fn owner_references(&self) -> impl Iterator<Item = (&str, &str)>;
}

impl<T: OwnedObject> OwnedObject for &T {
    fn namespace(&self) -> &str {
        (**self).namespace()
    }

    fn name(&self) -> &str {
        (**self).name()
    }

    fn owner_references(&self) -> impl Iterator<Item = (&str, &str)> {
        (**self).owner_references()
    }
}

pub struct Owners<'a, P> {
    pub managed: &'a [ObjectKey<'a>],
    pub parents: &'a [(&'a str, &'a [P])],
}

impl<P: OwnedObject> Owners<'_, P> {
    fn manages(&self, kind: &str, namespace: &str, name: &str) -> bool {
        self.managed
            .iter()
            .any(|key| key.kind == kind && key.namespace == namespace && key.name == name)
    }

    fn manages_through_parent(&self, kind: &str, namespace: &str, name: &str) -> bool {
        self.parents
            .iter()
            .filter(|entry| entry.0 == kind)
            .flat_map(|entry| entry.1.iter())
            .filter(|parent| parent.namespace() == namespace && parent.name() == name)
            .flat_map(|parent| parent.owner_references())
            .any(|(grand_kind, grand_name)| self.manages(grand_kind, namespace, grand_name))
    }
}

pub fn owned_by<O: OwnedObject, P: OwnedObject>(
    pods: impl IntoIterator<Item = O>, owners: &Owners<'_, P>,
) -> Vec<O> {
    pods.into_iter()
        .filter(|pod| {
            let namespace = pod.namespace();
            owners.manages("Pod", namespace, pod.name())
                || pod.owner_references().any(|(kind, name)| {
                    owners.manages(kind, namespace, name)
                        || owners.manages_through_parent(kind, namespace, name)
                })
        })
        .collect()
}

#[derive(Clone, PartialEq, Eq)]
pub enum KubeconfigSource {
    Content(String),
    Path(Vec<String>),
}

impl fmt::Debug for KubeconfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Content(_) => f.write_str("Content(<redacted>)"),
            Self::Path(paths) => f.debug_tuple("Path").field(paths).finish(),
        }
    }
}

impl KubeconfigSource {
    pub fn from_config(config: &HashMap<String, String>, path_key: &str) -> Self {
        if let Some(content) = config
            .get(KUBECONFIG_CONTENT_KEY)
            .filter(|content| !content.trim().is_empty())
        {
            return Self::Content(content.clone());
        }
        let configured = config
            .get(path_key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty());
        match configured {
            Some(paths) => Self::Path(kubeconfig_paths(paths)),
            None => Self::Path(kubeconfig_paths(&default_kubeconfig_path())),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Content(_) => "pasted kubeconfig content".to_string(),
            Self::Path(paths) => paths.join(", "),
        }
    }

    pub fn load(&self) -> PluginResult<Kubeconfig> {
        match self {
            Self::Content(content) => parse_kubeconfig_content(content),
            Self::Path(paths) => read_kubeconfigs(paths),
        }
    }
}

pub fn default_kubeconfig_path() -> String {
    if let Ok(kubeconfig) = std::env::var("KUBECONFIG") {
        if !kubeconfig.trim().is_empty() {
            return kubeconfig;
        }
    }

    dirs::home_dir()
        .map(|home| home.join(".kube/config").to_string_lossy().to_string())
        .unwrap_or_else(|| "~/.kube/config".to_string())
}

fn expand_path(path: &str) -> String {
    shellexpand::full(path)
        .map(Cow::into_owned)
        .unwrap_or_else(|_| path.to_string())
}

fn split_paths(value: &str) -> impl Iterator<Item = &str> {
    let separator = if cfg!(windows) { ';' } else { ':' };

    value
        .split(separator)
        .map(str::trim)
        .filter(|path| !path.is_empty())
}

pub fn kubeconfig_paths(value: &str) -> Vec<String> {
    split_paths(value)
        .flat_map(|path| {
            split_paths(&expand_path(path))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn parse_kubeconfig_content(content: &str) -> PluginResult<Kubeconfig> {
    Kubeconfig::from_yaml(content).map_err(|_| {
        PluginError::InvalidConfig(
            "The pasted kubeconfig content is not valid kubeconfig YAML".to_string(),
        )
    })
}

pub async fn available_contexts<F>(load: F, source: String) -> PluginResult<Vec<String>>
where
    F: FnOnce() -> PluginResult<Kubeconfig> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let mut contexts: Vec<String> = load()?
            .contexts
            .into_iter()
            .map(|context| context.name)
            .collect();
        if contexts.is_empty() {
            return Err(PluginError::InvalidConfig(format!(
                "No contexts found in kubeconfig: {source}"
            )));
        }
        contexts.sort();
        contexts.dedup();

        Ok(contexts)
    })
    .await
    .map_err(|error| PluginError::Internal(format!("Kubeconfig loader task failed: {error}")))?
}

pub fn read_kubeconfigs(paths: &[String]) -> PluginResult<Kubeconfig> {
    let mut merged: Option<Kubeconfig> = None;

    for path in paths.iter().filter(|path| Path::new(path).exists()) {
        let kubeconfig = Kubeconfig::read_from(path).map_err(|error| {
            PluginError::InvalidConfig(format!("Failed to read kubeconfig {path}: {error}"))
        })?;
        merged = Some(match merged {
            Some(existing) => existing.merge(kubeconfig).map_err(|error| {
                PluginError::InvalidConfig(format!("Failed to merge kubeconfig {path}: {error}"))
            })?,
            None => kubeconfig,
        });
    }

    merged.ok_or_else(|| {
        PluginError::InvalidConfig(format!("No kubeconfig file found in: {}", paths.join(", ")))
    })
}

pub fn ensure_supported_auth(
    kubeconfig: &Kubeconfig, context: Option<&str>, mobile: bool,
) -> PluginResult<()> {
    if !mobile {
        return Ok(());
    }

    let uses_exec = context
        .or(kubeconfig.current_context.as_deref())
        .and_then(|name| {
            kubeconfig
                .contexts
                .iter()
                .find(|candidate| candidate.name == name)
        })
        .and_then(|named| named.context.as_ref()?.user.as_deref())
        .and_then(|user| {
            kubeconfig
                .auth_infos
                .iter()
                .find(|candidate| candidate.name == user)
        })
        .filter(|named| {
            named
                .auth_info
                .as_ref()
                .is_some_and(|auth_info| auth_info.exec.is_some())
        });

    match uses_exec {
        Some(named) => Err(PluginError::NotSupported(format!(
            "Kubernetes user '{}' authenticates through an exec credential plugin, which is not supported on this platform. Use a token or client certificate in the kubeconfig instead.",
            named.name
        ))),
        None => Ok(()),
    }
}

pub fn map_error(error: kube::Error, context: &str) -> PluginError {
    match error {
        kube::Error::Api(status) => {
            let retry_after = status
                .details
                .as_ref()
                .map(|details| details.retry_after_seconds.min(MAX_RETRY_AFTER_SECONDS))
                .filter(|seconds| *seconds > 0)
                .map(|seconds| Duration::from_secs(seconds.into()));
            from_status(
                status.code,
                format!("{context}: {} (HTTP {})", status.message, status.code),
                retry_after,
            )
        }
        kube::Error::Auth(error) => {
            PluginError::AuthenticationFailed(format!("{context}: {error}"))
        }
        kube::Error::SerdeError(error) => {
            PluginError::SerializationError(format!("{context}: {error}"))
        }
        other => PluginError::NetworkError(format!("{context}: {other}")),
    }
}

pub async fn send<T: DeserializeOwned>(
    client: &kube::Client, request: Result<http::Request<Vec<u8>>, kube::core::request::Error>,
    context: &str,
) -> PluginResult<Result<T, kube::Error>> {
    let request =
        request.map_err(|error| PluginError::InvalidConfig(format!("{context}: {error}")))?;

    Ok(client.request(request).await)
}

pub async fn exec<T: DeserializeOwned>(
    client: &kube::Client, request: Result<http::Request<Vec<u8>>, kube::core::request::Error>,
    context: &str,
) -> PluginResult<T> {
    send(client, request, context)
        .await?
        .map_err(|error| map_error(error, context))
}

pub async fn connect<F>(
    load: F, context: Option<String>, reject_mobile_exec: bool,
) -> PluginResult<kube::Client>
where
    F: FnOnce() -> PluginResult<Kubeconfig> + Send + 'static,
{
    let runtime = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        let kubeconfig = load()?;
        if reject_mobile_exec {
            ensure_supported_auth(
                &kubeconfig,
                context.as_deref(),
                cfg!(any(target_os = "ios", target_os = "android")),
            )?;
        }
        let options = KubeConfigOptions {
            context,
            ..Default::default()
        };
        let mut config = runtime
            .block_on(kube::Config::from_custom_kubeconfig(kubeconfig, &options))
            .map_err(|error| {
                PluginError::InvalidConfig(format!("Failed to load kubeconfig: {error}"))
            })?;
        config.default_retry = false;

        kube::Client::try_from(config).map_err(|error| {
            PluginError::InvalidConfig(format!("Failed to create Kubernetes client: {error}"))
        })
    })
    .await
    .map_err(|error| PluginError::Internal(format!("Kubeconfig loader task failed: {error}")))?
}

#[cfg(test)]
mod tests;
