use std::fmt;

use chrono::{
    DateTime,
    Duration,
    Utc,
};
use pipedash_plugin_api::SignInToken;
use serde::{
    Deserialize,
    Serialize,
};

use super::error::{
    DomainError,
    DomainResult,
};

pub const SIGN_IN_CONFIG_KEY: &str = "sign_in";
pub const SIGN_IN_EXPIRED: &str = "Your sign-in expired. Sign in again.";

const STORED_PREFIX: &str = "pipedash-sign-in:v1:";
const RENEW_MARGIN_SECONDS: i64 = 60;
const PROVIDER_REF_PREFIX: &str = "provider:";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInCredential {
    pub token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub token_endpoint: String,
    pub client_id: String,
    pub kind: SignInToken,
    pub account: Option<String>,
}

impl SignInCredential {
    pub fn encode(&self) -> DomainResult<String> {
        serde_json::to_string(self)
            .map(|json| format!("{STORED_PREFIX}{json}"))
            .map_err(|e| DomainError::InternalError(format!("Couldn't store the sign-in: {e}")))
    }

    pub fn decode(stored: &str) -> Option<Self> {
        serde_json::from_str(stored.strip_prefix(STORED_PREFIX)?).ok()
    }

    pub fn bearer(stored: &str) -> String {
        Self::decode(stored)
            .map(|credential| credential.token)
            .unwrap_or_else(|| stored.to_string())
    }

    pub fn renew_at(&self) -> Option<DateTime<Utc>> {
        self.refresh_token.as_ref()?;
        Some(self.expires_at? - Duration::seconds(RENEW_MARGIN_SECONDS))
    }
}

impl fmt::Debug for SignInCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignInCredential")
            .field("account", &self.account)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInRef {
    Flow(String),
    Provider(i64),
}

impl SignInRef {
    pub fn parse(reference: &str) -> Self {
        reference
            .strip_prefix(PROVIDER_REF_PREFIX)
            .and_then(|id| id.parse().ok())
            .map_or_else(|| Self::Flow(reference.to_string()), Self::Provider)
    }
}

impl fmt::Display for SignInRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Flow(id) => f.write_str(id),
            Self::Provider(id) => write!(f, "{PROVIDER_REF_PREFIX}{id}"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SignInStart {
    pub id: String,
    pub authorize_url: String,
    pub label: String,
    pub manual: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SignInStatus {
    Pending,
    Completed { account: Option<String> },
    Failed { message: String },
    Expired,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credential(refresh_token: Option<&str>) -> SignInCredential {
        SignInCredential {
            token: "id-token-secret".into(),
            refresh_token: refresh_token.map(Into::into),
            expires_at: Some(DateTime::from_timestamp(1_800_000_000, 0).unwrap()),
            token_endpoint: "https://idp.example.com/token".into(),
            client_id: "cli".into(),
            kind: SignInToken::IdToken,
            account: Some("dev@example.com".into()),
        }
    }

    #[test]
    fn encoded_credential_round_trips_and_exposes_bearer() {
        let stored = credential(Some("refresh-secret")).encode().unwrap();
        assert_eq!(
            SignInCredential::decode(&stored),
            Some(credential(Some("refresh-secret")))
        );
        assert_eq!(SignInCredential::bearer(&stored), "id-token-secret");
    }

    #[test]
    fn plain_token_is_not_a_credential() {
        assert_eq!(SignInCredential::decode("ghp_plain"), None);
        assert_eq!(SignInCredential::bearer("ghp_plain"), "ghp_plain");
    }

    #[test]
    fn debug_hides_tokens() {
        let printed = format!("{:?}", credential(Some("refresh-secret")));
        assert!(!printed.contains("id-token-secret"));
        assert!(!printed.contains("refresh-secret"));
        assert!(printed.contains("dev@example.com"));
    }

    #[test]
    fn renews_a_minute_before_expiry_only_with_refresh_token() {
        assert_eq!(credential(None).renew_at(), None);
        let with_refresh = credential(Some("r"));
        assert_eq!(
            with_refresh.renew_at(),
            Some(with_refresh.expires_at.unwrap() - Duration::seconds(60))
        );
    }

    #[test]
    fn reference_parses_provider_and_flow() {
        assert_eq!(SignInRef::parse("provider:7"), SignInRef::Provider(7));
        assert_eq!(SignInRef::Provider(7).to_string(), "provider:7");
        assert_eq!(
            SignInRef::parse("Xy-z_09"),
            SignInRef::Flow("Xy-z_09".into())
        );
    }
}
