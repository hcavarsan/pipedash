use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;

const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_DATABASE_PATH: &str = "pipedash-push-relay.db";

pub struct Config {
    pub bind_addr: SocketAddr,
    pub database_path: PathBuf,
    pub apns_key: String,
    pub apns_key_id: String,
    pub apns_team_id: String,
    pub allowed_bundle_ids: HashSet<String>,
    pub trusted_proxy: bool,
}

fn required(var: &impl Fn(&str) -> Option<String>, name: &str) -> anyhow::Result<String> {
    var(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{name} is required"))
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    fn from_vars(var: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let bind_addr = var("RELAY_BIND_ADDR")
            .unwrap_or_else(|| DEFAULT_BIND_ADDR.to_owned())
            .parse()
            .context("Invalid RELAY_BIND_ADDR")?;
        let database_path = var("RELAY_DATABASE_PATH")
            .unwrap_or_else(|| DEFAULT_DATABASE_PATH.to_owned())
            .into();

        let apns_key = match var("RELAY_APNS_KEY").filter(|key| !key.trim().is_empty()) {
            Some(key) => key,
            None => {
                let path = required(&var, "RELAY_APNS_KEY_PATH")
                    .context("Set RELAY_APNS_KEY or RELAY_APNS_KEY_PATH")?;
                std::fs::read_to_string(&path).context("Couldn't read RELAY_APNS_KEY_PATH")?
            }
        };

        let allowed_bundle_ids: HashSet<String> = required(&var, "RELAY_ALLOWED_BUNDLE_IDS")?
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect();
        if allowed_bundle_ids.is_empty() {
            anyhow::bail!("RELAY_ALLOWED_BUNDLE_IDS lists no bundle ids");
        }

        Ok(Self {
            bind_addr,
            database_path,
            apns_key,
            apns_key_id: required(&var, "RELAY_APNS_KEY_ID")?,
            apns_team_id: required(&var, "RELAY_APNS_TEAM_ID")?,
            allowed_bundle_ids,
            trusted_proxy: var("RELAY_TRUSTED_PROXY")
                .is_some_and(|value| matches!(value.trim(), "true" | "1")),
        })
    }
}

#[cfg(test)]
mod tests;
