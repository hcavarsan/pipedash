use std::io::{
    Read,
    Write,
};
use std::net::{
    TcpListener,
    TcpStream,
};
use std::sync::{
    Arc,
    Mutex,
};

use reqwest::header::{
    HeaderMap,
    HeaderValue,
    WWW_AUTHENTICATE,
};
use serde_json::json;

use super::auth::{
    may_receive_secret,
    parse_challenge,
    realm_url,
    Challenge,
};
use super::authority::registry_authority;
use super::provenance::source_url;
use super::*;

const SOURCE_LABEL: &str = "org.opencontainers.image.source";
const REVISION_LABEL: &str = "org.opencontainers.image.revision";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

struct Request {
    path: String,
    authorization: Option<String>,
}

impl Request {
    fn auth_is(&self, expected: &str) -> bool {
        self.authorization.as_deref() == Some(expected)
    }
}

type Log = Arc<Mutex<Vec<(String, Option<String>)>>>;

fn client() -> OciClient {
    let _ = rustls::crypto::ring::default_provider().install_default();
    OciClient::new(reqwest::Client::new())
}

fn digest_of(body: &[u8]) -> Digest {
    verified_digest(body, None, None).unwrap()
}

fn fake_digest(byte: u8) -> Digest {
    Digest::parse(&format!("sha256:{}", format!("{byte:02x}").repeat(32))).unwrap()
}

fn read_request(stream: &mut TcpStream) -> Request {
    let mut data = Vec::new();
    let mut buf = [0u8; 1024];
    while !data.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut buf).unwrap_or(0);
        if read == 0 {
            break;
        }
        data.extend_from_slice(&buf[..read]);
    }
    let text = String::from_utf8_lossy(&data);
    let mut lines = text.lines();
    let path = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default()
        .to_string();
    let authorization = lines.find_map(|line| {
        let (name, value) = line.split_once(": ")?;
        name.eq_ignore_ascii_case("authorization")
            .then(|| value.to_string())
    });
    Request {
        path,
        authorization,
    }
}

fn reply(status: &str, headers: &[(&str, String)], body: &str) -> String {
    let extra: String = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    format!(
        "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn serve(handler: impl Fn(&Request, u16) -> String + Send + 'static) -> (String, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let log: Log = Arc::default();
    let seen = Arc::clone(&log);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let request = read_request(&mut stream);
            seen.lock()
                .unwrap()
                .push((request.path.clone(), request.authorization.clone()));
            let _ = stream.write_all(handler(&request, port).as_bytes());
        }
    });
    (format!("127.0.0.1:{port}"), log)
}

fn challenge(port: u16, scope: &str) -> Vec<(&'static str, String)> {
    vec![(
        "WWW-Authenticate",
        format!(r#"Bearer realm="http://127.0.0.1:{port}/token",service="mock",scope="{scope}""#),
    )]
}

fn basic(user: &str, secret: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{secret}"))
    )
}

fn repository(host: &str, path: &str) -> ArtifactRepository {
    super::super::image::ImageRef::parse(&format!("{host}/{path}"))
        .unwrap()
        .repository
}

fn credentials() -> RegistryAuth {
    RegistryAuth {
        username: Some("bot".into()),
        secret: "pw".into(),
        bearer: false,
        token_hosts: Vec::new(),
    }
}

fn descriptor(digest: Digest, platform: Option<(&str, &str)>) -> Descriptor {
    Descriptor {
        media_type: "application/vnd.oci.image.manifest.v1+json".into(),
        digest,
        size: 1,
        artifact_type: None,
        annotations: HashMap::new(),
        platform: platform.map(|(os, architecture)| Platform {
            os: os.into(),
            architecture: architecture.into(),
        }),
    }
}

fn headers(values: &[&str]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for value in values {
        map.append(WWW_AUTHENTICATE, HeaderValue::from_str(value).unwrap());
    }
    map
}

