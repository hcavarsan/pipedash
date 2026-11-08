use super::*;

fn config(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn selection_tokens_round_trip_github_gerrit_and_refless_jobs() {
    let github = Selection::parse("kubernetes/test-infra/ci-x").unwrap();
    assert_eq!(
        github.scope,
        Some(Scope {
            org: "kubernetes".into(),
            repo: "test-infra".into()
        })
    );
    assert_eq!(github.job, "ci-x");

    let gerrit = Selection::parse("https://gerrit.example.com/platform/build/pull-build").unwrap();
    let scope = gerrit.scope.clone().unwrap();
    assert_eq!(scope.org, "https://gerrit.example.com");
    assert_eq!(scope.repo, "platform/build");
    assert_eq!(scope.repository(), "gerrit.example.com/platform/build");
    assert_eq!(gerrit.job, "pull-build");

    let refless = Selection::parse("ci-noref").unwrap();
    assert_eq!(refless.scope, None);

    for selection in [github, gerrit, refless] {
        let id = selection.token();
        assert_eq!(parse_pipeline_id(&id).unwrap(), selection);
        assert_eq!(Selection::parse(&selection.token()).unwrap(), selection);
    }

    assert!(Selection::parse("a/b").is_err());
    assert!(Selection::parse("").is_err());
    assert!(parse_pipeline_id("a/b").is_err());
    assert!(parse_pipeline_id("").is_err());
}

#[test]
fn auth_header_supports_bearer_basic_and_cookie() {
    let (name, value) = auth_header(&config(&[("token", "abc")])).unwrap().unwrap();
    assert_eq!(name, AUTHORIZATION);
    assert_eq!(value, "Bearer abc");
    assert!(value.is_sensitive());

    let (_, value) = auth_header(&config(&[("token", "user:pass"), ("auth_scheme", "basic")]))
        .unwrap()
        .unwrap();
    assert_eq!(value, "Basic dXNlcjpwYXNz");

    assert!(auth_header(&config(&[("token", "nopass"), ("auth_scheme", "basic")])).is_err());

    let (name, value) = auth_header(&config(&[
        ("token", "_oauth2_proxy=xyz"),
        ("auth_scheme", "cookie"),
    ]))
    .unwrap()
    .unwrap();
    assert_eq!(name, COOKIE);
    assert_eq!(value, "_oauth2_proxy=xyz");

    assert!(auth_header(&config(&[("token", "  ")])).unwrap().is_none());
    assert!(auth_header(&config(&[("token", "a"), ("auth_scheme", "digest")])).is_err());
}
