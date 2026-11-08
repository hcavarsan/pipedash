use std::path::Path;

use toml_edit::{
    DocumentMut,
    Item,
    TableLike,
};

use super::{
    ConfigLoadError,
    ConfigLoadResult,
    ConfigLoader,
};
use crate::infrastructure::config::interpolation::{
    has_variables,
    interpolate,
};
use crate::infrastructure::config::schema::{
    GeneralConfig,
    PipedashConfig,
};

impl ConfigLoader {
    pub fn to_toml(config: &PipedashConfig) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(config)
    }

    pub fn save(config: &PipedashConfig, path: &Path) -> ConfigLoadResult<()> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            let mut doc = content.parse::<DocumentMut>().map_err(|e| {
                ConfigLoadError::InvalidConfig(format!("Failed to parse existing config: {}", e))
            })?;

            Self::update_document(&mut doc, config)?;

            std::fs::write(path, doc.to_string())?;
        } else {
            let toml_str = Self::to_toml(config).map_err(|e| {
                ConfigLoadError::InvalidConfig(format!("Failed to serialize config: {}", e))
            })?;

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            std::fs::write(path, toml_str)?;
        }

        Ok(())
    }

    pub(in crate::infrastructure::config) fn apply_general(
        doc: &mut DocumentMut, general: &GeneralConfig,
    ) {
        let item = doc
            .entry("general")
            .or_insert_with(|| Item::Table(toml_edit::Table::new()));
        let Some(table) = item.as_table_like_mut() else {
            return;
        };

        for (key, value) in [
            ("metrics_enabled", general.metrics_enabled.into()),
            (
                "default_refresh_interval",
                i64::from(general.default_refresh_interval).into(),
            ),
            ("status_pages_enabled", general.status_pages_enabled.into()),
        ] {
            match table.get_mut(key) {
                Some(existing) => *existing = Item::Value(value),
                None => {
                    table.insert(key, Item::Value(value));
                }
            }
        }
        Self::set_string(table, "proxy_url", &general.proxy_url);
    }

    fn set_string(table: &mut dyn TableLike, key: &str, value: &str) {
        let keeps_reference = table.get(key).and_then(Item::as_str).is_some_and(|raw| {
            has_variables(raw) && interpolate(raw).is_ok_and(|resolved| resolved == value)
        });

        if !keeps_reference {
            table.insert(key, Item::Value(value.into()));
        }
    }

    fn update_document(doc: &mut DocumentMut, config: &PipedashConfig) -> ConfigLoadResult<()> {
        Self::apply_general(doc, &config.general);

        if let Some(server) = doc.get_mut("server").and_then(|v| v.as_table_like_mut()) {
            Self::set_string(server, "bind_addr", &config.server.bind_addr);
            server.insert(
                "cors_allow_all",
                Item::Value(config.server.cors_allow_all.into()),
            );
        }

        if doc.get("storage").is_none() {
            doc.insert("storage", Item::Table(toml_edit::Table::new()));
        }

        if let Some(storage) = doc.get_mut("storage").and_then(|v| v.as_table_like_mut()) {
            storage.insert(
                "backend",
                Item::Value(config.storage.backend.to_string().into()),
            );

            if config.storage.data_dir.is_empty() {
                storage.remove("data_dir");
            } else {
                Self::set_string(storage, "data_dir", &config.storage.data_dir);
            }

            if !config.storage.postgres.connection_string.is_empty() {
                if storage.get("postgres").is_none() {
                    storage.insert("postgres", Item::Table(toml_edit::Table::new()));
                }
                if let Some(postgres) = storage
                    .get_mut("postgres")
                    .and_then(|v| v.as_table_like_mut())
                {
                    Self::set_string(
                        postgres,
                        "connection_string",
                        &config.storage.postgres.connection_string,
                    );
                }
            }
        }

        Ok(())
    }
}
