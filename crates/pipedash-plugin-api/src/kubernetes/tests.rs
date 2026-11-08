use super::*;

const KUBECONFIG: &str = "apiVersion: v1\nkind: Config\ncurrent-context: c\ncontexts:\n- name: c\n  context: {cluster: k, user: u}\nclusters:\n- name: k\n  cluster: {server: 'https://example.invalid'}\nusers:\n- name: u\n  user:\n    exec: {apiVersion: client.authentication.k8s.io/v1beta1, command: helper}\n";

#[test]
fn exec_auth_is_rejected_only_on_mobile() {
    let kubeconfig = Kubeconfig::from_yaml(KUBECONFIG).unwrap();
    assert!(ensure_supported_auth(&kubeconfig, None, false).is_ok());
    assert!(matches!(
        ensure_supported_auth(&kubeconfig, None, true),
        Err(PluginError::NotSupported(_))
    ));
}

#[test]
fn missing_files_are_reported() {
    assert!(matches!(
        read_kubeconfigs(&["/nonexistent/kubeconfig".into()]),
        Err(PluginError::InvalidConfig(_))
    ));
}

const TWO_CONTEXTS: &str = "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster:\n    server: https://127.0.0.1:6443\ncontexts:\n- name: ctx\n  context:\n    cluster: c\n    user: u\n- name: other\n  context:\n    cluster: c\n    user: u\ncurrent-context: ctx\nusers:\n- name: u\n  user:\n    token: t\n";

const DUPLICATE_CONTEXTS: &str = "apiVersion: v1\nkind: Config\ncontexts:\n- name: zeta\n  context: {cluster: c, user: u}\n- name: alpha\n  context: {cluster: c, user: u}\n- name: zeta\n  context: {cluster: c, user: u}\n";

const SECRET: &str = "users: {token: s3cr3t-value";

fn config(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn pasted_content_wins_over_a_configured_path() {
    let source = KubeconfigSource::from_config(
        &config(&[("path", "/a/one"), ("token", "apiVersion: v1")]),
        "path",
    );

    assert_eq!(
        source,
        KubeconfigSource::Content("apiVersion: v1".to_string())
    );
}

#[test]
fn blank_content_falls_back_to_the_split_and_trimmed_path() {
    let value = if cfg!(windows) {
        "/a/one; /a/two ;;"
    } else {
        "/a/one: /a/two ::"
    };
    let source =
        KubeconfigSource::from_config(&config(&[("path", value), ("token", "  \n ")]), "path");

    assert_eq!(
        source,
        KubeconfigSource::Path(vec!["/a/one".to_string(), "/a/two".to_string()])
    );
}

#[test]
fn a_variable_holding_several_files_expands_to_each_file() {
    let separator = if cfg!(windows) { ";" } else { ":" };
    std::env::set_var(
        "PIPEDASH_TEST_KUBECONFIG_LIST",
        ["/a/one", " /a/two ", ""].join(separator),
    );
    let value = ["$PIPEDASH_TEST_KUBECONFIG_LIST", "/a/three"].join(separator);

    assert_eq!(
        KubeconfigSource::from_config(&config(&[("path", &value)]), "path"),
        KubeconfigSource::Path(vec![
            "/a/one".to_string(),
            "/a/two".to_string(),
            "/a/three".to_string()
        ])
    );
}

#[test]
fn missing_or_blank_path_falls_back_to_the_default() {
    let default = KubeconfigSource::Path(kubeconfig_paths(&default_kubeconfig_path()));

    assert_eq!(KubeconfigSource::from_config(&config(&[]), "path"), default);
    assert_eq!(
        KubeconfigSource::from_config(&config(&[("path", " ")]), "path"),
        default
    );
}

#[test]
fn pasted_content_is_never_printed() {
    let source = KubeconfigSource::from_config(&config(&[("token", SECRET)]), "path");

    assert_eq!(format!("{source:?}"), "Content(<redacted>)");
    assert!(!source.describe().contains("s3cr3t-value"));
}

#[test]
fn pasted_content_loads_without_touching_the_filesystem() {
    let kubeconfig = KubeconfigSource::Content(TWO_CONTEXTS.to_string())
        .load()
        .unwrap();

    assert_eq!(kubeconfig.contexts.len(), 2);
}

#[test]
fn invalid_pasted_content_is_rejected_without_echoing_it() {
    let error = KubeconfigSource::Content(SECRET.to_string())
        .load()
        .unwrap_err();

    assert!(matches!(&error, PluginError::InvalidConfig(_)), "{error:?}");
    assert!(!error.to_string().contains("s3cr3t-value"), "{error}");
}

#[tokio::test]
async fn available_contexts_are_sorted_and_deduplicated() {
    let contexts = available_contexts(
        || parse_kubeconfig_content(DUPLICATE_CONTEXTS),
        "src".into(),
    )
    .await
    .unwrap();

    assert_eq!(contexts, ["alpha", "zeta"]);
}

#[tokio::test]
async fn a_kubeconfig_without_contexts_names_its_source() {
    let error = available_contexts(
        || parse_kubeconfig_content("apiVersion: v1\nkind: Config\n"),
        "/a/one".into(),
    )
    .await
    .unwrap_err();

    assert!(matches!(&error, PluginError::InvalidConfig(message) if message.contains("/a/one")));
}
