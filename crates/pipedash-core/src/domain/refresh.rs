use serde::{
    Deserialize,
    Serialize,
};

use super::error::FailureKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshState {
    Ok,
    Failed,
    Pending,
    Recent,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshResult {
    pub provider_id: i64,
    pub state: RefreshState,
    pub kind: Option<FailureKind>,
    pub message: Option<String>,
}

impl RefreshResult {
    pub fn new(provider_id: i64, state: RefreshState) -> Self {
        Self {
            provider_id,
            state,
            kind: None,
            message: None,
        }
    }

    pub fn failed(provider_id: i64, kind: FailureKind, message: String) -> Self {
        Self {
            provider_id,
            state: RefreshState::Failed,
            kind: Some(kind),
            message: Some(message),
        }
    }
}

#[cfg(test)]
mod tests;
