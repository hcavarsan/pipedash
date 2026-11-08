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

pub fn kubeconfig_paths(value: &str) -> Vec<String> {
    let separator = if cfg!(windows) { ';' } else { ':' };

    value
        .split(separator)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(expand_path)
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
