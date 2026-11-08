use std::collections::HashMap;

use super::*;

fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();

    move |name| map.get(name).cloned()
}

const REQUIRED: [(&str, &str); 4] = [
    ("RELAY_APNS_KEY", "pem"),
    ("RELAY_APNS_KEY_ID", "KEYID"),
    ("RELAY_APNS_TEAM_ID", "TEAMID"),
    ("RELAY_ALLOWED_BUNDLE_IDS", "com.a.app, com.b.app,,"),
];

#[test]
fn defaults_apply_when_only_the_required_variables_are_set() {
    let config = Config::from_vars(vars(&REQUIRED)).unwrap();

    assert_eq!(config.bind_addr, "0.0.0.0:8080".parse().unwrap());
    assert_eq!(config.database_path, PathBuf::from(DEFAULT_DATABASE_PATH));
    assert_eq!(config.apns_key, "pem");
    assert_eq!(config.apns_key_id, "KEYID");
    assert_eq!(config.apns_team_id, "TEAMID");
    assert_eq!(
        config.allowed_bundle_ids,
        HashSet::from(["com.a.app".to_string(), "com.b.app".to_string()])
    );
    assert!(!config.trusted_proxy);
}

#[test]
fn the_key_can_come_from_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("AuthKey.p8");
    std::fs::write(&path, "from-file").unwrap();
    let path = path.to_str().unwrap();
    let mut pairs: Vec<(&str, &str)> = REQUIRED[1..].to_vec();
    pairs.push(("RELAY_APNS_KEY_PATH", path));
    pairs.push(("RELAY_BIND_ADDR", "127.0.0.1:9000"));
    pairs.push(("RELAY_DATABASE_PATH", "/data/relay.db"));
    pairs.push(("RELAY_TRUSTED_PROXY", "true"));

    let config = Config::from_vars(vars(&pairs)).unwrap();

    assert_eq!(config.apns_key, "from-file");
    assert_eq!(config.bind_addr, "127.0.0.1:9000".parse().unwrap());
    assert_eq!(config.database_path, PathBuf::from("/data/relay.db"));
    assert!(config.trusted_proxy);
}

#[test]
fn missing_or_empty_settings_are_reported_by_name() {
    for missing in ["RELAY_APNS_KEY", "RELAY_APNS_KEY_ID", "RELAY_APNS_TEAM_ID"] {
        let pairs: Vec<_> = REQUIRED
            .iter()
            .copied()
            .filter(|(name, _)| *name != missing)
            .collect();
        let error = Config::from_vars(vars(&pairs)).err().unwrap();

        assert!(
            format!("{error:#}").contains(missing),
            "{missing}: {error:#}"
        );
    }

    let mut pairs = REQUIRED.to_vec();
    pairs[3] = ("RELAY_ALLOWED_BUNDLE_IDS", " , ");
    let error = Config::from_vars(vars(&pairs)).err().unwrap();
    assert!(format!("{error:#}").contains("RELAY_ALLOWED_BUNDLE_IDS"));

    pairs[3] = ("RELAY_ALLOWED_BUNDLE_IDS", "com.a.app");
    pairs.push(("RELAY_BIND_ADDR", "nope"));
    let error = Config::from_vars(vars(&pairs)).err().unwrap();
    assert!(format!("{error:#}").contains("RELAY_BIND_ADDR"));
}
