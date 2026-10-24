use serde::Deserialize;

use super::{
    call,
    client,
    Peer,
    PushError,
    Result,
};

const DEVICE_NAME: &str = "Pipedash for iOS";

fn route(url: &str, id: Option<&str>) -> String {
    match id {
        Some(id) => format!("{url}/api/v1/push/subscriptions/{id}"),
        None => format!("{url}/api/v1/push/subscriptions"),
    }
}

pub async fn register(
    url: &str, host: &str, api_token: &str, subscription_id: &str,
) -> Result<String> {
    #[derive(Deserialize)]
    struct Sub {
        id: String,
    }

    let request = client()?
        .post(route(url, None))
        .bearer_auth(api_token)
        .json(&serde_json::json!({
            "subscription_id": subscription_id,
            "device_name": DEVICE_NAME,
            "platform": "ios",
        }));
    let response = call(&Peer::Server(host), request, false).await?;
    let status = response.status().as_u16();
    response
        .json::<Sub>()
        .await
        .map(|sub| sub.id)
        .map_err(|_| PushError::Unexpected(host.to_owned(), status))
}

pub async fn delete(url: &str, host: &str, api_token: &str, id: &str) -> Result<()> {
    let request = client()?
        .delete(route(url, Some(id)))
        .bearer_auth(api_token);
    call(&Peer::Server(host), request, true).await.map(drop)
}

pub async fn test(url: &str, host: &str, api_token: &str, id: &str) -> Result<()> {
    let request = client()?
        .post(format!("{}/test", route(url, Some(id))))
        .bearer_auth(api_token);
    call(&Peer::Server(host), request, false).await.map(drop)
}
