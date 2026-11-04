use std::net::{
    IpAddr,
    Ipv4Addr,
    Ipv6Addr,
};

use chrono::{
    DateTime,
    Utc,
};
use sha2::{
    Digest,
    Sha256,
};

const CODE_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const CODE_LEN: usize = 8;
const MAX_DEVICE_NAME_CHARS: usize = 100;
const PRIVATE_SUFFIXES: [&str; 4] = [".local", ".lan", ".home.arpa", ".internal"];

pub const DEVICE_PLATFORMS: [&str; 5] = ["macos", "windows", "linux", "ios", "android"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerUrl {
    pub url: String,
    pub scheme: String,
    pub host: String,
}

impl ServerUrl {
    pub fn parse(input: &str) -> Result<Self, &'static str> {
        let parsed = reqwest::Url::parse(input.trim()).map_err(|_| "not a valid URL")?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("must start with http:// or https://");
        }
        let host = parsed
            .host_str()
            .filter(|host| !host.is_empty())
            .ok_or("missing host")?;
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("must not contain credentials");
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err("must not contain a query or fragment");
        }
        let port = parsed
            .port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default();
        let path = parsed.path().trim_end_matches('/');
        Ok(Self {
            url: format!("{}://{host}{port}{path}", parsed.scheme()),
            scheme: parsed.scheme().to_owned(),
            host: host.to_owned(),
        })
    }

    fn bare_host(&self) -> &str {
        self.host.trim_start_matches('[').trim_end_matches(']')
    }

    pub fn is_loopback(&self) -> bool {
        let host = self.bare_host();
        host == "localhost" || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
    }

    pub fn is_private(&self) -> bool {
        let host = self.bare_host();
        match host.parse::<IpAddr>() {
            Ok(ip) => is_private_ip(ip),
            Err(_) => {
                !host.contains('.') || PRIVATE_SUFFIXES.iter().any(|suffix| host.ends_with(suffix))
            }
        }
    }

    pub fn websocket_url(&self) -> String {
        let scheme = if self.scheme == "https" { "wss" } else { "ws" };
        let rest = self.url.split_once("://").map_or("", |(_, rest)| rest);
        format!("{scheme}://{rest}/api/v1/ws")
    }
}

pub fn is_private_ip(ip: IpAddr) -> bool {
    if ip.is_loopback() {
        return true;
    }

    match ip {
        IpAddr::V4(ip) => {
            let [first, second, ..] = ip.octets();
            ip.is_private() || ip.is_link_local() || (first == 100 && second & 0xc0 == 64)
        }
        IpAddr::V6(ip) => {
            if let Some(embedded) = ip.to_ipv4_mapped().or_else(|| nat64_embedded(ip)) {
                return is_private_ip(IpAddr::V4(embedded));
            }
            let first = ip.segments()[0];
            first & 0xfe00 == 0xfc00 || first & 0xffc0 == 0xfe80
        }
    }
}

fn nat64_embedded(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let [a, b, c, d, e, f, g, h] = ip.segments();
    ([a, b, c, d, e, f] == [0x64, 0xff9b, 0, 0, 0, 0]).then(|| {
        let [hi_a, hi_b] = g.to_be_bytes();
        let [lo_a, lo_b] = h.to_be_bytes();
        Ipv4Addr::new(hi_a, hi_b, lo_a, lo_b)
    })
}

pub fn generate_pairing_code() -> String {
    let bits = rand::random::<u64>();
    (0..CODE_LEN)
        .map(|i| CODE_ALPHABET[((bits >> (i * 5)) & 31) as usize] as char)
        .collect()
}

pub fn normalize_pairing_code(input: &str) -> Option<String> {
    let code: String = input
        .chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect();

    (code.len() == CODE_LEN && code.bytes().all(|b| CODE_ALPHABET.contains(&b))).then_some(code)
}

pub fn display_pairing_code(code: &str) -> String {
    match code.split_at_checked(CODE_LEN / 2) {
        Some((head, tail)) => format!("{head}-{tail}"),
        None => code.to_string(),
    }
}

pub fn pairing_link(server_url: &str, code: &str) -> String {
    reqwest::Url::parse_with_params("pipedash://pair", [("url", server_url), ("code", code)])
        .map_or_else(|_| String::new(), String::from)
}

pub fn parse_pairing_link(input: &str) -> Option<(String, String)> {
    let parsed = reqwest::Url::parse(input.trim()).ok()?;
    if parsed.scheme() != "pipedash" || parsed.host_str() != Some("pair") {
        return None;
    }
    let mut url = None;
    let mut code = None;
    for (key, value) in parsed.query_pairs() {
        match &*key {
            "url" => url = Some(value.into_owned()),
            "code" => code = Some(value.into_owned()),
            _ => {}
        }
    }

    Some((url?, code?))
}

pub fn validate_device(name: &str, platform: &str) -> Result<String, (&'static str, &'static str)> {
    let name = name.trim();
    if !(1..=MAX_DEVICE_NAME_CHARS).contains(&name.chars().count()) {
        return Err((
            "INVALID_DEVICE_NAME",
            "Device name must be 1 to 100 characters.",
        ));
    }
    if !DEVICE_PLATFORMS.contains(&platform) {
        return Err(("INVALID_PLATFORM", "Unknown device platform."));
    }

    Ok(name.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairedDevice {
    pub id: String,
    pub token_hash: String,
    pub name: String,
    pub platform: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

impl PairedDevice {
    pub fn issue(name: &str, platform: &str) -> (Self, String) {
        let token = format!(
            "{:032x}{:032x}",
            rand::random::<u128>(),
            rand::random::<u128>()
        );
        let now = Utc::now();
        let device = Self {
            id: format!("{:032x}", rand::random::<u128>()),
            token_hash: Self::hash_token(&token),
            name: name.to_string(),
            platform: platform.to_string(),
            created_at: now,
            last_seen_at: Some(now),
        };

        (device, token)
    }

    pub fn hash_token(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());

        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
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
}
