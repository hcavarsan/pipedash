use super::*;

#[test]
fn repository_name_ignores_hosts_and_ports() {
    for url in [
        "https://github.com/acme/web.git",
        "http://gitea:3000/acme/web.git",
        "ssh://git@git.example.com:2222/acme/web",
        "git@github.com:acme/web.git",
    ] {
        assert_eq!(repository_name(url), "acme/web", "{url}");
        assert_eq!(extract_git_org(url), "acme", "{url}");
    }
}

fn config(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn only_the_kubeconfig_mode_uses_the_cluster() {
    assert!(uses_kubeconfig(&config(&[("auth_mode", "kubeconfig")])));
    for mode in ["token", "sign_in", "none", "bogus"] {
        assert!(!uses_kubeconfig(&config(&[("auth_mode", mode)])), "{mode}");
    }
    assert!(!uses_kubeconfig(&config(&[])));
}

#[test]
fn cluster_settings_split_namespaces_and_read_the_context() {
    let settings = ClusterSettings::from_config(&config(&[
        ("namespaces", "argocd, team-a"),
        ("context", " prod "),
    ]))
    .unwrap();

    assert_eq!(settings.namespaces, ["argocd", "team-a"]);
    assert_eq!(settings.context.as_deref(), Some("prod"));
}

#[test]
fn cluster_settings_without_namespaces_read_every_namespace() {
    for namespaces in [None, Some(""), Some("  ")] {
        let pairs: Vec<_> = namespaces
            .map(|value| ("namespaces", value))
            .into_iter()
            .collect();
        let settings = ClusterSettings::from_config(&config(&pairs)).unwrap();

        assert!(settings.namespaces.is_empty(), "{namespaces:?}");
        assert_eq!(settings.context, None);
    }
}

#[test]
fn cluster_settings_reject_invalid_namespaces() {
    let error = ClusterSettings::from_config(&config(&[("namespaces", "argocd,Bad_NS")]))
        .err()
        .unwrap();

    assert!(
        matches!(&error, PluginError::InvalidConfig(message) if message.contains("Bad_NS")),
        "{error:?}"
    );
}