#[test]
fn token_challenges_are_parsed_with_quoted_commas() {
    let parsed = parse_challenge(&headers(&[
        r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/alpine:pull,push""#,
    ]));
    assert_eq!(
        parsed,
        Some(Challenge::Bearer {
            realm: "https://auth.docker.io/token".into(),
            service: Some("registry.docker.io".into()),
            scope: Some("repository:library/alpine:pull,push".into()),
        })
    );

    let lax = parse_challenge(&headers(&[
        "bearer realm=https://auth.test/token, service=reg.test",
    ]));
    assert_eq!(
        lax,
        Some(Challenge::Bearer {
            realm: "https://auth.test/token".into(),
            service: Some("reg.test".into()),
            scope: None,
        })
    );
}

#[test]
fn token_challenges_reject_unusable_headers_and_prefer_bearer() {
    assert_eq!(parse_challenge(&headers(&[])), None);
    assert_eq!(parse_challenge(&headers(&["Negotiate abc"])), None);
    assert_eq!(parse_challenge(&headers(&[r#"Bearer service="x""#])), None);
    assert_eq!(
        parse_challenge(&headers(&[r#"Basic realm="Registry""#])),
        Some(Challenge::Basic)
    );
    let both = parse_challenge(&headers(&[
        r#"Basic realm="Registry""#,
        r#"Bearer realm="https://auth.test/token""#,
    ]));
    assert!(matches!(both, Some(Challenge::Bearer { .. })));
}

#[test]
fn token_realms_must_be_https_or_loopback() {
    let url = realm_url(
        "https://auth.test/token?x=1",
        Some("svc".into()),
        Some("s".into()),
    )
    .unwrap();
    assert_eq!(
        url.as_str(),
        "https://auth.test/token?x=1&service=svc&scope=s"
    );
    assert!(realm_url("http://127.0.0.1:5000/token", None, None).is_ok());
    assert!(realm_url("http://auth.test/token", None, None).is_err());
    assert!(realm_url("https://user:pw@auth.test/token", None, None).is_err());
}

#[test]
fn index_resolution_prefers_linux_amd64_then_the_first_entry() {
    let arm = descriptor(fake_digest(1), Some(("linux", "arm64")));
    let amd = descriptor(fake_digest(2), Some(("linux", "amd64")));
    let bare = descriptor(fake_digest(3), None);
    assert_eq!(
        resolve_index(&[arm.clone(), amd.clone()]).map(|entry| &entry.digest),
        Some(&amd.digest)
    );
    assert_eq!(
        resolve_index(&[bare.clone(), arm]).map(|entry| &entry.digest),
        Some(&bare.digest)
    );
    assert!(resolve_index(&[]).is_none());
}

#[test]
fn registry_urls_map_docker_hub_and_reject_unsafe_input() {
    assert_eq!(
        registry_base("docker.io").unwrap(),
        "https://registry-1.docker.io"
    );
    assert_eq!(registry_base("GHCR.io").unwrap(), "https://ghcr.io");
    assert_eq!(
        registry_base("localhost:5000").unwrap(),
        "http://localhost:5000"
    );
    assert!(registry_base("evil@host").is_err());
    assert!(registry_base("host/path").is_err());

    let hub = repository("docker.io", "library/alpine");
    let url = registry_url(&hub, "manifests", "latest").unwrap();
    assert_eq!(
        url.as_str(),
        "https://registry-1.docker.io/v2/library/alpine/manifests/latest"
    );
    assert!(registry_url(&hub, "manifests", "a/b").is_err());
    assert!(registry_url(&hub, "manifests", "").is_err());
}

#[tokio::test]
async fn manifest_and_labels_are_read_through_the_token_flow() {
    let config = json!({"config": {"Labels": {SOURCE_LABEL: "https://git.test/acme/shop", REVISION_LABEL: COMMIT}}})
        .to_string();
    let config_digest = digest_of(config.as_bytes());
    let image = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {"mediaType": "application/vnd.oci.image.config.v1+json", "digest": config_digest, "size": config.len()},
        "layers": [{"mediaType": "application/vnd.oci.image.layer.v1.tar+gzip", "digest": fake_digest(9), "size": 5}]
    })
    .to_string();
    let image_digest = digest_of(image.as_bytes());
    let index = json!({
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.index.v1+json",
        "manifests": [
            {"mediaType": "application/vnd.oci.image.manifest.v1+json", "digest": fake_digest(7), "size": 1, "platform": {"os": "linux", "architecture": "arm64"}},
            {"mediaType": "application/vnd.oci.image.manifest.v1+json", "digest": image_digest, "size": image.len(), "platform": {"os": "linux", "architecture": "amd64"}}
        ]
    })
    .to_string();
    let index_digest = digest_of(index.as_bytes());

    let (host, log) = {
        let (image_digest, config_digest) = (image_digest.clone(), config_digest.clone());
        serve(move |request, port| {
            let path = request.path.split('?').next().unwrap_or_default();
            if path == "/token" {
                let valid = request.auth_is(&basic("bot", "pw"));
                let query = request.path.contains("service=mock")
                    && request
                        .path
                        .contains("scope=repository%3Ashop%2Fapi%3Apull");
                if valid && query {
                    return reply("200 OK", &[], r#"{"token":"t0k"}"#);
                }
                return reply("401 Unauthorized", &[], "denied");
            }
            if request.authorization.as_deref() != Some("Bearer t0k") {
                return reply(
                    "401 Unauthorized",
                    &challenge(port, "repository:shop/api:pull"),
                    "",
                );
            }
            let (body, digest) = if path == "/v2/shop/api/manifests/latest" {
                (&index, &index_digest)
            } else if path == format!("/v2/shop/api/manifests/{}", image_digest.as_str()) {
                (&image, &image_digest)
            } else if path == format!("/v2/shop/api/blobs/{}", config_digest.as_str()) {
                (&config, &config_digest)
            } else {
                return reply("404 Not Found", &[], "");
            };
            reply(
                "200 OK",
                &[("Docker-Content-Digest", digest.as_str().to_string())],
                body,
            )
        })
    };
    let repository = repository(&host, "shop/api");
    let client = client();

    let (digest, manifest) = client
        .manifest(&repository, "latest", Some(&credentials()))
        .await
        .unwrap();
    assert_eq!(digest, image_digest);
    assert_eq!(manifest.config.digest, config_digest);
    assert_eq!(manifest.layers.len(), 1);

    let labels = client
        .config_labels(&repository, &manifest, Some(&credentials()))
        .await
        .unwrap();
    assert_eq!(labels[REVISION_LABEL], COMMIT);
    assert_eq!(labels[SOURCE_LABEL], "https://git.test/acme/shop");

    let seen = log.lock().unwrap();
    assert!(seen
        .iter()
        .filter(|(path, _)| path.starts_with("/v2/"))
        .all(|(_, authorization)| authorization.as_deref() != Some(basic("bot", "pw").as_str())));
    assert!(seen.iter().any(|(path, _)| path.starts_with("/token")));
}

#[tokio::test]
async fn a_manifest_that_does_not_match_its_digest_is_rejected() {
    let body =
        json!({"config": {"mediaType": "x", "digest": fake_digest(1), "size": 1}}).to_string();
    let (host, _) = serve(move |_, _| {
        reply(
            "200 OK",
            &[("Docker-Content-Digest", fake_digest(5).as_str().to_string())],
            &body,
        )
    });
    let error = client()
        .manifest(&repository(&host, "shop/api"), "latest", None)
        .await
        .unwrap_err();
    assert!(matches!(error, PluginError::ApiError(_)), "{error:?}");
}

#[tokio::test]
async fn missing_digest_header_falls_back_to_the_body_hash() {
    let body =
        json!({"config": {"mediaType": "x", "digest": fake_digest(1), "size": 1}}).to_string();
    let expected = digest_of(body.as_bytes());
    let (host, _) = serve(move |_, _| reply("200 OK", &[], &body));
    let (digest, _) = client()
        .manifest(&repository(&host, "shop/api"), "latest", None)
        .await
        .unwrap();
    assert_eq!(digest, expected);
}

#[tokio::test]
async fn tag_digests_list_the_index_digest_before_its_children() {
    let entry = |byte: u8| {
        json!({
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest": fake_digest(byte),
            "size": 1
        })
    };
    let index = json!({"manifests": [entry(7), entry(8), entry(7)]}).to_string();
    let plain =
        json!({"config": {"mediaType": "x", "digest": fake_digest(1), "size": 1}}).to_string();
    let (index_digest, plain_digest) = (digest_of(index.as_bytes()), digest_of(plain.as_bytes()));
    let (host, _) = serve(move |request, _| match request.path.as_str() {
        "/v2/shop/api/manifests/multi" => reply("200 OK", &[], &index),
        _ => reply("200 OK", &[], &plain),
    });
    let repository = repository(&host, "shop/api");
    let client = client();

    let multi = client
        .tag_digests(&repository, "multi", None)
        .await
        .unwrap();
    assert_eq!(multi, [index_digest, fake_digest(7), fake_digest(8)]);

    let single = client
        .tag_digests(&repository, "single", None)
        .await
        .unwrap();
    assert_eq!(single, [plain_digest]);
}

#[tokio::test]
async fn referrers_fall_back_to_the_tag_schema_on_404() {
    let attestation = json!({"manifests": [{
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "digest": fake_digest(4),
        "size": 10,
        "artifactType": "application/vnd.dev.sigstore.bundle.v0.3+json",
        "annotations": {"k": "v"}
    }]})
    .to_string();
    let tag_path = format!("/v2/shop/api/manifests/sha256-{}", "aa".repeat(32));
    let api_path = format!("/v2/shop/api/referrers/sha256:{}", "bb".repeat(32));
    let (host, log) = serve(move |request, _| {
        if request.authorization.as_deref() != Some("Bearer pass") {
            return reply("401 Unauthorized", &[], "");
        }
        if request.path == tag_path || request.path == api_path {
            return reply("200 OK", &[], &attestation);
        }
        reply("404 Not Found", &[], "")
    });
    let repository = repository(&host, "shop/api");
    let auth = RegistryAuth {
        username: None,
        secret: "pass".into(),
        bearer: true,
        token_hosts: Vec::new(),
    };
    let client = client();

    let by_tag = client
        .referrers(&repository, &fake_digest(0xaa), Some(&auth))
        .await
        .unwrap();
    assert_eq!(by_tag.len(), 1);
    assert_eq!(
        by_tag[0].artifact_type.as_deref(),
        Some("application/vnd.dev.sigstore.bundle.v0.3+json")
    );
    assert_eq!(by_tag[0].annotations["k"], "v");

    let by_api = client
        .referrers(&repository, &fake_digest(0xbb), Some(&auth))
        .await
        .unwrap();
    assert_eq!(by_api.len(), 1);

    let none = client
        .referrers(&repository, &fake_digest(0xcc), Some(&auth))
        .await
        .unwrap();
    assert!(none.is_empty());

    let paths: Vec<_> = log
        .lock()
        .unwrap()
        .iter()
        .map(|(path, _)| path.clone())
        .collect();
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.contains("/referrers/"))
            .count(),
        3
    );
}

#[tokio::test]
async fn ping_accepts_200_or_a_401_followed_by_a_token_exchange() {
    let (open, _) = serve(|_, _| reply("200 OK", &[], "{}"));
    assert!(client().ping(&open, None).await.unwrap());

    let (gated, _) = serve(|request, port| {
        if request.authorization.as_deref() == Some("Bearer t") {
            return reply("200 OK", &[], "{}");
        }
        if request.path.starts_with("/token") {
            if request.auth_is(&basic("bot", "pw")) {
                return reply("200 OK", &[], r#"{"access_token":"t"}"#);
            }
            return reply("401 Unauthorized", &[], "denied");
        }
        reply(
            "401 Unauthorized",
            &challenge(port, "registry:catalog:*"),
            "",
        )
    });
    assert!(client().ping(&gated, Some(&credentials())).await.unwrap());
    let wrong = RegistryAuth {
        secret: "nope".into(),
        ..credentials()
    };
    assert!(!client().ping(&gated, Some(&wrong)).await.unwrap());
}

#[tokio::test]
async fn ping_answers_basic_challenges_with_the_credentials() {
    let (host, _) = serve(|request, _| match request.authorization.as_deref() {
        Some(value) if value == basic("bot", "pw") => reply("200 OK", &[], "{}"),
        _ => reply(
            "401 Unauthorized",
            &[("WWW-Authenticate", r#"Basic realm="Registry""#.to_string())],
            "",
        ),
    });
    assert!(client().ping(&host, Some(&credentials())).await.unwrap());
    assert!(!client().ping(&host, None).await.unwrap());
}

fn plain_manifest() -> String {
    json!({"config": {"mediaType": "x", "digest": fake_digest(1), "size": 1}}).to_string()
}

fn port_of(host: &str) -> u16 {
    host.rsplit_once(':')
        .and_then(|(_, port)| port.parse().ok())
        .unwrap_or_default()
}

fn token_server(token: &'static str) -> (String, Log) {
    serve(move |_, _| reply("200 OK", &[], &format!(r#"{{"token":"{token}"}}"#)))
}

fn registry_behind(realm: &str, token: &'static str) -> (String, Log) {
    let realm_port = port_of(realm);
    let expected = format!("Bearer {token}");
    serve(move |request, _| {
        if request.authorization.as_deref() == Some(expected.as_str()) {
            return reply("200 OK", &[], &plain_manifest());
        }
        reply(
            "401 Unauthorized",
            &challenge(realm_port, "repository:shop/api:pull"),
            "",
        )
    })
}

fn authorizations(log: &Log) -> Vec<Option<String>> {
    log.lock()
        .unwrap()
        .iter()
        .map(|(_, authorization)| authorization.clone())
        .collect()
}

#[tokio::test]
async fn a_realm_on_another_host_gets_an_anonymous_exchange() {
    let (realm, realm_log) = token_server("anon");
    let (registry, registry_log) = registry_behind(&realm, "anon");

    let (digest, _) = client()
        .manifest(
            &repository(&registry, "shop/api"),
            "latest",
            Some(&credentials()),
        )
        .await
        .unwrap();

    assert_eq!(digest, digest_of(plain_manifest().as_bytes()));
    assert_eq!(authorizations(&realm_log), [None]);
    assert!(authorizations(&registry_log).contains(&Some("Bearer anon".into())));
}

#[tokio::test]
async fn an_explicitly_trusted_token_host_gets_the_credentials() {
    let (realm, realm_log) = token_server("trusted");
    let (registry, _) = registry_behind(&realm, "trusted");
    let auth = RegistryAuth {
        token_hosts: vec![realm.to_uppercase()],
        ..credentials()
    };

    client()
        .manifest(&repository(&registry, "shop/api"), "latest", Some(&auth))
        .await
        .unwrap();

    assert_eq!(authorizations(&realm_log), [Some(basic("bot", "pw"))]);
}

#[tokio::test]
async fn a_realm_on_the_registry_host_gets_the_credentials() {
    let (registry, log) = serve(|request, port| {
        if request.path.starts_with("/token") {
            return reply("200 OK", &[], r#"{"token":"t"}"#);
        }
        if request.authorization.as_deref() == Some("Bearer t") {
            return reply("200 OK", &[], &plain_manifest());
        }
        reply(
            "401 Unauthorized",
            &challenge(port, "repository:shop/api:pull"),
            "",
        )
    });

    client()
        .manifest(
            &repository(&registry, "shop/api"),
            "latest",
            Some(&credentials()),
        )
        .await
        .unwrap();

    let token_calls: Vec<_> = log
        .lock()
        .unwrap()
        .iter()
        .filter(|(path, _)| path.starts_with("/token"))
        .map(|(_, authorization)| authorization.clone())
        .collect();
    assert_eq!(token_calls, [Some(basic("bot", "pw"))]);
}

#[tokio::test]
async fn a_bearer_secret_is_only_sent_to_the_registry_host() {
    let (realm, realm_log) = token_server("anon");
    let (registry, registry_log) = registry_behind(&realm, "anon");
    let auth = RegistryAuth {
        username: None,
        secret: "pass".into(),
        bearer: true,
        token_hosts: vec![realm.clone()],
    };

    let result = client()
        .manifest(&repository(&registry, "shop/api"), "latest", Some(&auth))
        .await;

    assert!(result.is_err());
    assert!(authorizations(&realm_log).is_empty());
    assert_eq!(
        authorizations(&registry_log),
        [Some("Bearer pass".to_string())]
    );
}

#[tokio::test]
async fn redirects_never_carry_a_secret_to_another_host() {
    let (elsewhere, elsewhere_log) = token_server("stolen");
    let target = elsewhere.clone();
    let (registry, registry_log) = serve(move |_, _| {
        reply(
            "302 Found",
            &[("Location", format!("http://{target}/stolen"))],
            "",
        )
    });
    let bearer = RegistryAuth {
        username: None,
        secret: "pass".into(),
        bearer: true,
        token_hosts: Vec::new(),
    };
    let _ = client()
        .manifest(&repository(&registry, "shop/api"), "latest", Some(&bearer))
        .await;
    assert_eq!(
        authorizations(&registry_log),
        [Some("Bearer pass".to_string())]
    );
    assert_eq!(authorizations(&elsewhere_log), [None]);

    let (elsewhere, elsewhere_log) = token_server("stolen");
    let target = elsewhere.clone();
    let (registry, registry_log) = serve(move |request, port| {
        if request.path.starts_with("/token") {
            return reply(
                "302 Found",
                &[("Location", format!("http://{target}/stolen"))],
                "",
            );
        }
        reply(
            "401 Unauthorized",
            &challenge(port, "repository:shop/api:pull"),
            "",
        )
    });
    let _ = client()
        .manifest(
            &repository(&registry, "shop/api"),
            "latest",
            Some(&credentials()),
        )
        .await;
    assert!(authorizations(&registry_log).contains(&Some(basic("bot", "pw"))));
    assert_eq!(authorizations(&elsewhere_log), [None]);
}

#[test]
fn token_hosts_match_exactly_and_never_by_suffix() {
    let allow = |registry: &str, realm: &str, hosts: &[&str]| {
        let auth = RegistryAuth {
            token_hosts: hosts.iter().map(|host| host.to_string()).collect(),
            ..credentials()
        };
        may_receive_secret(
            &auth,
            &Url::parse(registry).unwrap(),
            &Url::parse(realm).unwrap(),
        )
    };
    let registry = "https://registry.test/v2/";
    assert!(allow(registry, "https://REGISTRY.test:443/token", &[]));
    assert!(allow(registry, "https://Auth.test/token", &["AUTH.test"]));
    assert!(allow(
        registry,
        "https://auth.test:8443/t",
        &["auth.test:8443"]
    ));
    assert!(!allow(registry, "https://auth.test/token", &[]));
    assert!(!allow(registry, "https://auth.test:8443/t", &["auth.test"]));
    assert!(!allow(registry, "https://sub.auth.test/t", &["auth.test"]));
    assert!(!allow(registry, "https://auth.test/t", &["sub.auth.test"]));
    assert!(!allow(registry, "https://registry.test:5000/t", &[]));
    assert!(!allow(
        "https://a.shared.test/v2/",
        "https://b.shared.test/token",
        &[]
    ));
    assert!(allow(registry, "https://auth.test/t", &["auth.test:443"]));
    assert!(allow(registry, "https://auth.test/t", &[" Auth.TEST:443 "]));
    assert!(allow(
        registry,
        "https://auth.test:443/t",
        &["auth.test:443"]
    ));
    assert!(allow(
        "https://registry.test:443/v2/",
        "https://registry.test/token",
        &[]
    ));
    assert!(allow(
        "http://127.0.0.1/v2/",
        "http://127.0.0.1:80/t",
        &["127.0.0.1:80"]
    ));
    assert!(allow(
        "http://127.0.0.1:5000/v2/",
        "http://127.0.0.1/t",
        &["127.0.0.1:80"]
    ));
    assert!(!allow(registry, "https://auth.test/t", &["auth.test:80"]));
    assert!(!allow(registry, "https://auth.test/t", &["auth.test:8443"]));
    assert!(!allow(
        registry,
        "https://auth.test:8443/t",
        &["auth.test:443"]
    ));
    assert!(!allow(
        "http://127.0.0.1:5000/v2/",
        "http://127.0.0.1/t",
        &["127.0.0.1:443"]
    ));
}

#[test]
fn registry_authorities_drop_only_the_scheme_default_port() {
    assert_eq!(registry_authority(" Registry.TEST:443 "), "registry.test");
    assert_eq!(registry_authority("registry.test"), "registry.test");
    assert_eq!(
        registry_authority("registry.test:8443"),
        "registry.test:8443"
    );
    assert_eq!(registry_authority("registry.test:80"), "registry.test:80");
    assert_eq!(registry_authority("localhost:80"), "localhost");
    assert_eq!(registry_authority("localhost:443"), "localhost:443");
    assert_eq!(registry_authority("localhost:5000"), "localhost:5000");
}

fn statement(predicate_type: &str, predicate: serde_json::Value) -> String {
    base64::engine::general_purpose::STANDARD.encode(
        json!({
            "_type": "https://in-toto.io/Statement/v1",
            "subject": [{"name": "x", "digest": {"sha256": "aa"}}],
            "predicateType": predicate_type,
            "predicate": predicate
        })
        .to_string(),
    )
}

fn envelope(payload: &str) -> Vec<u8> {
    json!({"payloadType": "application/vnd.in-toto+json", "payload": payload, "signatures": []})
        .to_string()
        .into_bytes()
}

fn bundle(payload: &str) -> Vec<u8> {
    json!({
        "mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json",
        "verificationMaterial": {},
        "dsseEnvelope": {"payloadType": "application/vnd.in-toto+json", "payload": payload, "signatures": []}
    })
    .to_string()
    .into_bytes()
}

#[test]
fn dsse_decodes_slsa_v1_envelopes_and_bundles() {
    let payload = statement(
        "https://slsa.dev/provenance/v1",
        json!({
            "buildDefinition": {
                "externalParameters": {"workflow": {"repository": "https://git.test/Acme/shop.git", "ref": "refs/heads/main"}},
                "resolvedDependencies": [
                    {"uri": "git+https://git.test/acme/shop", "digest": {"sha1": "x"}},
                    {"uri": "git+https://git.test/acme/shop", "digest": {"gitCommit": COMMIT}}
                ]
            },
            "runDetails": {"metadata": {"invocationId": "https://git.test/acme/shop/actions/runs/1/attempts/1"}}
        }),
    );
    let expected = SlsaProvenance {
        repo: RepoId::parse("https://git.test/Acme/shop"),
        commit: CommitSha::parse(COMMIT),
        invocation: Some("https://git.test/acme/shop/actions/runs/1/attempts/1".into()),
    };
    assert_eq!(dsse_provenance(&envelope(&payload)), Some(expected.clone()));
    assert_eq!(dsse_provenance(&bundle(&payload)), Some(expected));
}

#[test]
fn dsse_decodes_slsa_v02_payloads() {
    let payload = statement(
        "https://slsa.dev/provenance/v0.2",
        json!({
            "builder": {"id": "https://git.test/slsa-framework/slsa-generator"},
            "invocation": {"configSource": {
                "uri": "git+https://git.test/acme/shop@refs/heads/main",
                "digest": {"sha1": COMMIT},
                "entryPoint": "ci/workflows/release.yml"
            }},
            "metadata": {"buildInvocationId": "42-1"}
        }),
    );
    assert_eq!(
        dsse_provenance(&bundle(&payload)),
        Some(SlsaProvenance {
            repo: RepoId::parse("https://git.test/acme/shop"),
            commit: CommitSha::parse(COMMIT),
            invocation: Some("42-1".into()),
        })
    );
}

#[test]
fn dsse_keeps_missing_fields_empty_and_rejects_other_payloads() {
    let sparse = statement(
        "https://slsa.dev/provenance/v1",
        json!({"buildDefinition": {}}),
    );
    assert_eq!(
        dsse_provenance(&envelope(&sparse)),
        Some(SlsaProvenance::default())
    );

    let sbom = statement("https://spdx.dev/Document", json!({}));
    assert_eq!(dsse_provenance(&envelope(&sbom)), None);
    assert_eq!(dsse_provenance(&envelope("!!not base64!!")), None);
    assert_eq!(dsse_provenance(b"not json"), None);
    assert_eq!(dsse_provenance(br#"{"messageSignature": {}}"#), None);
}

#[test]
fn source_urls_drop_the_git_ref() {
    assert_eq!(
        source_url("git+https://git.test/acme/shop@refs/heads/main"),
        "https://git.test/acme/shop"
    );
    assert_eq!(
        source_url("git+ssh://git@host.test/acme/shop@refs/tags/v1"),
        "ssh://git@host.test/acme/shop"
    );
    assert_eq!(
        source_url("https://host.test/acme/shop"),
        "https://host.test/acme/shop"
    );
}
