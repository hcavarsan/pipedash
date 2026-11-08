use pipedash_plugin_api::SignInToken;
use serde_json::json;

use super::*;
use crate::infrastructure::oauth::loopback::tests::{
    free_redirect_uri,
    get,
};
use crate::infrastructure::oauth::test_idp::{
    jwt,
    Idp,
};

async fn setup() -> (Arc<SignInService>, Idp, SignInClient) {
    let idp = Idp::start().await;
    idp.reply(
        200,
        json!({"access_token": "a", "id_token": jwt(json!({"email": "dev@example.com"})), "refresh_token": "r"}),
    );
    let client = SignInClient {
        label: "Dex".into(),
        issuer: idp.base_url.clone(),
        client_id: "cli".into(),
        scopes: vec!["openid".into()],
        redirect_uri: free_redirect_uri().await.to_string(),
        authorize_params: Vec::new(),
        token: SignInToken::IdToken,
    };
    (Arc::new(SignInService::default()), idp, client)
}

fn callback(client: &SignInClient, start: &SignInStart, query: &str) -> Url {
    let state = Url::parse(&start.authorize_url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    let query = query.replace("{state}", &state);
    Url::parse(&format!("{}?{query}", client.redirect_uri)).unwrap()
}

#[tokio::test]
async fn browser_redirect_completes_the_flow() {
    let (service, _idp, client) = setup().await;
    let start = service
        .start(client.clone(), Arc::new(reqwest::Client::new()), &[], None)
        .await
        .unwrap();
    assert!(!start.manual);

    let (status, body) = get(&callback(&client, &start, "code=c&state={state}")).await;

    assert_eq!(status, 200);
    assert!(body.contains("Signed in"));
    let completed = SignInStatus::Completed {
        account: Some("dev@example.com".into()),
    };
    assert_eq!(service.wait(&start.id).await, completed);
    assert_eq!(
        service
            .credential(&start.id)
            .await
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("r")
    );
}

#[tokio::test]
async fn foreign_state_keeps_waiting_and_provider_errors_fail() {
    let (service, _idp, client) = setup().await;
    let start = service
        .start(client.clone(), Arc::new(reqwest::Client::new()), &[], None)
        .await
        .unwrap();

    let (status, _) = get(&callback(&client, &start, "code=c&state=other")).await;
    assert_eq!(status, 400);
    assert_eq!(service.status(&start.id), SignInStatus::Pending);

    let (status, body) = get(&callback(
        &client,
        &start,
        "error=access_denied&error_description=Denied&state={state}",
    ))
    .await;
    assert_eq!(status, 400);
    assert!(body.contains("Denied"));
    assert_eq!(
        service.status(&start.id),
        SignInStatus::Failed {
            message: "Denied".into()
        }
    );
}

#[tokio::test]
async fn pasted_address_completes_only_for_the_redirect() {
    let (service, _idp, client) = setup().await;
    let start = service
        .start(client.clone(), Arc::new(reqwest::Client::new()), &[], None)
        .await
        .unwrap();

    let foreign = service
        .complete(&start.id, "https://example.com/auth/callback?code=c")
        .await;
    assert!(matches!(foreign, Err(DomainError::InvalidConfig(m)) if m == WRONG_ADDRESS));

    let pasted = callback(&client, &start, "code=c&state={state}");
    let status = service
        .complete(&start.id, &format!("  {pasted} "))
        .await
        .unwrap();
    assert!(matches!(status, SignInStatus::Completed { .. }));
}

#[tokio::test]
async fn cancel_frees_the_redirect_port_and_expires_the_flow() {
    let (service, _idp, client) = setup().await;
    let http = Arc::new(reqwest::Client::new());
    let first = service
        .start(client.clone(), http.clone(), &[], None)
        .await
        .unwrap();

    service.cancel(&first.id).await;

    assert_eq!(service.status(&first.id), SignInStatus::Expired);
    assert!(service.credential(&first.id).await.is_err());
    let second = service.start(client, http, &[], None).await.unwrap();
    assert!(!second.manual);
}

#[tokio::test]
async fn cancel_keeps_a_completed_sign_in_for_saving() {
    let (service, _idp, client) = setup().await;
    let start = service
        .start(client.clone(), Arc::new(reqwest::Client::new()), &[], None)
        .await
        .unwrap();
    let pasted = callback(&client, &start, "code=c&state={state}");
    service.complete(&start.id, pasted.as_str()).await.unwrap();

    service.cancel(&start.id).await;

    assert!(matches!(
        service.status(&start.id),
        SignInStatus::Completed { .. }
    ));
    assert!(service.credential(&start.id).await.is_ok());
}

#[tokio::test]
async fn a_completed_sign_in_is_renewed_once_before_it_is_used() {
    let (service, idp, client) = setup().await;
    let expired = jwt(json!({"email": "dev@example.com", "exp": Utc::now().timestamp() - 10}));
    idp.reply(
        200,
        json!({"access_token": "a", "id_token": expired, "refresh_token": "r1"}),
    );
    let start = service
        .start(client.clone(), Arc::new(reqwest::Client::new()), &[], None)
        .await
        .unwrap();
    let pasted = callback(&client, &start, "code=c&state={state}");
    service.complete(&start.id, pasted.as_str()).await.unwrap();
    let fresh = jwt(json!({"email": "dev@example.com", "exp": Utc::now().timestamp() + 3600}));
    idp.reply(
        200,
        json!({"access_token": "a", "id_token": fresh, "refresh_token": "r2"}),
    );

    let first = service.credential(&start.id).await.unwrap();
    let second = service.credential(&start.id).await.unwrap();

    assert_eq!(first.token, fresh);
    assert_eq!(first.refresh_token.as_deref(), Some("r2"));
    assert_eq!(second, first);
    assert_eq!(idp.forms.lock().unwrap().len(), 2);
}
