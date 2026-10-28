pub(crate) mod loopback;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{
    DateTime,
    Duration,
    Utc,
};
use pipedash_plugin_api::{
    from_status,
    map_send_error,
    redact_url,
    truncate_chars,
    SignInClient,
    SignInToken,
};
use reqwest::Url;
use serde::Deserialize;
use serde_json::{
    Map,
    Value,
};
use sha2::{
    Digest,
    Sha256,
};

use crate::domain::{
    DomainError,
    DomainResult,
    SignInCredential,
    SIGN_IN_EXPIRED,
};
use crate::infrastructure::providers::PluginAdapter;

const OFFLINE_ACCESS: &str = "offline_access";
const ACCOUNT_CLAIMS: [&str; 4] = ["email", "preferred_username", "name", "sub"];
const EXPIRED_GRANT_ERRORS: [&str; 3] = ["invalid_grant", "invalid_client", "unauthorized_client"];
const BODY_PREVIEW_CHARS: usize = 300;

#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    pub authorization: Url,
    pub token: Url,
    pub scopes_supported: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Discovery {
    authorization_endpoint: Url,
    token_endpoint: Url,
    #[serde(default)]
    scopes_supported: Option<Vec<String>>,
}

pub(crate) struct Authorization {
    pub url: Url,
    pub state: String,
    pub verifier: String,
}

#[derive(Debug, Default)]
pub(crate) struct Callback {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

impl Callback {
    pub(crate) fn from_url(url: &Url) -> Self {
        let mut callback = Self::default();
        for (key, value) in url.query_pairs() {
            let slot = match key.as_ref() {
                "code" => &mut callback.code,
                "state" => &mut callback.state,
                "error" => &mut callback.error,
                "error_description" => &mut callback.error_description,
                _ => continue,
            };
            *slot = Some(value.into_owned());
        }
        callback
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Deserialize)]
struct OAuthError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Clone, Copy)]
enum Grant {
    Exchange,
    Refresh,
}

pub(crate) fn random_id<const N: usize>() -> String {
    URL_SAFE_NO_PAD.encode(rand::random::<[u8; N]>())
}

fn parse_url(raw: &str) -> DomainResult<Url> {
    Url::parse(raw)
        .map_err(|e| DomainError::InvalidConfig(format!("Invalid sign-in URL {raw}: {e}")))
}

fn send_error(err: reqwest::Error, url: &Url) -> DomainError {
    PluginAdapter::map_error(map_send_error(err, url))
}

pub(crate) async fn discover(http: &reqwest::Client, issuer: &str) -> DomainResult<Endpoints> {
    let url = parse_url(&format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    ))?;
    let response = http
        .get(url.clone())
        .send()
        .await
        .map_err(|e| send_error(e, &url))?;
    let status = response.status();
    if !status.is_success() {
        return Err(DomainError::InvalidConfig(format!(
            "Couldn't read the sign-in settings from {} ({status})",
            redact_url(&url)
        )));
    }
    let discovery: Discovery = response.json().await.map_err(|e| {
        DomainError::ApiError(format!(
            "The sign-in server at {} returned invalid settings: {}",
            redact_url(&url),
            e.without_url()
        ))
    })?;
    Ok(Endpoints {
        authorization: discovery.authorization_endpoint,
        token: discovery.token_endpoint,
        scopes_supported: discovery.scopes_supported,
    })
}

pub(crate) fn authorize(client: &SignInClient, endpoints: &Endpoints) -> Authorization {
    let verifier = random_id::<32>();
    let state = random_id::<32>();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

    let mut scopes = client.scopes.clone();
    let offline_supported = endpoints
        .scopes_supported
        .as_ref()
        .is_none_or(|supported| supported.iter().any(|scope| scope == OFFLINE_ACCESS));
    if offline_supported && !scopes.iter().any(|scope| scope == OFFLINE_ACCESS) {
        scopes.push(OFFLINE_ACCESS.to_string());
    }

    let mut url = endpoints.authorization.clone();
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client.client_id)
        .append_pair("redirect_uri", &client.redirect_uri)
        .append_pair("scope", &scopes.join(" "))
        .append_pair("state", &state)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .extend_pairs(
            client
                .authorize_params
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        );

    Authorization {
        url,
        state,
        verifier,
    }
}

