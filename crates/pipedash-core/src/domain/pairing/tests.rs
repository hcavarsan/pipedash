use super::*;

fn normalized(input: &str) -> String {
    ServerUrl::parse(input).unwrap().url
}

#[test]
fn normalizes_accepted_urls() {
    assert_eq!(
        normalized("  HTTPS://CI.Example.COM/  "),
        "https://ci.example.com"
    );
    assert_eq!(normalized("http://10.0.0.5:8080"), "http://10.0.0.5:8080");
    assert_eq!(
        normalized("https://host/Base/path//"),
        "https://host/Base/path"
    );
    assert_eq!(normalized("https://host:443/x/"), "https://host/x");
    assert_eq!(normalized("http://[::1]:3000/"), "http://[::1]:3000");
}

#[test]
fn reports_the_host_and_scheme() {
    let n = ServerUrl::parse("https://User.example.com:8443/a").unwrap();
    assert_eq!(n.host, "user.example.com");
    assert_eq!(n.scheme, "https");
}

#[test]
fn rejects_unusable_urls() {
    for input in [
        "",
        "   ",
        "ci.example.com",
        "ftp://ci.example.com",
        "mailto:a@b.c",
        "https://",
        "https://user@ci.example.com",
        "https://user:pw@ci.example.com",
        "https://ci.example.com?a=1",
        "https://ci.example.com/#frag",
        "https://ci.example.com/?",
    ] {
        assert!(
            ServerUrl::parse(input).is_err(),
            "{input:?} should be rejected"
        );
    }
}

#[test]
fn classifies_private_and_loopback_hosts() {
    for (input, private, loopback) in [
        ("http://localhost:8080", true, true),
        ("http://127.0.0.1", true, true),
        ("http://[::1]:3000", true, true),
        ("http://192.168.1.20", true, false),
        ("http://10.1.2.3", true, false),
        ("http://172.16.0.1", true, false),
        ("http://172.32.0.1", false, false),
        ("http://169.254.1.1", true, false),
        ("http://100.64.0.1", true, false),
        ("http://100.128.0.1", false, false),
        ("http://[fd00::1]", true, false),
        ("http://[fe80::1]", true, false),
        ("http://[2001:db8::1]", false, false),
        ("http://mac.local", true, false),
        ("http://nas", true, false),
        ("http://pc.lan", true, false),
        ("http://router.home.arpa", true, false),
        ("http://ci.internal", true, false),
        ("http://example.com", false, false),
        ("http://8.8.8.8", false, false),
    ] {
        let url = ServerUrl::parse(input).unwrap();
        assert_eq!(url.is_private(), private, "{input} private");
        assert_eq!(url.is_loopback(), loopback, "{input} loopback");
    }
}

#[test]
fn private_addresses_exclude_every_public_range() {
    for (input, private) in [
        ("127.0.0.1", true),
        ("::1", true),
        ("10.0.0.1", true),
        ("172.31.255.255", true),
        ("192.168.0.1", true),
        ("169.254.0.1", true),
        ("100.127.255.255", true),
        ("fc00::1", true),
        ("fe80::1", true),
        ("172.32.0.1", false),
        ("100.128.0.1", false),
        ("8.8.8.8", false),
        ("0.0.0.0", false),
        ("::", false),
        ("2001:4860:4860::8888", false),
        ("::ffff:8.8.8.8", false),
        ("::ffff:192.168.1.10", true),
        ("::ffff:127.0.0.1", true),
        ("64:ff9b::c0a8:10a", true),
        ("64:ff9b::a00:1", true),
        ("64:ff9b::808:808", false),
        ("64:ff9b:1::c0a8:10a", false),
    ] {
        let ip: IpAddr = input.parse().unwrap();
        assert_eq!(is_private_ip(ip), private, "{input}");
    }
}

#[test]
fn websocket_url_follows_the_scheme_and_base_path() {
    for (input, expected) in [
        ("http://10.0.0.5:8080", "ws://10.0.0.5:8080/api/v1/ws"),
        (
            "https://ci.example.com/pipedash/",
            "wss://ci.example.com/pipedash/api/v1/ws",
        ),
    ] {
        assert_eq!(ServerUrl::parse(input).unwrap().websocket_url(), expected);
    }
}

#[test]
fn pairing_links_round_trip() {
    let link = pairing_link("https://ci.example.com/base", "ABCDEFGH");

    assert!(link.starts_with("pipedash://pair?"));
    assert_eq!(
        parse_pairing_link(&link),
        Some(("https://ci.example.com/base".into(), "ABCDEFGH".into()))
    );
    for input in [
        "https://pair?url=a&code=b",
        "pipedash://other?url=a&code=b",
        "pipedash://pair?url=a",
        "pipedash://pair?code=b",
        "not a link",
    ] {
        assert_eq!(parse_pairing_link(input), None, "{input}");
    }
}

#[test]
fn pairing_codes_normalize_and_display() {
    assert_eq!(
        normalize_pairing_code("abcd-efgh").as_deref(),
        Some("ABCDEFGH")
    );
    assert_eq!(
        normalize_pairing_code(" ab cd-ef gh ").as_deref(),
        Some("ABCDEFGH")
    );
    for input in ["ABCD-EFGI", "ABCDEFG", "ABCDEFGHJ", "", "ÄBCDEFGH"] {
        assert_eq!(normalize_pairing_code(input), None, "{input}");
    }
    assert_eq!(display_pairing_code("ABCDEFGH"), "ABCD-EFGH");

    let generated = generate_pairing_code();
    assert_eq!(normalize_pairing_code(&generated), Some(generated));
}

#[test]
fn device_input_is_validated() {
    assert_eq!(validate_device("  Pixel  ", "android"), Ok("Pixel".into()));
    assert_eq!(
        validate_device("   ", "android"),
        Err((
            "INVALID_DEVICE_NAME",
            "Device name must be 1 to 100 characters."
        ))
    );
    assert_eq!(
        validate_device(&"x".repeat(101), "android").map_err(|e| e.0),
        Err("INVALID_DEVICE_NAME")
    );
    assert_eq!(
        validate_device("Pixel", "palm"),
        Err(("INVALID_PLATFORM", "Unknown device platform."))
    );
}

#[test]
fn issued_devices_store_only_the_token_hash() {
    let (device, token) = PairedDevice::issue("Pixel", "android");

    assert_eq!(device.id.len(), 32);
    assert_eq!(token.len(), 64);
    assert_eq!(device.token_hash, PairedDevice::hash_token(&token));
    assert_eq!(device.token_hash.len(), 64);
    assert_ne!(device.token_hash, token);
}
