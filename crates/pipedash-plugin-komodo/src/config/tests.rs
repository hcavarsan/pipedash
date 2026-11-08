use super::*;

fn cfg(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn server_url_drops_whitespace_and_trailing_slashes() {
    for raw in [
        "https://komodo.example.com",
        " https://komodo.example.com/ ",
        "https://komodo.example.com//",
    ] {
        assert_eq!(
            get_server_url(&cfg(&[("server_url", raw)])).unwrap(),
            "https://komodo.example.com"
        );
    }
    assert!(get_server_url(&cfg(&[("server_url", " / ")])).is_err());
    assert!(get_server_url(&cfg(&[])).is_err());
}
