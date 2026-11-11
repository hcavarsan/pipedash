use super::*;

const HEX64: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn scan(template: &str) -> Option<(Option<String>, String)> {
    let line = template.replace("<d>", &format!("sha256:{HEX64}"));
    pushed_digests(&line).map(|(repository, digest)| {
        (
            repository.map(|r| r.as_str().to_string()),
            digest.as_str().to_string(),
        )
    })
}

fn pushed(repository: Option<&str>) -> Option<(Option<String>, String)> {
    Some((repository.map(str::to_string), format!("sha256:{HEX64}")))
}

#[test]
fn push_lines_from_each_builder_yield_the_digest() {
    let cases = [
        ("v1.2.3: digest: <d> size: 1234", None),
        (
            "#12 pushing manifest for registry.test/shop/api:v1@<d> 0.3s done",
            Some("registry.test/shop/api"),
        ),
        (
            "INFO[0042] Pushed registry.test/shop/api@<d>",
            Some("registry.test/shop/api"),
        ),
        ("Pushed image to 1 destinations digest: <d>", None),
        (
            "2026/01/01 10:00:00 Published ghcr.io/acme/api@<d>",
            Some("ghcr.io/acme/api"),
        ),
        ("digest: <d>", None),
    ];
    for (line, repository) in cases {
        assert_eq!(scan(line), pushed(repository), "{line}");
    }
}

#[test]
fn pull_and_base_image_lines_are_ignored() {
    let cases = [
        "Digest: <d>",
        "latest: Pulling from library/node",
        "Status: Downloaded newer image for node@<d>",
        "Status: Image is up to date for node@<d>",
        "Step 1/9 : FROM node@<d>",
        "#5 resolve docker.io/library/node@<d> done",
        "abc123: Pull complete <d>",
        "Writing manifest to image destination",
        "Built and pushed image as registry.test/shop/api",
        "image registry.test/shop/api@<d> is present",
        "digest: sha256:abc size: 1",
    ];
    for line in cases {
        assert_eq!(scan(line), None, "{line}");
    }
}
