use super::*;

const KUBECONFIG: &str = "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster:\n    server: https://127.0.0.1:6443\ncontexts:\n- name: ctx\n  context:\n    cluster: c\n    user: u\n- name: other\n  context:\n    cluster: c\n    user: u\ncurrent-context: ctx\nusers:\n- name: u\n  user:\n    token: t\n";

#[test]
fn pasted_content_parses_without_touching_the_filesystem() {
    let kubeconfig = parse_kubeconfig_content(KUBECONFIG).unwrap();

    assert_eq!(kubeconfig.contexts.len(), 2);
}

#[test]
fn invalid_pasted_content_is_rejected_without_echoing_it() {
    let error = parse_kubeconfig_content("users: {token: s3cr3t-value").unwrap_err();

    assert!(matches!(&error, PluginError::InvalidConfig(_)), "{error:?}");
    assert!(!error.to_string().contains("s3cr3t-value"), "{error}");
}

#[test]
fn unreadable_files_are_skipped_and_readable_ones_concatenated() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good");
    let broken = dir.path().join("broken");
    let missing = dir.path().join("missing");
    std::fs::write(&good, KUBECONFIG).unwrap();
    std::fs::write(&broken, "users: {token: ").unwrap();
    let path = |path: &std::path::Path| path.to_string_lossy().into_owned();

    let merged = merge_kubeconfigs(vec![
        path(&broken),
        path(&good),
        path(&good),
        path(&missing),
    ])
    .unwrap();

    assert_eq!(merged.contexts.len(), 4);
    assert_eq!(merged.current_context.as_deref(), Some("ctx"));
    assert!(merge_kubeconfigs(vec![path(&broken), path(&missing)]).is_err());
}
