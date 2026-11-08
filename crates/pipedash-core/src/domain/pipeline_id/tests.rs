use super::*;

#[test]
fn compose_prefixes_the_local_id_with_type_and_provider_id() {
    assert_eq!(compose("svc", 7, "a__b"), "svc__7__a__b");
    assert_eq!(compose("svc", 7, "a/b/c"), "svc__7__a/b/c");
}

#[test]
fn strip_inverts_compose() {
    for local in ["a", "a__b", "a/b/c.yml", "x__y__z", "7"] {
        let id = compose("svc", 7, local);
        assert_eq!(strip("svc", 7, &id).unwrap(), local);
    }
}

#[test]
fn strip_rejects_other_types_providers_and_malformed_ids() {
    for bad in [
        "",
        "svc",
        "svc__7",
        "svc__7__",
        "other__7__a",
        "svc__8__a",
        "svc__07__a",
        "svc__x__a",
        "svc7__a",
        "a__svc__7__b",
        "svcx__7__a",
    ] {
        let error = strip("svc", 7, bad).unwrap_err();
        assert!(matches!(error, DomainError::InvalidConfig(_)), "{bad}");
    }
}

#[test]
fn strip_error_names_the_expected_prefix() {
    let error = strip("svc", 7, "other__7__a").unwrap_err().to_string();
    assert!(error.contains("'other__7__a'"), "{error}");
    assert!(error.contains("'svc__7__<id>'"), "{error}");
}
