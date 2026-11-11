use super::*;

const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";
const HEX64: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn repo_id_normalizes_every_remote_form() {
    let cases = [
        (
            "git@Git.Example.com:Group/Sub/Repo.git",
            Some("git.example.com/Group/Sub/Repo"),
        ),
        ("https://user:pw@host:443/a/b/c.git/", Some("host/a/b/c")),
        ("http://Host:80/a/b", Some("host/a/b")),
        ("ssh://git@host:22/a/b.git", Some("host/a/b")),
        ("ssh://git@host:2222/a/b.git", Some("host:2222/a/b")),
        ("git://host/a/b?ref=main#frag", Some("host/a/b")),
        ("https://host/a", Some("host/a")),
        ("https://host/a/b.git.git", Some("host/a/b.git")),
        ("https://host/", None),
        ("https://host", None),
        ("https:///a/b", None),
        ("ftp://host/a/b", None),
        ("owner/repo", None),
        ("", None),
    ];
    for (input, expected) in cases {
        let parsed = RepoId::parse(input);
        assert_eq!(parsed.as_ref().map(RepoId::as_str), expected, "{input}");
    }
}

#[test]
fn repo_id_from_host_path_matches_parse() {
    assert_eq!(
        RepoId::from_host_path("Git.Example.com", "/acme/shop.git/"),
        RepoId::parse("https://git.example.com/acme/shop")
    );
    assert_eq!(RepoId::from_host_path("", "acme/shop"), None);
    assert_eq!(RepoId::from_host_path("host", ""), None);
}

#[test]
fn repo_id_exposes_host_and_path() {
    let repo = RepoId::parse("https://host:8443/a/b").unwrap();
    assert_eq!(repo.host(), "host:8443");
    assert_eq!(repo.path(), "a/b");
}

#[test]
fn commit_sha_accepts_only_full_hex() {
    let upper = SHA1.to_uppercase();
    let cases = [
        (SHA1, true),
        (upper.as_str(), true),
        (HEX64, true),
        ("0123456", false),
        ("g123456789abcdef0123456789abcdef01234567", false),
        ("", false),
    ];
    for (input, accepted) in cases {
        assert_eq!(CommitSha::parse(input).is_some(), accepted, "{input}");
    }
    assert_eq!(
        CommitSha::parse(&format!(" {upper}\n")).unwrap().as_str(),
        SHA1
    );
    assert_eq!(CommitSha::parse(SHA1).unwrap().short(), "0123456");
}

#[test]
fn digest_accepts_sha256_and_image_ids() {
    let digest = format!("sha256:{HEX64}");
    assert_eq!(
        Digest::parse(&digest.to_uppercase()).unwrap().as_str(),
        digest
    );
    assert_eq!(Digest::parse("sha256:abc"), None);
    assert_eq!(Digest::parse(HEX64), None);
    let pullable = format!("docker-pullable://registry.test/shop/api@{digest}");
    assert_eq!(Digest::from_image_id(&pullable).unwrap().as_str(), digest);
    assert_eq!(Digest::from_image_id(&digest).unwrap().as_str(), digest);
    assert_eq!(Digest::from_image_id("registry.test/shop/api:v1"), None);
}

#[test]
fn cluster_id_keeps_its_origin() {
    assert_eq!(ClusterId::from_uid("ABC-1").unwrap().as_str(), "uid:abc-1");
    assert_eq!(ClusterId::from_uid(" "), None);
    let servers = [
        (
            "https://K8s.Example.com:443/",
            Some("server:k8s.example.com"),
        ),
        (
            "https://k8s.example.com:6443",
            Some("server:k8s.example.com:6443"),
        ),
        ("https://10.0.0.1", Some("server:10.0.0.1")),
        ("https://kubernetes.default.svc", None),
        ("https://kubernetes.default.svc:443/", None),
        ("k8s.example.com", None),
    ];
    for (input, expected) in servers {
        let parsed = ClusterId::from_server(input);
        assert_eq!(parsed.as_ref().map(ClusterId::as_str), expected, "{input}");
    }
    assert_eq!(
        ClusterId::from_host("Prod-East").unwrap().as_str(),
        "host:prod-east"
    );
    assert_eq!(ClusterId::from_host(""), None);
}

#[test]
fn unit_id_requires_content() {
    assert_eq!(UnitId::new(" apps/api ").unwrap().as_str(), "apps/api");
    assert_eq!(UnitId::new("  "), None);
}
