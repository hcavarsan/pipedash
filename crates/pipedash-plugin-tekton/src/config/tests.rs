use super::*;

#[test]
fn test_parse_pipeline_id() {
    let id = "default__my-pipeline";
    let (namespace, pipeline_name) = parse_pipeline_id(id).unwrap();
    assert_eq!(namespace, "default");
    assert_eq!(pipeline_name, "my-pipeline");
}

#[test]
fn test_parse_pipeline_id_with_dashes() {
    let id = "tekton-pipelines__complex-pipeline-name";
    let (namespace, pipeline_name) = parse_pipeline_id(id).unwrap();
    assert_eq!(namespace, "tekton-pipelines");
    assert_eq!(pipeline_name, "complex-pipeline-name");
}

#[test]
fn test_parse_invalid_pipeline_id() {
    let id = "namespace-without-pipeline";
    assert!(parse_pipeline_id(id).is_err());
}

#[test]
fn kube_names_are_lowercase_dns_subdomains_up_to_253_bytes() {
    let longest = "a".repeat(253);
    let valid = ["a", "build-1-clone", "pod.name-1", "0", longest.as_str()];
    for name in valid {
        assert!(is_kube_name(name), "{name}");
    }

    let too_long = "a".repeat(254);
    let invalid = [
        "",
        "UPPER",
        "has space",
        "a/b",
        "../x",
        "..",
        "a%2fb",
        "a:b",
        "a_b",
        "-lead",
        "trail-",
        ".lead",
        "trail.",
        "new\nline",
        "naïve",
        too_long.as_str(),
    ];
    for name in invalid {
        assert!(!is_kube_name(name), "{name:?}");
    }
}

fn config(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn kubeconfig_content_is_parsed_from_config() {
    let source = get_kubeconfig_source(&config(&[("token", "apiVersion: v1\nkind: Config\n")]));

    assert_eq!(
        source,
        KubeconfigSource::Content("apiVersion: v1\nkind: Config\n".to_string())
    );
}

#[test]
fn kubeconfig_content_wins_over_the_path() {
    let source = get_kubeconfig_source(&config(&[
        ("kubeconfig_path", "/home/me/.kube/config"),
        ("token", "apiVersion: v1"),
    ]));

    assert_eq!(
        source,
        KubeconfigSource::Content("apiVersion: v1".to_string())
    );
}

#[test]
fn blank_content_falls_back_to_the_path() {
    let source = get_kubeconfig_source(&config(&[
        ("kubeconfig_path", "/home/me/.kube/config"),
        ("token", "  \n "),
    ]));

    assert_eq!(
        source,
        KubeconfigSource::Path(Some("/home/me/.kube/config".to_string()))
    );
}

#[test]
fn kubeconfig_content_is_never_printed() {
    let source = get_kubeconfig_source(&config(&[("token", "token: s3cr3t-value")]));

    assert!(!format!("{source:?}").contains("s3cr3t-value"));
}
