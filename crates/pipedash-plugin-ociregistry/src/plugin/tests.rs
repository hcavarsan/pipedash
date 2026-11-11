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

#[tokio::test]
async fn an_uppercase_loopback_host_keeps_its_port_for_matching_and_ping() {
    use std::io::{
        Read,
        Write,
    };

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let read = stream.read(&mut buf).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .unwrap();
        String::from_utf8_lossy(&buf[..read]).into_owned()
    });
    let configured = format!("LOCALHOST:{port}");
    let loopback = plugin(&[("registry_host", configured.as_str())]);

    assert!(loopback
        .registry_auth(&format!("localhost:{port}"))
        .await
        .unwrap()
        .is_some());
    assert!(loopback.registry_auth("localhost").await.unwrap().is_none());
    assert!(loopback.validate_credentials().await.unwrap());
    assert!(server.join().unwrap().starts_with("GET /v2/ HTTP/1.1"));

    let http_default = plugin(&[("registry_host", "LOCALHOST:80")]);
    assert!(http_default
        .registry_auth("localhost")
        .await
        .unwrap()
        .is_some());
    assert!(http_default
        .registry_auth("localhost:80")
        .await
        .unwrap()
        .is_some());

    let https_port = plugin(&[("registry_host", "LOCALHOST:443")]);
    assert!(https_port
        .registry_auth("localhost:443")
        .await
        .unwrap()
        .is_some());
    assert!(https_port
        .registry_auth("localhost")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn a_default_port_in_registry_host_matches_the_bare_host() {
    let with_port = plugin(&[("registry_host", "Registry.Test:443")]);
    for host in ["registry.test", "REGISTRY.test:443"] {
        assert!(with_port.registry_auth(host).await.unwrap().is_some());
    }
    assert!(with_port
        .registry_auth("registry.test:5000")
        .await
        .unwrap()
        .is_none());

    let bare = plugin(&[("registry_host", "registry.test")]);
    assert!(bare
        .registry_auth("registry.test:443")
        .await
        .unwrap()
        .is_some());

    let custom = plugin(&[("registry_host", "registry.test:8443")]);
    assert!(custom
        .registry_auth("registry.test")
        .await
        .unwrap()
        .is_none());
    assert!(custom
        .registry_auth("registry.test:8443")
        .await
        .unwrap()
        .is_some());

    let hub = plugin(&[("registry_host", "docker.io:443")]);
    assert_eq!(
        hub.registry_auth("docker.io")
            .await
            .unwrap()
            .unwrap()
            .token_hosts,
        ["auth.docker.io"]
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
