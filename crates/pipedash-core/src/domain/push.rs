use chrono::{
    DateTime,
    Utc,
};
use sha2::{
    Digest,
    Sha256,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushSubscription {
    pub id: String,
    pub relay_subscription_id: String,
    pub device_name: String,
    pub platform: String,
    pub created_at: DateTime<Utc>,
    pub last_success_at: Option<DateTime<Utc>>,
}

impl PushSubscription {
    pub fn local_id(relay_subscription_id: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(relay_subscription_id.as_bytes());

        hasher
            .finalize()
            .iter()
            .take(16)
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}
