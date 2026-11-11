use super::*;

fn plugin(extra: &[(&str, &str)]) -> OciRegistryPlugin {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut config = HashMap::from([
        (
            "registry_host".to_string(),
            "Registry.Example.com:5000".to_string(),
        ),
        ("token".to_string(), "secret".to_string()),
    ]);
    config.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    let mut plugin = OciRegistryPlugin::new();
    plugin
        .initialize(1, config, Some(Arc::new(reqwest::Client::new())))
        .unwrap();
    plugin
}

#[tokio::test]
async fn registry_auth_matches_only_the_configured_host() {
    let bearer = plugin(&[]);
    let auth = bearer
        .registry_auth("registry.example.com:5000")
        .await
        .unwrap()
        .unwrap();
    assert!(auth.bearer);
    assert_eq!(auth.username, None);
    assert_eq!(auth.secret, "secret");
    assert!(bearer
        .registry_auth("REGISTRY.example.com:5000")
        .await
        .unwrap()
        .is_some());
    assert!(bearer
        .registry_auth("registry.example.com")
        .await
        .unwrap()
        .is_none());
    assert!(bearer.registry_auth("ghcr.io").await.unwrap().is_none());

    let basic = plugin(&[("username", "bot")]);
    let auth = basic
        .registry_auth("registry.example.com:5000")
        .await
        .unwrap()
        .unwrap();
    assert!(!auth.bearer);
    assert_eq!(auth.username.as_deref(), Some("bot"));

    let blank = plugin(&[("username", "  ")]);
    let auth = blank
        .registry_auth("registry.example.com:5000")
        .await
        .unwrap()
        .unwrap();
    assert!(auth.bearer);
}

#[tokio::test]
async fn token_hosts_come_from_the_config_and_the_hub_default() {
    let hosts = |plugin: OciRegistryPlugin, registry: &'static str| async move {
        plugin
            .registry_auth(registry)
            .await
            .unwrap()
            .unwrap()
            .token_hosts
    };
    let own = "registry.example.com:5000";

    assert!(hosts(plugin(&[]), own).await.is_empty());
    assert_eq!(
        hosts(plugin(&[("token_host", " Auth.Example.com ")]), own).await,
        ["auth.example.com"]
    );
    assert_eq!(
        hosts(plugin(&[("token_host", "  ")]), own).await,
        Vec::<String>::new()
    );
    for hub in ["docker.io", "index.docker.io", "registry-1.docker.io"] {
        assert_eq!(
            hosts(plugin(&[("registry_host", hub)]), hub).await,
            ["auth.docker.io"]
        );
    }
    assert_eq!(
        hosts(
            plugin(&[
                ("registry_host", "docker.io"),
                ("token_host", "tokens.test")
            ]),
            "docker.io"
        )
        .await,
        ["tokens.test", "auth.docker.io"]
    );
}

#[test]
fn the_token_host_field_is_optional_and_validated() {
    let fields = metadata::create_metadata().config_schema.fields;
    let find = |key: &str| fields.iter().find(|field| field.key == key).unwrap();
    let field = find("token_host");

    assert!(!field.required);
    assert!(matches!(field.section, ConfigSection::Authentication));
    assert!(field.validation_regex.is_some());
    assert_eq!(
        field.validation_regex,
        find("registry_host").validation_regex
    );
}

#[tokio::test]
async fn an_uninitialized_plugin_shares_no_credentials() {
    let plugin = OciRegistryPlugin::new();
    assert!(plugin.registry_auth("").await.unwrap().is_none());
    assert!(matches!(
        plugin.validate_credentials().await,
        Err(PluginError::Internal(_))
    ));
}

#[test]
fn initialize_requires_a_host_and_a_token() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut plugin = OciRegistryPlugin::new();
    let client = Some(Arc::new(reqwest::Client::new()));
    let no_token = HashMap::from([("registry_host".to_string(), "ghcr.io".to_string())]);
    assert!(plugin.initialize(1, no_token, client.clone()).is_err());
    let no_host = HashMap::from([("token".to_string(), "t".to_string())]);
    assert!(plugin.initialize(1, no_host, client).is_err());
}

#[tokio::test]
async fn pipelines_are_empty_and_runs_are_not_supported() {
    let plugin = plugin(&[]);
    assert!(plugin.fetch_pipelines().await.unwrap().is_empty());
    assert!(matches!(
        plugin.fetch_run_history("x", 1).await,
        Err(PluginError::NotSupported(_))
    ));
    assert!(matches!(
        plugin.fetch_run_details("x", 1).await,
        Err(PluginError::NotSupported(_))
    ));
    assert!(matches!(
        plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: "x".to_string(),
                inputs: None,
            })
            .await,
        Err(PluginError::NotSupported(_))
    ));
}
