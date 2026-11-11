use super::*;

const HEX64: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
fn image_ref_splits_registry_tag_and_digest() {
    let digest = format!("sha256:{HEX64}");
    let pinned = format!("Registry.Test:5000/shop/api:v1@{digest}");
    let pullable = format!("docker-pullable://ghcr.io/acme/api@{digest}");
    let cases = [
        ("nginx", Some(("docker.io/library/nginx", None, false))),
        (
            "nginx:1.27",
            Some(("docker.io/library/nginx", Some("1.27"), false)),
        ),
        (
            "acme/api:v2",
            Some(("docker.io/acme/api", Some("v2"), false)),
        ),
        ("localhost/api", Some(("localhost/api", None, false))),
        (
            "registry.test:5000/api",
            Some(("registry.test:5000/api", None, false)),
        ),
        (
            pinned.as_str(),
            Some(("registry.test:5000/shop/api", Some("v1"), true)),
        ),
        (pullable.as_str(), Some(("ghcr.io/acme/api", None, true))),
        (
            "ghcr.io/Acme/Api:Tag",
            Some(("ghcr.io/Acme/Api", Some("Tag"), false)),
        ),
        ("ghcr.io/acme/api@sha256:short", None),
        ("ghcr.io/", None),
        ("has space/api", None),
        ("", None),
    ];
    for (input, expected) in cases {
        let parsed = ImageRef::parse(input);
        let actual = parsed.as_ref().map(|image| {
            (
                image.repository.as_str(),
                image.tag.as_deref(),
                image.digest.is_some(),
            )
        });
        assert_eq!(actual, expected, "{input}");
    }
}

#[test]
fn repository_exposes_host_and_path() {
    let image = ImageRef::parse("ghcr.io/acme/api:v1").unwrap();
    assert_eq!(image.repository.host(), "ghcr.io");
    assert_eq!(image.repository.path(), "acme/api");
    let observation = image.into_observation();
    assert_eq!(observation.tag.as_deref(), Some("v1"));
    assert_eq!(observation.digest, None);
}
