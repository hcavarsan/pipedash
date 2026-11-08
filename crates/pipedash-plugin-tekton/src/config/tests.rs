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
