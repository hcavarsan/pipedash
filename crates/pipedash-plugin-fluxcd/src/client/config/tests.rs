use super::*;

const KUBECONFIG: &str = "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster:\n    server: https://127.0.0.1:6443\ncontexts:\n- name: ctx\n  context:\n    cluster: c\n    user: u\n- name: other\n  context:\n    cluster: c\n    user: u\ncurrent-context: ctx\nusers:\n- name: u\n  user:\n    token: t\n";

#[test]
fn pasted_content_loads_without_touching_the_filesystem() {
    let source = KubeconfigSource::Content(KUBECONFIG.to_string());

    let kubeconfig = load_kubeconfig(&source).unwrap();

    assert_eq!(kubeconfig.contexts.len(), 2);
}

#[test]
fn invalid_pasted_content_is_rejected_without_echoing_it() {
    let source = KubeconfigSource::Content("users: {token: s3cr3t-value".to_string());

    let error = load_kubeconfig(&source).unwrap_err();

    assert!(matches!(&error, PluginError::InvalidConfig(_)), "{error:?}");
    assert!(!error.to_string().contains("s3cr3t-value"), "{error}");
}
