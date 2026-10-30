use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignInToken {
    IdToken,
    AccessToken,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignInClient {
    pub label: String,
    pub issuer: String,
    pub client_id: String,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
    pub authorize_params: Vec<(String, String)>,
    pub token: SignInToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignInRefreshRejection {
    pub status: u16,
    pub error: &'static str,
    pub description: &'static str,
}
