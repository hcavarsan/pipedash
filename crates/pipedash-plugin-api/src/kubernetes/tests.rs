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

#[test]
fn kubeconfig_source_prefers_pasted_content_and_never_prints_it() {
    let config = |content: &str| HashMap::from([("token".to_string(), content.to_string())]);
    let pasted = KubeconfigSource::from_config(&config("apiVersion: v1"), "token", || 1u8);
    assert_eq!(
        pasted,
        KubeconfigSource::Content("apiVersion: v1".to_string())
    );
    assert_eq!(format!("{pasted:?}"), "Content(<redacted>)");

    let blank = KubeconfigSource::from_config(&config("  "), "token", || 1u8);
    assert_eq!(blank, KubeconfigSource::Path(1));
    assert_eq!(format!("{blank:?}"), "Path(1)");
}
