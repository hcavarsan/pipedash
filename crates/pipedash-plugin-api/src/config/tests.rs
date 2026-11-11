use super::*;

fn config(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn value_trims_and_rejects_blank() {
    let c = config(&[("a", "  x "), ("b", "   ")]);
    assert_eq!(value(&c, "a"), Some("x"));
    assert_eq!(value(&c, "b"), None);
    assert_eq!(value(&c, "c"), None);
    assert!(required_value(&c, "b", "missing b").is_err());
}

#[test]
fn comma_separated_drops_empty_and_keeps_duplicates() {
    let items: Vec<_> = comma_separated(" a, ,b,a ,").collect();
    assert_eq!(items, ["a", "b", "a"]);
}

#[test]
fn bool_value_is_strict() {
    let c = config(&[("t", "true"), ("u", "TRUE")]);
    assert!(bool_value(&c, "t"));
    assert!(!bool_value(&c, "u"));
    assert!(!bool_value(&c, "missing"));
}

#[test]
fn credential_and_selected_items() {
    let c = config(&[("token", " t "), ("selected_items", " a, ,b ")]);
    assert_eq!(credential(&c, "token").unwrap(), "t");
    assert_eq!(
        credential(&c, "key").unwrap_err().to_string(),
        PluginError::InvalidConfig("Missing key in config".to_string()).to_string()
    );
    assert_eq!(
        selected_items(&c),
        Some(vec!["a".to_string(), "b".to_string()])
    );
    assert_eq!(selected_items(&config(&[])), None);
    assert_eq!(
        selected_items(&config(&[("selected_items", "")])),
        Some(Vec::new())
    );
}

#[test]
fn insecure_is_true_only_for_the_word_true() {
    assert!(insecure(&config(&[("insecure", "true")])));
    assert!(!insecure(&config(&[("insecure", "yes")])));
    assert!(!insecure(&config(&[])));
}

#[test]
fn base_url_trims_trailing_slashes_and_rejects_blank() {
    let c = config(&[("a", " https://x.dev/// "), ("b", " / ")]);
    assert_eq!(base_url(&c, "a"), Some("https://x.dev"));
    assert_eq!(base_url(&c, "b"), None);
    assert_eq!(base_url(&c, "c"), None);
    assert!(required_base_url(&c, "b", "missing").is_err());
}

#[test]
fn http_base_url_validates_scheme_query_and_fragment() {
    let c = config(&[("u", "https://svc.dev/")]);
    let (raw, parsed) = http_base_url(&c, "u", "missing", "Svc").unwrap();
    assert_eq!(
        (raw, parsed.host_str()),
        ("https://svc.dev", Some("svc.dev"))
    );
    for bad in [
        "",
        "svc.dev",
        "ftp://svc.dev",
        "https://svc.dev?x=1",
        "https://svc.dev#f",
    ] {
        let c = config(&[("u", bad)]);
        let result = http_base_url(&c, "u", "missing", "Svc");
        assert!(
            matches!(result, Err(PluginError::InvalidConfig(_))),
            "{bad}"
        );
    }
}
