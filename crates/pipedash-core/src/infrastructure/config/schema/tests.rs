use super::*;

#[test]
fn test_default_config() {
    let config = PipedashConfig::default();

    assert!(config.general.metrics_enabled);
    assert_eq!(config.general.default_refresh_interval, 30);
    assert_eq!(config.server.bind_addr, "127.0.0.1:8080");
    assert_eq!(config.storage.backend, StorageBackend::Sqlite);
    assert!(config.providers.is_empty());
    assert!(!config.server.cors_allow_all);
    assert!(config.general.status_pages_enabled);
}

#[test]
fn test_status_pages_default_to_enabled_when_absent() {
    let config: PipedashConfig = toml::from_str("[general]\nmetrics_enabled = false").unwrap();
    let disabled: PipedashConfig =
        toml::from_str("[general]\nstatus_pages_enabled = false").unwrap();

    assert!(config.general.status_pages_enabled);
    assert!(!disabled.general.status_pages_enabled);
    assert!(GeneralConfig::default().status_pages_enabled);
}

#[test]
fn test_cors_allow_all_defaults_to_false_when_absent() {
    let config: PipedashConfig = toml::from_str("[server]\nbind_addr = \"0.0.0.0:8080\"").unwrap();

    assert!(!config.server.cors_allow_all);
    assert!(!ServerConfig::default().cors_allow_all);
}

#[test]
fn test_push_relay_url_is_unset_by_default_and_read_from_the_push_table() {
    let unset: PipedashConfig = toml::from_str("[server]\nbind_addr = \"0.0.0.0:8080\"").unwrap();
    let set: PipedashConfig =
        toml::from_str("[push]\nrelay_url = \"https://relay.example.com\"").unwrap();

    assert!(unset.push.relay_url.is_empty());
    assert!(!toml::to_string(&unset).unwrap().contains("[push]"));
    assert_eq!(set.push.relay_url, "https://relay.example.com");
    assert!(toml::to_string(&set).unwrap().contains("relay_url"));
    assert_eq!(
        ConfigKey::PushRelayUrl.env_var_name(),
        "PIPEDASH_PUSH_RELAY_URL"
    );
}

#[test]
fn test_storage_backend_parsing() {
    assert_eq!(
        "sqlite".parse::<StorageBackend>().unwrap(),
        StorageBackend::Sqlite
    );
    assert_eq!(
        "postgres".parse::<StorageBackend>().unwrap(),
        StorageBackend::Postgres
    );
    assert_eq!(
        "postgresql".parse::<StorageBackend>().unwrap(),
        StorageBackend::Postgres
    );
}

#[test]
fn test_storage_backend_helpers() {
    assert!(StorageBackend::Postgres.requires_postgres());
    assert!(!StorageBackend::Sqlite.requires_postgres());
    assert!(StorageBackend::Sqlite.is_sqlite());
    assert!(!StorageBackend::Postgres.is_sqlite());
}

#[test]
fn test_storage_config_validation() {
    let sqlite_config = StorageConfig::default();
    assert!(sqlite_config.validate().is_ok());

    let pg_config = StorageConfig {
        backend: StorageBackend::Postgres,
        ..Default::default()
    };
    assert!(pg_config.validate().is_err());

    let pg_config_with_conn = StorageConfig {
        backend: StorageBackend::Postgres,
        postgres: PostgresConfig {
            connection_string: "postgres://localhost/test".to_string(),
        },
        ..Default::default()
    };
    assert!(pg_config_with_conn.validate().is_ok());
}

#[test]
fn test_provider_display_name() {
    let with_name = ProviderFileConfig {
        name: Some("My Stub".to_string()),
        provider_type: "stub".to_string(),
        ..Default::default()
    };
    assert_eq!(with_name.display_name("stub-main"), "My Stub");

    let without_name = ProviderFileConfig {
        name: None,
        provider_type: "stub".to_string(),
        ..Default::default()
    };
    assert_eq!(without_name.display_name("stub-main"), "stub-main");
}

#[test]
fn test_config_key_env_vars() {
    assert_eq!(
        ConfigKey::MetricsEnabled.env_var_name(),
        "PIPEDASH_METRICS_ENABLED"
    );
    assert_eq!(
        ConfigKey::StorageBackend.env_var_name(),
        "PIPEDASH_STORAGE_BACKEND"
    );
    assert_eq!(
        ConfigKey::PostgresConnectionString.env_var_name(),
        "PIPEDASH_POSTGRES_URL"
    );
}

#[test]
fn test_config_paths() {
    let config = PipedashConfig::default();
    let data_dir = config.data_dir();

    assert!(config.db_path().starts_with(&data_dir));
    assert!(config.cache_dir().starts_with(&data_dir));
}
