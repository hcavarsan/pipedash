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
mod tests;
