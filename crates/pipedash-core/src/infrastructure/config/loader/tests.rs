use tempfile::TempDir;

use super::*;
use crate::application::test_support::plugins::registry;
use crate::test_env::EnvGuard;

#[test]
fn test_platform_detect_default() {
    let platform = Platform::detect();
    assert!(matches!(platform, Platform::Desktop | Platform::Server));
}

#[test]
fn test_parse_simple_config() {
    let content = r#"
[general]
metrics_enabled = true
default_refresh_interval = 60

[storage]
backend = "sqlite"
"#;

    let config = ConfigLoader::parse(content).unwrap();
    assert!(config.general.metrics_enabled);
    assert_eq!(config.general.default_refresh_interval, 60);
}

#[test]
fn test_parse_with_providers() {
    let content = r#"
[providers.my-stub]
name = "My Stub"
type = "stub"
token = "STUB_TOKEN"
refresh_interval = 30

[providers.my-stub.config]
base_url = ""
"#;

    let config = ConfigLoader::parse(content).unwrap();
    assert_eq!(config.providers.len(), 1);
    assert!(config.providers.contains_key("my-stub"));
    assert_eq!(
        config.providers["my-stub"].name,
        Some("My Stub".to_string())
    );
    assert_eq!(config.providers["my-stub"].provider_type, "stub");
}

#[test]
fn test_setup_status_validates_with_the_registry() {
    let temp_dir = TempDir::new().unwrap();
    std::fs::write(
        temp_dir.path().join("config.toml"),
        "[providers.main]\ntype = \"stub\"\n",
    )
    .unwrap();

    let status = ConfigLoader::get_setup_status(temp_dir.path(), &registry());

    assert!(status.config_exists);
    assert!(status.config_valid);
    assert!(status.validation_errors.is_empty());
    assert!(!status.needs_setup);
}

#[test]
fn test_load_or_create() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config = ConfigLoader::load_or_create(&config_path, Platform::Desktop).unwrap();
    assert!(config_path.exists());
    assert!(config.general.metrics_enabled);

    let config2 = ConfigLoader::load(&config_path).unwrap();
    assert!(config2.general.metrics_enabled);
}

#[test]
fn test_env_var_interpolation() {
    std::env::set_var("TEST_BIND_ADDR", "0.0.0.0:9999");

    let content = r#"
[server]
bind_addr = "${TEST_BIND_ADDR}"
"#;

    let config = ConfigLoader::parse(content).unwrap();
    assert_eq!(config.server.bind_addr, "0.0.0.0:9999");

    std::env::remove_var("TEST_BIND_ADDR");
}

#[test]
fn test_env_var_with_default() {
    let content = r#"
[server]
bind_addr = "${NONEXISTENT_VAR:-127.0.0.1:8080}"
"#;

    let config = ConfigLoader::parse(content).unwrap();
    assert_eq!(config.server.bind_addr, "127.0.0.1:8080");
}

#[test]
fn test_to_toml() {
    let config = PipedashConfig::default();
    let toml_str = ConfigLoader::to_toml(&config).unwrap();

    assert!(toml_str.contains("[general]"));
    assert!(toml_str.contains("metrics_enabled"));
}

#[test]
fn test_save_creates_the_general_table_when_the_file_has_none() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("config.toml");
    std::fs::write(&path, "# my config\n[storage]\nbackend = \"sqlite\"\n").unwrap();
    let mut config = ConfigLoader::load(&path).unwrap();
    assert!(config.general.status_pages_enabled);
    config.general.status_pages_enabled = false;

    ConfigLoader::save(&config, &path).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# my config"));
    assert!(text.contains("[general]"));
    let reloaded = ConfigLoader::load(&path).unwrap();
    assert!(!reloaded.general.status_pages_enabled);
    assert_eq!(reloaded.general.default_refresh_interval, 30);
}

#[test]
fn test_save_with_the_default_data_dir_drops_the_old_path() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("config.toml");
    std::fs::write(
        &path,
        "[storage]\n# where data lives\ndata_dir = \"/old/custom/dir\"\nbackend = \"sqlite\"\n",
    )
    .unwrap();
    let mut config = ConfigLoader::load(&path).unwrap();
    config.storage.data_dir.clear();

    ConfigLoader::save(&config, &path).unwrap();

    let reloaded = ConfigLoader::load(&path).unwrap();
    assert!(reloaded.storage.data_dir.is_empty());
    assert_eq!(reloaded.data_dir(), PipedashConfig::default_data_dir());
}

#[test]
fn test_save_updates_an_existing_general_table_in_place() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("config.toml");
    std::fs::write(
        &path,
        "[general]\n# how often to refresh\ndefault_refresh_interval = 60\n",
    )
    .unwrap();
    let mut config = ConfigLoader::load(&path).unwrap();
    config.general.status_pages_enabled = false;

    ConfigLoader::save(&config, &path).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("# how often to refresh"));
    assert_eq!(text.matches("[general]").count(), 1);
    assert!(
        !ConfigLoader::load(&path)
            .unwrap()
            .general
            .status_pages_enabled
    );
}

const REFERENCED_CONFIG: &str = "[server]\nbind_addr = \"${PIPEDASH_TEST_BIND}\"\n\n[storage]\nbackend = \"postgres\"\ndata_dir = \"${PIPEDASH_TEST_DATA_DIR}\"\n\n[storage.postgres]\nconnection_string = \"${PIPEDASH_TEST_PG_URL}\"\n";

fn referenced_env() -> EnvGuard {
    EnvGuard::set(&[
        ("PIPEDASH_TEST_BIND", Some("0.0.0.0:9090")),
        ("PIPEDASH_TEST_DATA_DIR", Some("/srv/pipedash")),
        (
            "PIPEDASH_TEST_PG_URL",
            Some("postgres://pipedash:s3cret@db:5432/pipedash"),
        ),
    ])
}

#[test]
fn test_save_keeps_variable_references_that_resolve_to_the_saved_values() {
    let _env = referenced_env();
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("config.toml");
    std::fs::write(&path, REFERENCED_CONFIG).unwrap();
    let config = ConfigLoader::load(&path).unwrap();

    ConfigLoader::save(&config, &path).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("s3cret"), "{text}");
    for reference in [
        "${PIPEDASH_TEST_BIND}",
        "${PIPEDASH_TEST_DATA_DIR}",
        "${PIPEDASH_TEST_PG_URL}",
    ] {
        assert!(
            text.contains(reference),
            "{reference} missing from:\n{text}"
        );
    }
}

#[test]
fn test_save_replaces_variable_references_when_the_value_changes() {
    let _env = referenced_env();
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("config.toml");
    std::fs::write(&path, REFERENCED_CONFIG).unwrap();
    let mut config = ConfigLoader::load(&path).unwrap();
    config.server.bind_addr = "127.0.0.1:8080".to_string();
    config.storage.data_dir = "/var/lib/pipedash".to_string();
    config.storage.postgres.connection_string = "postgres://other@db2/pipedash".to_string();

    ConfigLoader::save(&config, &path).unwrap();

    let saved = ConfigLoader::load(&path).unwrap();
    assert_eq!(saved.server.bind_addr, "127.0.0.1:8080");
    assert_eq!(saved.storage.data_dir, "/var/lib/pipedash");
    assert_eq!(
        saved.storage.postgres.connection_string,
        "postgres://other@db2/pipedash"
    );
}

#[test]
fn test_discover_config_path_env_override() {
    let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", Some("/custom/path/config.toml"))]);
    let path = ConfigLoader::discover_config_path();
    assert_eq!(path, PathBuf::from("/custom/path/config.toml"));
}
