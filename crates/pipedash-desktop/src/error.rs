use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl From<pipedash_core::DomainError> for ErrorResponse {
    fn from(err: pipedash_core::DomainError) -> Self {
        ErrorResponse {
            error: err.to_string(),
            details: None,
            code: Some(err.code().to_owned()),
        }
    }
}