pub(crate) async fn exchange(
    http: &reqwest::Client, token_endpoint: &Url, client: &SignInClient, code: &str, verifier: &str,
) -> DomainResult<SignInCredential> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", client.redirect_uri.as_str()),
        ("client_id", client.client_id.as_str()),
        ("code_verifier", verifier),
    ];
    let response = request_token(http, token_endpoint, &form, Grant::Exchange).await?;
    credential(
        response,
        client.token,
        token_endpoint,
        &client.client_id,
        None,
    )
}

pub(crate) async fn refresh(
    http: &reqwest::Client, current: &SignInCredential,
) -> DomainResult<SignInCredential> {
    let refresh_token = current
        .refresh_token
        .as_deref()
        .ok_or_else(|| DomainError::ProviderAuthenticationFailed(SIGN_IN_EXPIRED.into()))?;
    let token_endpoint = parse_url(&current.token_endpoint)?;
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", current.client_id.as_str()),
    ];
    let response = request_token(http, &token_endpoint, &form, Grant::Refresh).await?;
    credential(
        response,
        current.kind,
        &token_endpoint,
        &current.client_id,
        Some(current),
    )
}

async fn request_token(
    http: &reqwest::Client, endpoint: &Url, form: &[(&str, &str)], grant: Grant,
) -> DomainResult<TokenResponse> {
    let response = http
        .post(endpoint.clone())
        .form(form)
        .send()
        .await
        .map_err(|e| send_error(e, endpoint))?;
    let status = response.status();
    let body = response.text().await.map_err(|e| send_error(e, endpoint))?;

    if status.is_success() {
        return serde_json::from_str(&body).map_err(|e| {
            DomainError::ApiError(format!(
                "The sign-in server returned an invalid token response: {e}"
            ))
        });
    }

    if let Ok(error) = serde_json::from_str::<OAuthError>(&body) {
        match grant {
            Grant::Refresh if EXPIRED_GRANT_ERRORS.contains(&error.error.as_str()) => {
                return Err(DomainError::ProviderAuthenticationFailed(
                    SIGN_IN_EXPIRED.into(),
                ));
            }
            Grant::Exchange => {
                return Err(DomainError::ProviderAuthenticationFailed(
                    error.error_description.unwrap_or(error.error),
                ));
            }
            Grant::Refresh => {}
        }
    }

    let message = format!(
        "{} from {}: {}",
        status.as_u16(),
        redact_url(endpoint),
        truncate_chars(body.trim(), BODY_PREVIEW_CHARS)
    );
    Err(PluginAdapter::map_error(from_status(
        status.as_u16(),
        message,
        None,
    )))
}

fn credential(
    response: TokenResponse, kind: SignInToken, token_endpoint: &Url, client_id: &str,
    previous: Option<&SignInCredential>,
) -> DomainResult<SignInCredential> {
    let token = match kind {
        SignInToken::IdToken => response.id_token.clone().ok_or_else(|| {
            DomainError::ProviderAuthenticationFailed(
                "The sign-in server didn't return an ID token".into(),
            )
        })?,
        SignInToken::AccessToken => response.access_token,
    };
    let expires_at = jwt_claims(&token)
        .and_then(|claims| claims.get("exp")?.as_i64())
        .and_then(|exp| DateTime::from_timestamp(exp, 0))
        .or_else(|| {
            response
                .expires_in
                .map(|seconds| Utc::now() + Duration::seconds(seconds))
        });
    let account = response
        .id_token
        .as_deref()
        .and_then(jwt_claims)
        .and_then(|claims| account_of(&claims))
        .or_else(|| previous.and_then(|p| p.account.clone()));
    let refresh_token = response
        .refresh_token
        .or_else(|| previous.and_then(|p| p.refresh_token.clone()));

    Ok(SignInCredential {
        token,
        refresh_token,
        expires_at,
        token_endpoint: token_endpoint.to_string(),
        client_id: client_id.to_string(),
        kind,
        account,
    })
}

