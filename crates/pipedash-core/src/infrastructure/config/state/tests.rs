use tempfile::TempDir;

use super::*;
use crate::application::test_support::plugins::registry;
use crate::infrastructure::config::schema::ProviderFileConfig;
use crate::{
    infrastructure::MemoryTokenStore,
    test_env::EnvGuard,
};

fn create_test_state(temp_dir: &TempDir, token_store: Arc<dyn TokenStore>) -> Arc<ConfigState> {
    let config_path = temp_dir.path().join("config.toml");
    let config = PipedashConfig::default();
    ConfigState::from_config(config, config_path, token_store, registry())
}

#[tokio::test]
async fn test_config_state_from_config() {
    let temp_dir = TempDir::new().unwrap();
    let token_store = Arc::new(MemoryTokenStore::new());

    let state = create_test_state(&temp_dir, token_store);

    let config = state.get().await;
    assert!(config.general.metrics_enabled);
}

#[tokio::test]
async fn test_add_remove_provider() {
    let temp_dir = TempDir::new().unwrap();
    let token_store = Arc::new(MemoryTokenStore::new());

    let state = create_test_state(&temp_dir, token_store);

    let provider = ProviderFileConfig {
        name: Some("Test Stub".to_string()),
        provider_type: "stub".to_string(),
        token: "${STUB_TOKEN:-}".to_string(),
        refresh_interval: 30,
        config: std::collections::HashMap::new(),
    };

    state.remember_provider("test-stub", &provider).await;

    let config = state.get().await;
    assert_eq!(config.providers.len(), 1);
    assert!(config.providers.contains_key("test-stub"));

    state.remove_provider("test-stub").await.unwrap();

    let config = state.get().await;
    assert!(config.providers.is_empty());
}

#[tokio::test]
async fn test_reload_replaces_in_memory_config() {
    let temp_dir = TempDir::new().unwrap();
    let token_store = Arc::new(MemoryTokenStore::new());

    let state = create_test_state(&temp_dir, token_store);

    let mut on_disk = PipedashConfig::default();
    on_disk.general.default_refresh_interval = 123;
    std::fs::write(
        temp_dir.path().join("config.toml"),
        ConfigLoader::to_toml(&on_disk).unwrap(),
    )
    .unwrap();

    state.reload().await.unwrap();

    assert_eq!(state.get().await.general.default_refresh_interval, 123);
}

#[tokio::test]
async fn loading_an_invalid_file_names_the_field_in_plain_text() {
    let temp_dir = TempDir::new().unwrap();
    let state = create_test_state(&temp_dir, Arc::new(MemoryTokenStore::new()));
    std::fs::write(
        temp_dir.path().join("config.toml"),
        "[views.team]\nname = \"\"\nname_contains = \"api\"\n",
    )
    .unwrap();

    let error = state.load_file().await.unwrap_err().to_string();

    assert!(
        error.contains("views.team.name: View name is required"),
        "{error}"
    );
    assert!(!error.contains("ConfigError {"), "{error}");
}

#[tokio::test]
async fn test_get_config_values() {
    let _env = EnvGuard::set(&[
        ("PIPEDASH_METRICS_ENABLED", None),
        ("PIPEDASH_DEFAULT_REFRESH_INTERVAL", None),
    ]);

    let temp_dir = TempDir::new().unwrap();
    let token_store = Arc::new(MemoryTokenStore::new());

    let state = create_test_state(&temp_dir, token_store);

    assert!(state.get_bool(ConfigKey::MetricsEnabled).await);
    assert_eq!(
        state.get_string(ConfigKey::DefaultRefreshInterval).await,
        "30"
    );
}

#[tokio::test]
async fn test_env_var_override() {
    let _env = EnvGuard::set(&[("PIPEDASH_METRICS_ENABLED", Some("false"))]);

    let temp_dir = TempDir::new().unwrap();
    let token_store = Arc::new(MemoryTokenStore::new());

    let state = create_test_state(&temp_dir, token_store);

    assert!(!state.get_bool(ConfigKey::MetricsEnabled).await);
}

#[tokio::test]
async fn test_push_relay_url_comes_from_the_file_unless_the_env_var_overrides_it() {
    let temp_dir = TempDir::new().unwrap();
    let mut config = PipedashConfig::default();
    config.push.relay_url = "https://file.example.com".to_string();
    let state = ConfigState::from_config(
        config,
        temp_dir.path().join("config.toml"),
        Arc::new(MemoryTokenStore::new()),
        registry(),
    );

    {
        let _env = EnvGuard::set(&[("PIPEDASH_PUSH_RELAY_URL", None)]);
        assert_eq!(
            state.get_string(ConfigKey::PushRelayUrl).await,
            "https://file.example.com"
        );
    }

    let _env = EnvGuard::set(&[("PIPEDASH_PUSH_RELAY_URL", Some("https://env.example.com"))]);
    assert_eq!(
        state.get_string(ConfigKey::PushRelayUrl).await,
        "https://env.example.com"
    );
}

