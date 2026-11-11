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

#[test]
fn namespaces_are_dns_labels_of_at_most_63_characters() {
    assert!(is_namespace("team-apps"));
    assert!(is_namespace("a-1"));
    assert!(is_namespace(&"a".repeat(63)));
    for rejected in ["", "-a", "a-", "A", "a_b", "a.b", &"a".repeat(64)] {
        assert!(!is_namespace(rejected), "{rejected:?} must be rejected");
    }
}

#[test]
fn resource_names_are_dot_separated_dns_labels() {
    assert!(is_resource_name("web.v2"));
    assert!(is_resource_name("app"));
    for rejected in ["", "..", "a..b", ".a", "a.", "A", "a_b", &"a".repeat(254)] {
        assert!(!is_resource_name(rejected), "{rejected:?} must be rejected");
    }
}

struct Object {
    name: String,
    namespace: String,
    owners: Vec<(String, String)>,
}

impl OwnedObject for Object {
    fn namespace(&self) -> &str {
        &self.namespace
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn owner_references(&self) -> impl Iterator<Item = (&str, &str)> {
        self.owners
            .iter()
            .map(|(kind, name)| (kind.as_str(), name.as_str()))
    }
}

fn object(name: &str, owner: Option<(&str, &str)>) -> Object {
    Object {
        name: name.to_string(),
        namespace: "web".to_string(),
        owners: owner
            .map(|(kind, name)| (kind.to_string(), name.to_string()))
            .into_iter()
            .collect(),
    }
}

fn keys<'a>(entries: &[(&'a str, &'a str, &'a str)]) -> Vec<ObjectKey<'a>> {
    entries
        .iter()
        .map(|&(kind, namespace, name)| ObjectKey {
            kind,
            namespace,
            name,
        })
        .collect()
}

fn names(objects: &[Object]) -> Vec<&str> {
    objects.iter().map(|object| object.name.as_str()).collect()
}

#[test]
fn deployment_pods_are_found_through_their_replica_set() {
    let managed = keys(&[("Deployment", "web", "api")]);
    let replica_sets = vec![
        object("api-7f", Some(("Deployment", "api"))),
        object("db-1", Some(("Deployment", "db"))),
    ];
    let pods = vec![
        object("api-7f-a", Some(("ReplicaSet", "api-7f"))),
        object("db-1-a", Some(("ReplicaSet", "db-1"))),
    ];
    let owners = Owners {
        managed: &managed,
        parents: &[("ReplicaSet", replica_sets.as_slice())],
    };

    let found = owned_by(pods, &owners);

    assert_eq!(names(&found), ["api-7f-a"]);
}

#[test]
fn cron_job_pods_are_found_through_their_job() {
    let managed = keys(&[("CronJob", "web", "nightly")]);
    let jobs = vec![object("nightly-28", Some(("CronJob", "nightly")))];
    let pods = vec![
        object("nightly-28-x", Some(("Job", "nightly-28"))),
        object("other-1-x", Some(("Job", "other-1"))),
    ];
    let owners = Owners {
        managed: &managed,
        parents: &[("Job", jobs.as_slice())],
    };

    let found = owned_by(pods, &owners);

    assert_eq!(names(&found), ["nightly-28-x"]);
}

#[test]
fn stateful_set_and_bare_pods_are_owned_directly() {
    let managed = keys(&[("StatefulSet", "web", "db"), ("Pod", "web", "lonely")]);
    let pods = vec![
        object("db-0", Some(("StatefulSet", "db"))),
        object("lonely", None),
        object("stranger", None),
    ];
    let owners = Owners::<Object> {
        managed: &managed,
        parents: &[],
    };

    let found = owned_by(pods, &owners);

    assert_eq!(names(&found), ["db-0", "lonely"]);
}

#[test]
fn unrelated_pods_in_the_same_namespace_are_excluded() {
    let managed = keys(&[("Deployment", "web", "api")]);
    let replica_sets = vec![object("api-7f", Some(("Deployment", "api")))];
    let pods = vec![
        object("stranger", None),
        object("elsewhere", Some(("ReplicaSet", "unknown"))),
        object("same-name", Some(("Deployment", "other"))),
    ];
    let owners = Owners {
        managed: &managed,
        parents: &[("ReplicaSet", replica_sets.as_slice())],
    };

    assert!(owned_by(pods, &owners).is_empty());
}

#[test]
fn resources_in_another_namespace_do_not_own_pods() {
    let managed = keys(&[("StatefulSet", "other", "db")]);
    let owners = Owners::<Object> {
        managed: &managed,
        parents: &[],
    };

    assert!(owned_by(vec![object("db-0", Some(("StatefulSet", "db")))], &owners).is_empty());
}

#[test]
fn a_parent_of_another_kind_does_not_link_a_pod_to_its_owner() {
    let managed = keys(&[("Deployment", "web", "api")]);
    let jobs = vec![object("api-7f", Some(("Deployment", "api")))];
    let pods = vec![object("api-7f-a", Some(("ReplicaSet", "api-7f")))];
    let owners = Owners {
        managed: &managed,
        parents: &[("Job", jobs.as_slice())],
    };

    assert!(owned_by(pods, &owners).is_empty());
}

#[test]
fn borrowed_pods_are_filtered_without_cloning() {
    let managed = keys(&[("StatefulSet", "web", "db")]);
    let pods = [
        object("db-0", Some(("StatefulSet", "db"))),
        object("stranger", None),
    ];
    let owners = Owners::<Object> {
        managed: &managed,
        parents: &[],
    };

    let found = owned_by(pods.iter(), &owners);

    assert_eq!(found.len(), 1);
    assert!(std::ptr::eq(found[0], &pods[0]));
}
