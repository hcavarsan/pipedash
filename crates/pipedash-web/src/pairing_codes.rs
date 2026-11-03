use std::collections::HashMap;
use std::sync::{
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Duration;

use axum::http::StatusCode;
use chrono::{
    DateTime,
    TimeDelta,
    Utc,
};
use pipedash_core::domain::pairing::{
    generate_pairing_code,
    normalize_pairing_code,
};
use tokio::time::Instant;

use crate::error::{
    ApiError,
    AppError,
};

const MAX_CODES: usize = 8;
const CODE_TTL_SECS: u64 = 300;
const CODE_TTL: Duration = Duration::from_secs(CODE_TTL_SECS);

#[derive(Default)]
pub struct PairingCodes {
    map: Mutex<HashMap<String, Instant>>,
}

impl PairingCodes {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Instant>> {
        self.map.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn issue(&self) -> Result<(String, DateTime<Utc>), AppError> {
        let code = generate_pairing_code();
        let mut map = self.lock();
        map.retain(|_, issued| issued.elapsed() < CODE_TTL);
        if map.len() >= MAX_CODES {
            return Err(AppError::new(
                StatusCode::TOO_MANY_REQUESTS,
                ApiError::new(
                    "TOO_MANY_PAIRING_CODES",
                    "Too many pairing codes are open. Use one or wait 5 minutes.",
                ),
            ));
        }
        map.insert(code.clone(), Instant::now());

        Ok((code, Utc::now() + TimeDelta::seconds(CODE_TTL_SECS as i64)))
    }

    pub fn take(&self, input: &str) -> bool {
        let Some(code) = normalize_pairing_code(input) else {
            return false;
        };

        self.lock()
            .remove(&code)
            .is_some_and(|issued| issued.elapsed() < CODE_TTL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_code_works_once_in_any_spelling() {
        let codes = PairingCodes::default();
        let (code, _) = codes.issue().unwrap();

        assert!(!codes.take("ZZZZ-ZZZZ"));
        assert!(codes.take(&format!("{}-{}", &code[..4], code[4..].to_lowercase())));
        assert!(!codes.take(&code));
    }

    #[tokio::test]
    async fn a_code_expires_after_five_minutes() {
        let codes = PairingCodes::default();
        let (code, _) = codes.issue().unwrap();

        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(301)).await;

        assert!(!codes.take(&code));
    }

    #[tokio::test]
    async fn the_ninth_open_code_is_refused_until_one_expires() {
        let codes = PairingCodes::default();
        for _ in 0..MAX_CODES {
            codes.issue().unwrap();
        }

        let refused = codes.issue().err().unwrap();
        assert_eq!(refused.status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(refused.error.code, "TOO_MANY_PAIRING_CODES");

        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(301)).await;

        assert!(codes.issue().is_ok());
    }
}
