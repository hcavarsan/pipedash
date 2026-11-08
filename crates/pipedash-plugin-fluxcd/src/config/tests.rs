use super::*;

fn config(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn pipeline_id_round_trips_kind_namespace_and_name() {
    let target = Target::new(Kind::HelmRelease, "apps", "web.v2").unwrap();

    assert_eq!(parse_pipeline_id(&target.selection_id()).unwrap(), target);
}

#[test]
fn pipeline_id_rejects_malformed_identities() {
    for id in [
        "fluxcd__Kustomization__ci__app",
        "Kustomization__ci",
        "Kustomization__ci__app__extra",
        "kustomization__ci__app",
        "Kustomization__CI__app",
        "Kustomization__ci__app_name",
        "Kustomization__ci__..",
        "Kustomization____app",
    ] {
        assert!(parse_pipeline_id(id).is_err(), "{id} must be rejected");
    }
}

#[test]
fn selection_requires_exactly_kind_namespace_and_name() {
    assert!(parse_selection("Kustomization__ci__app").is_ok());
    assert!(parse_selection("ci__app").is_err());
    assert!(parse_selection("Deployment__ci__app").is_err());
    assert!(parse_selection("HelmRelease__ci/other__app").is_err());
}

#[test]
fn settings_split_comma_separated_namespaces_and_selection_without_duplicates() {
    let settings = Settings::from_config(&config(&[
        ("namespaces", "ci, apps,ci"),
        (
            "selected_items",
            "Kustomization__ci__app, HelmRelease__apps__web,Kustomization__ci__app",
        ),
    ]))
    .unwrap();

    assert_eq!(settings.namespaces, vec!["ci", "apps"]);
    assert_eq!(settings.selection.len(), 2);
    assert_eq!(settings.monitored_namespaces(), vec!["apps", "ci"]);
}

#[test]
fn settings_reject_invalid_namespaces_and_selection_outside_them() {
    assert!(Settings::from_config(&config(&[("namespaces", "ci,Bad_NS")])).is_err());
    assert!(Settings::from_config(&config(&[
        ("namespaces", "ci"),
        ("selected_items", "Kustomization__other__app"),
    ]))
    .is_err());
}

#[test]
fn empty_namespaces_monitor_all_and_allow_any_valid_target() {
    let settings = Settings::from_config(&config(&[])).unwrap();
    let target = Target::new(Kind::Kustomization, "anywhere", "app").unwrap();

    assert!(settings.monitored_namespaces().is_empty());
    assert!(settings.ensure_allowed(&target).is_ok());
}

#[test]
fn selection_limits_which_targets_are_allowed() {
    let settings =
        Settings::from_config(&config(&[("selected_items", "Kustomization__ci__app")])).unwrap();
    let selected = Target::new(Kind::Kustomization, "ci", "app").unwrap();
    let sibling = Target::new(Kind::Kustomization, "ci", "other").unwrap();
    let same_name_other_kind = Target::new(Kind::HelmRelease, "ci", "app").unwrap();

    assert!(settings.ensure_allowed(&selected).is_ok());
    assert!(settings.ensure_allowed(&sibling).is_err());
    assert!(settings.ensure_allowed(&same_name_other_kind).is_err());
}

#[test]
fn kubeconfig_content_is_parsed_from_settings() {
    let settings =
        Settings::from_config(&config(&[("token", "apiVersion: v1\nkind: Config\n")])).unwrap();

    assert_eq!(
        settings.kubeconfig,
        KubeconfigSource::Content("apiVersion: v1\nkind: Config\n".to_string())
    );
}

#[test]
fn kubeconfig_content_wins_over_the_path() {
    let source = kubeconfig_source(&config(&[
        ("kubeconfig", "/home/me/.kube/config"),
        ("token", "apiVersion: v1"),
    ]));

    assert_eq!(
        source,
        KubeconfigSource::Content("apiVersion: v1".to_string())
    );
}

#[test]
fn blank_content_falls_back_to_the_path() {
    let source = kubeconfig_source(&config(&[
        ("kubeconfig", "/home/me/.kube/config"),
        ("token", "  \n "),
    ]));

    assert_eq!(
        source,
        KubeconfigSource::Path(vec!["/home/me/.kube/config".to_string()])
    );
}

#[test]
fn kubeconfig_content_is_never_printed() {
    let settings = Settings::from_config(&config(&[("token", "token: s3cr3t-value")])).unwrap();

    assert!(!format!("{settings:?}").contains("s3cr3t-value"));
    assert!(!describe(&settings.kubeconfig).contains("s3cr3t-value"));
}