#[tokio::test]
async fn test_set_push_relay_url_edits_only_the_push_table() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("config.toml");
    std::fs::write(&path, "# keep me\n[general]\nmetrics_enabled = false\n").unwrap();
    let state = ConfigState::from_config(
        PipedashConfig::default(),
        path.clone(),
        Arc::new(MemoryTokenStore::new()),
        registry(),
    );

    state
        .set_push_relay_url(" https://relay.pipedash.app/ ")
        .await
        .unwrap();
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("# keep me"));
    assert!(written.contains("[push]\nrelay_url = \"https://relay.pipedash.app\""));
    assert_eq!(
        state.get().await.push.relay_url,
        "https://relay.pipedash.app"
    );

    state.set_push_relay_url("").await.unwrap();
    let cleared = std::fs::read_to_string(&path).unwrap();
    assert!(!cleared.contains("push"));
    assert!(cleared.contains("# keep me"));
    assert!(state.get().await.push.relay_url.is_empty());
}

#[test]
fn test_push_relay_from_env_ignores_blank_values() {
    for (value, expected) in [
        (None, false),
        (Some("  "), false),
        (Some("https://r"), true),
    ] {
        let _env = EnvGuard::set(&[("PIPEDASH_PUSH_RELAY_URL", value)]);
        assert_eq!(ConfigState::push_relay_from_env(), expected);
    }
}

fn update(default_refresh_interval: u32, status_pages_enabled: bool) -> GeneralSettingsUpdate {
    GeneralSettingsUpdate {
        metrics_enabled: true,
        default_refresh_interval,
        status_pages_enabled,
        proxy_url: String::new(),
    }
}

#[tokio::test]
async fn test_update_general_settings_saves_a_proxy_and_refuses_an_invalid_one() {
    let temp_dir = TempDir::new().unwrap();
    let state = create_test_state(&temp_dir, Arc::new(MemoryTokenStore::new()));
    let path = temp_dir.path().join("config.toml");
    let with_proxy = |proxy_url: &str| GeneralSettingsUpdate {
        proxy_url: proxy_url.to_string(),
        ..update(30, true)
    };

    state
        .update_general_settings(with_proxy(" http://proxy.corp:3128 "))
        .await
        .unwrap();
    let refused = state
        .update_general_settings(with_proxy("http://alice:secret@proxy.corp:3128"))
        .await;

    assert!(matches!(refused, Err(DomainError::InvalidConfig(_))));
    assert_eq!(
        ConfigLoader::load(&path).unwrap().general.proxy_url,
        "http://proxy.corp:3128"
    );
    assert_eq!(
        state.get().await.general.proxy_url,
        "http://proxy.corp:3128"
    );

    state.update_general_settings(with_proxy("")).await.unwrap();

    assert_eq!(ConfigLoader::load(&path).unwrap().general.proxy_url, "");
}

#[tokio::test]
async fn test_update_general_settings_creates_the_general_table_and_survives_a_reload() {
    let _env = EnvGuard::set(&[("PIPEDASH_STATUS_PAGES_ENABLED", None)]);
    let temp_dir = TempDir::new().unwrap();
    let state = create_test_state(&temp_dir, Arc::new(MemoryTokenStore::new()));
    let path = temp_dir.path().join("config.toml");
    std::fs::write(&path, "# keep me\n[storage]\nbackend = \"sqlite\"\n").unwrap();
    let (health, mut receivers) = HealthHandle::channel();
    state.attach_health(health);

    state
        .update_general_settings(update(60, false))
        .await
        .unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    state.reload().await.unwrap();

    assert!(saved.contains("# keep me"));
    assert!(saved.contains("[general]"));
    assert!(
        !ConfigLoader::load(&path)
            .unwrap()
            .general
            .status_pages_enabled
    );
    let config = state.get().await;
    assert!(!config.general.status_pages_enabled);
    assert_eq!(config.general.default_refresh_interval, 60);
    assert!(!state.get_bool(ConfigKey::StatusPagesEnabled).await);
    assert!(matches!(
        receivers.inputs.try_recv(),
        Ok(HealthInput::SettingsChanged)
    ));
    assert!(matches!(
        receivers.inputs.try_recv(),
        Ok(HealthInput::SettingsChanged)
    ));
}

