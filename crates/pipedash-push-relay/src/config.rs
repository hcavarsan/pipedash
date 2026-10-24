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
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();

        move |name| map.get(name).cloned()
    }

    const REQUIRED: [(&str, &str); 4] = [
        ("RELAY_APNS_KEY", "pem"),
        ("RELAY_APNS_KEY_ID", "KEYID"),
        ("RELAY_APNS_TEAM_ID", "TEAMID"),
        ("RELAY_ALLOWED_BUNDLE_IDS", "com.a.app, com.b.app,,"),
    ];

    #[test]
    fn defaults_apply_when_only_the_required_variables_are_set() {
        let config = Config::from_vars(vars(&REQUIRED)).unwrap();

        assert_eq!(config.bind_addr, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(config.database_path, PathBuf::from(DEFAULT_DATABASE_PATH));
        assert_eq!(config.apns_key, "pem");
        assert_eq!(config.apns_key_id, "KEYID");
        assert_eq!(config.apns_team_id, "TEAMID");
        assert_eq!(
            config.allowed_bundle_ids,
            HashSet::from(["com.a.app".to_string(), "com.b.app".to_string()])
        );
        assert!(!config.trusted_proxy);
    }

    #[test]
    fn the_key_can_come_from_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("AuthKey.p8");
        std::fs::write(&path, "from-file").unwrap();
        let path = path.to_str().unwrap();
        let mut pairs: Vec<(&str, &str)> = REQUIRED[1..].to_vec();
        pairs.push(("RELAY_APNS_KEY_PATH", path));
        pairs.push(("RELAY_BIND_ADDR", "127.0.0.1:9000"));
        pairs.push(("RELAY_DATABASE_PATH", "/data/relay.db"));
        pairs.push(("RELAY_TRUSTED_PROXY", "true"));

        let config = Config::from_vars(vars(&pairs)).unwrap();

        assert_eq!(config.apns_key, "from-file");
        assert_eq!(config.bind_addr, "127.0.0.1:9000".parse().unwrap());
        assert_eq!(config.database_path, PathBuf::from("/data/relay.db"));
        assert!(config.trusted_proxy);
    }

    #[test]
    fn missing_or_empty_settings_are_reported_by_name() {
        for missing in ["RELAY_APNS_KEY", "RELAY_APNS_KEY_ID", "RELAY_APNS_TEAM_ID"] {
            let pairs: Vec<_> = REQUIRED
                .iter()
                .copied()
                .filter(|(name, _)| *name != missing)
                .collect();
            let error = Config::from_vars(vars(&pairs)).err().unwrap();

            assert!(
                format!("{error:#}").contains(missing),
                "{missing}: {error:#}"
            );
        }

        let mut pairs = REQUIRED.to_vec();
        pairs[3] = ("RELAY_ALLOWED_BUNDLE_IDS", " , ");
        let error = Config::from_vars(vars(&pairs)).err().unwrap();
        assert!(format!("{error:#}").contains("RELAY_ALLOWED_BUNDLE_IDS"));

        pairs[3] = ("RELAY_ALLOWED_BUNDLE_IDS", "com.a.app");
        pairs.push(("RELAY_BIND_ADDR", "nope"));
        let error = Config::from_vars(vars(&pairs)).err().unwrap();
        assert!(format!("{error:#}").contains("RELAY_BIND_ADDR"));
    }
}