fn jwt_claims(token: &str) -> Option<Map<String, Value>> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn account_of(claims: &Map<String, Value>) -> Option<String> {
    ACCOUNT_CLAIMS.iter().find_map(|key| {
        claims
            .get(*key)?
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

#[cfg(test)]
pub(crate) mod test_idp {
    use std::sync::{
        Arc,
        Mutex,
    };

    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    use serde_json::{
        json,
        Value,
    };
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    pub(crate) fn jwt(claims: Value) -> String {
        format!(
            "{}.{}.sig",
            URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#),
            URL_SAFE_NO_PAD.encode(claims.to_string())
        )
    }

    #[derive(Clone)]
    pub(crate) struct Idp {
        pub base_url: String,
        pub forms: Arc<Mutex<Vec<String>>>,
        reply: Arc<Mutex<(u16, String)>>,
    }

    impl Idp {
        pub(crate) async fn start() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base_url = format!("http://{}", listener.local_addr().unwrap());
            let idp = Self {
                base_url: base_url.clone(),
                forms: Arc::default(),
                reply: Arc::new(Mutex::new((200, "{}".into()))),
            };
            let server = idp.clone();
            tokio::spawn(async move {
                while let Ok((mut stream, _)) = listener.accept().await {
                    let server = server.clone();
                    tokio::spawn(async move {
                        let (target, body) = read_request(&mut stream).await;
                        let (status, reply) =
                            if target.ends_with("/.well-known/openid-configuration") {
                                (
                                200,
                                json!({
                                    "authorization_endpoint": format!("{}/auth", server.base_url),
                                    "token_endpoint": format!("{}/token", server.base_url),
                                    "scopes_supported": ["openid", "email", "offline_access"],
                                })
                                .to_string(),
                            )
                            } else {
                                server.forms.lock().unwrap().push(body);
                                server.reply.lock().unwrap().clone()
                            };
                        let response = format!(
                            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                            reply.len()
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                    });
                }
            });
            idp
        }

        pub(crate) fn token_endpoint(&self) -> String {
            format!("{}/token", self.base_url)
        }

        pub(crate) fn reply(&self, status: u16, body: Value) {
            *self.reply.lock().unwrap() = (status, body.to_string());
        }
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> (String, String) {
        let mut data = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&data[..end]).to_string();
                let length = head
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                if data.len() >= end + 4 + length {
                    let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let body = String::from_utf8_lossy(&data[end + 4..end + 4 + length]);
                    return (target, body.to_string());
                }
            }
            match stream.read(&mut buffer).await {
                Ok(0) | Err(_) => return (String::new(), String::new()),
                Ok(n) => data.extend_from_slice(&buffer[..n]),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::test_idp::{
        jwt,
        Idp,
    };
    use super::*;

    fn client(token: SignInToken) -> SignInClient {
        SignInClient {
            label: "Dex".into(),
            issuer: "https://idp.example.com".into(),
            client_id: "argo-cd-cli".into(),
            scopes: vec!["openid".into(), "email".into()],
            redirect_uri: "http://localhost:8085/auth/callback".into(),
            authorize_params: vec![("connector_id".into(), "github".into())],
            token,
        }
    }

    fn endpoints(scopes_supported: Option<Vec<String>>) -> Endpoints {
        Endpoints {
            authorization: Url::parse("https://idp.example.com/auth").unwrap(),
            token: Url::parse("https://idp.example.com/token").unwrap(),
            scopes_supported,
        }
    }

    fn query(url: &Url) -> Vec<(String, String)> {
        url.query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    #[test]
    fn authorize_uses_pkce_s256_and_requests_offline_access_once() {
        let authorization = authorize(&client(SignInToken::IdToken), &endpoints(None));
        let pairs = query(&authorization.url);
        let get = |key: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap()
        };

        assert_eq!(get("response_type"), "code");
        assert_eq!(get("scope"), "openid email offline_access");
        assert_eq!(get("state"), authorization.state);
        assert_eq!(get("code_challenge_method"), "S256");
        assert_eq!(
            get("code_challenge"),
            URL_SAFE_NO_PAD.encode(Sha256::digest(authorization.verifier.as_bytes()))
        );
        assert_eq!(
            pairs.last().unwrap(),
            &("connector_id".into(), "github".into())
        );

        let mut with_offline = client(SignInToken::IdToken);
        with_offline.scopes.push(OFFLINE_ACCESS.into());
        let again = authorize(&with_offline, &endpoints(None));
        assert_eq!(
            query(&again.url)
                .iter()
                .find(|(k, _)| k == "scope")
                .unwrap()
                .1,
            "openid email offline_access"
        );
        let unsupported = authorize(
            &client(SignInToken::IdToken),
            &endpoints(Some(vec!["openid".into()])),
        );
        assert!(!unsupported.url.as_str().contains(OFFLINE_ACCESS));
    }

    #[tokio::test]
    async fn exchange_posts_verifier_and_reads_id_token() {
        let idp = Idp::start().await;
        let id_token = jwt(json!({"exp": 1_900_000_000, "email": "dev@example.com", "sub": "1"}));
        idp.reply(
            200,
            json!({"access_token": "opaque", "id_token": id_token, "refresh_token": "r1", "expires_in": 60}),
        );
        let endpoint = Url::parse(&idp.token_endpoint()).unwrap();

        let credential = exchange(
            &reqwest::Client::new(),
            &endpoint,
            &client(SignInToken::IdToken),
            "the-code",
            "the-verifier",
        )
        .await
        .unwrap();

        assert_eq!(credential.token, id_token);
        assert_eq!(credential.refresh_token.as_deref(), Some("r1"));
        assert_eq!(credential.account.as_deref(), Some("dev@example.com"));
        assert_eq!(
            credential.expires_at,
            DateTime::from_timestamp(1_900_000_000, 0)
        );
        let form = idp.forms.lock().unwrap()[0].clone();
        assert!(form.contains("grant_type=authorization_code"));
        assert!(form.contains("code=the-code"));
        assert!(form.contains("code_verifier=the-verifier"));
        assert!(form.contains("redirect_uri=http%3A%2F%2Flocalhost%3A8085%2Fauth%2Fcallback"));
    }

    #[tokio::test]
    async fn exchange_reports_the_identity_provider_error() {
        let idp = Idp::start().await;
        idp.reply(
            400,
            json!({"error": "invalid_request", "error_description": "Bad code"}),
        );
        let endpoint = Url::parse(&idp.token_endpoint()).unwrap();

        let error = exchange(
            &reqwest::Client::new(),
            &endpoint,
            &client(SignInToken::IdToken),
            "c",
            "v",
        )
        .await
        .unwrap_err();

        assert!(matches!(error, DomainError::ProviderAuthenticationFailed(m) if m == "Bad code"));
    }

    fn stored(idp: &Idp) -> SignInCredential {
        SignInCredential {
            token: "old".into(),
            refresh_token: Some("r1".into()),
            expires_at: None,
            token_endpoint: idp.token_endpoint(),
            client_id: "cli".into(),
            kind: SignInToken::AccessToken,
            account: Some("dev@example.com".into()),
        }
    }

    #[tokio::test]
    async fn refresh_keeps_previous_refresh_token_and_account() {
        let idp = Idp::start().await;
        idp.reply(200, json!({"access_token": "new", "expires_in": 3600}));

        let renewed = refresh(&reqwest::Client::new(), &stored(&idp))
            .await
            .unwrap();

        assert_eq!(renewed.token, "new");
        assert_eq!(renewed.refresh_token.as_deref(), Some("r1"));
        assert_eq!(renewed.account.as_deref(), Some("dev@example.com"));
        assert!(renewed.expires_at.unwrap() > Utc::now() + Duration::minutes(59));
        let form = idp.forms.lock().unwrap()[0].clone();
        assert!(form.contains("grant_type=refresh_token"));
        assert!(form.contains("refresh_token=r1"));
    }

    #[tokio::test]
    async fn refresh_with_revoked_grant_asks_to_sign_in_again() {
        let idp = Idp::start().await;
        idp.reply(400, json!({"error": "invalid_grant"}));

        let error = refresh(&reqwest::Client::new(), &stored(&idp))
            .await
            .unwrap_err();

        assert!(
            matches!(error, DomainError::ProviderAuthenticationFailed(m) if m == SIGN_IN_EXPIRED)
        );
    }

    #[tokio::test]
    async fn refresh_server_error_stays_transient() {
        let idp = Idp::start().await;
        idp.reply(503, json!({"error": "temporarily_unavailable"}));

        let error = refresh(&reqwest::Client::new(), &stored(&idp))
            .await
            .unwrap_err();

        assert!(matches!(error, DomainError::ServiceUnavailable { .. }));
    }

    #[test]
    fn callback_reads_query_pairs() {
        let callback = Callback::from_url(
            &Url::parse("http://localhost:8085/auth/callback?code=c&state=s&extra=1").unwrap(),
        );
        assert_eq!(callback.code.as_deref(), Some("c"));
        assert_eq!(callback.state.as_deref(), Some("s"));
        assert_eq!(callback.error, None);
    }
}
