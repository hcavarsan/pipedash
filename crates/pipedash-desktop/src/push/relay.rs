use serde::{
    Deserialize,
    Serialize,
};

use super::{
    call,
    client,
    Peer,
    PushError,
    Result,
};

const BUNDLE_ID: &str = "com.henrique.pipedash";

pub fn base_url() -> Option<&'static str> {
    option_env!("PIPEDASH_PUSH_RELAY_URL")
        .map(|url| url.trim().trim_end_matches('/'))
        .filter(|url| !url.is_empty())
}

fn environment() -> &'static str {
    if cfg!(debug_assertions) {
        "sandbox"
    } else {
        "production"
    }
}

pub async fn create(base: &str, device_token: &str) -> Result<String> {
    #[derive(Serialize)]
    struct Body<'a> {
        device_token: &'a str,
        environment: &'static str,
        bundle_id: &'static str,
    }

    #[derive(Deserialize)]
    struct Created {
        subscription_id: String,
    }

    let request = client()?
        .post(format!("{base}/v1/subscriptions"))
        .json(&Body {
            device_token,
            environment: environment(),
            bundle_id: BUNDLE_ID,
        });
    let response = call(&Peer::Relay, request, false).await?;
    let status = response.status().as_u16();
    response
        .json::<Created>()
        .await
        .map(|created| created.subscription_id)
        .map_err(|_| PushError::Unexpected(Peer::Relay.label(), status))
}

#[cfg(target_os = "ios")]
pub async fn rotate(base: &str, subscription_id: &str, device_token: &str) -> Result<()> {
    let request = client()?
        .put(format!("{base}/v1/subscriptions/{subscription_id}"))
        .json(&serde_json::json!({ "device_token": device_token }));
    call(&Peer::Relay, request, false).await.map(drop)
}

pub async fn delete(base: &str, subscription_id: &str) -> Result<()> {
    let request = client()?.delete(format!("{base}/v1/subscriptions/{subscription_id}"));
    call(&Peer::Relay, request, true).await.map(drop)
}
