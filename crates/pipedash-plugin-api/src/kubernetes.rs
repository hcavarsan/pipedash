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

#[derive(Clone, PartialEq, Eq)]
pub enum KubeconfigSource<P> {
    Content(String),
    Path(P),
}

impl<P: fmt::Debug> fmt::Debug for KubeconfigSource<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Content(_) => f.write_str("Content(<redacted>)"),
            Self::Path(path) => f.debug_tuple("Path").field(path).finish(),
        }
    }
}

impl<P> KubeconfigSource<P> {
    pub fn from_config(
        config: &HashMap<String, String>, content_key: &str, path: impl FnOnce() -> P,
    ) -> Self {
        match config
            .get(content_key)
            .filter(|content| !content.trim().is_empty())
        {
            Some(content) => Self::Content(content.clone()),
            None => Self::Path(path()),
        }
    }
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