#[tokio::test]
async fn test_update_general_settings_rejects_intervals_outside_five_to_three_hundred() {
    let temp_dir = TempDir::new().unwrap();
    let state = create_test_state(&temp_dir, Arc::new(MemoryTokenStore::new()));
    let (health, mut receivers) = HealthHandle::channel();
    state.attach_health(health);

    for interval in [0, 4, 301, 100_000] {
        let result = state.update_general_settings(update(interval, false)).await;

        assert!(
            matches!(result, Err(DomainError::InvalidConfig(_))),
            "{interval}"
        );
    }
    assert!(state.get().await.general.status_pages_enabled);
    assert!(!temp_dir.path().join("config.toml").exists());
    assert!(receivers.inputs.try_recv().is_err());

    for interval in [5, 300] {
        state
            .update_general_settings(update(interval, true))
            .await
            .unwrap();
        assert_eq!(state.get().await.general.default_refresh_interval, interval);
        assert!(matches!(
            receivers.inputs.try_recv(),
            Ok(HealthInput::SettingsChanged)
        ));
    }
}

#[tokio::test]
async fn test_update_general_settings_leaves_providers_and_secrets_alone() {
    let _env = EnvGuard::set(&[("PIPEDASH_TEST_TOKEN", Some("secret-value"))]);
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("config.toml");
    let original = "[general]\nmetrics_enabled = true\n\n[providers.main]\ntype = \"stub\"\ntoken = \"${PIPEDASH_TEST_TOKEN}\"\nrefresh_interval = 45\n";
    std::fs::write(&path, original).unwrap();
    let config = ConfigLoader::load(&path).unwrap();
    assert_eq!(config.providers["main"].token, "secret-value");
    let state = ConfigState::from_config(
        config,
        path.clone(),
        Arc::new(MemoryTokenStore::new()),
        registry(),
    );

    state
        .update_general_settings(update(90, false))
        .await
        .unwrap();

    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        saved.contains("token = \"${PIPEDASH_TEST_TOKEN}\""),
        "{saved}"
    );
    assert!(!saved.contains("secret-value"), "{saved}");
    assert!(saved.contains("refresh_interval = 45"));
    assert!(saved.contains("status_pages_enabled = false"));
    assert!(saved.contains("default_refresh_interval = 90"));
}

#[tokio::test]
async fn test_update_general_settings_keeps_the_proxy_reference() {
    let _env = EnvGuard::set(&[("PIPEDASH_TEST_PROXY", Some("http://proxy.corp:3128"))]);
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("config.toml");
    std::fs::write(&path, "[general]\nproxy_url = \"${PIPEDASH_TEST_PROXY}\"\n").unwrap();
    let config = ConfigLoader::load(&path).unwrap();
    let state = ConfigState::from_config(
        config,
        path.clone(),
        Arc::new(MemoryTokenStore::new()),
        registry(),
    );
    let general = state.get().await.general.clone();
    let save = |proxy_url: &str| GeneralSettingsUpdate {
        metrics_enabled: general.metrics_enabled,
        default_refresh_interval: general.default_refresh_interval,
        status_pages_enabled: !general.status_pages_enabled,
        proxy_url: proxy_url.to_string(),
    };

    state
        .update_general_settings(save(&general.proxy_url))
        .await
        .unwrap();
    let kept = std::fs::read_to_string(&path).unwrap();
    state
        .update_general_settings(save("http://other.corp:8080"))
        .await
        .unwrap();
    let replaced = std::fs::read_to_string(&path).unwrap();

    assert!(
        kept.contains("proxy_url = \"${PIPEDASH_TEST_PROXY}\""),
        "{kept}"
    );
    assert!(
        replaced.contains("proxy_url = \"http://other.corp:8080\""),
        "{replaced}"
    );
}

#[tokio::test]
async fn test_validate_warns_only_for_types_missing_from_the_registry() {
    let temp_dir = TempDir::new().unwrap();
    let state = create_test_state(&temp_dir, Arc::new(MemoryTokenStore::new()));
    let mut config = PipedashConfig::default();
    for (id, provider_type) in [("known", "stub"), ("unknown", "not-registered")] {
        config.providers.insert(
            id.to_string(),
            ProviderFileConfig {
                provider_type: provider_type.to_string(),
                ..Default::default()
            },
        );
    }
    state.remember_providers(config.providers).await;

    let result = state.validate().await;
    let fields: Vec<_> = result
        .warnings
        .iter()
        .filter(|w| w.message.starts_with("Unknown provider type"))
        .map(|w| w.field.as_str())
        .collect();
    assert_eq!(fields, ["providers.unknown.type"]);
}
