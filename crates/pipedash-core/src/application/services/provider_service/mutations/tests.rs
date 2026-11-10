use std::sync::Arc;
use std::time::Duration;

use crate::application::test_support::plugins::STUB_HTTP;
use crate::application::test_support::{
    unreachable_url,
    FeedStub,
    Gate,
    Harness,
    StubProvider,
};
use crate::domain::DomainError;
use crate::infrastructure::TokenStore;

const MESSAGE: &str = "Enter a whole number from 5 to 300 seconds.";

#[tokio::test]
async fn refresh_intervals_outside_five_to_three_hundred_are_rejected() {
    let h = Harness::new().await;
    let id = h.add_provider("one").await;

    for value in [i64::MIN, -1, 0, 4, 301, i64::MAX] {
        let error = h
            .provider_service
            .update_provider_refresh_interval(id, value)
            .await
            .unwrap_err();

        assert!(
            matches!(&error, DomainError::InvalidConfig(text) if text == MESSAGE),
            "{value}"
        );
    }
    assert_eq!(
        h.repository
            .get_provider(id)
            .await
            .unwrap()
            .refresh_interval,
        30
    );
}

#[tokio::test]
async fn refresh_interval_bounds_are_accepted() {
    let h = Harness::new().await;
    let id = h.add_provider("one").await;

    for value in [5, 300] {
        h.provider_service
            .update_provider_refresh_interval(id, value)
            .await
            .unwrap();
        assert_eq!(
            h.repository
                .get_provider(id)
                .await
                .unwrap()
                .refresh_interval,
            value
        );
    }
}

#[tokio::test]
async fn saving_the_refresh_interval_leaves_the_token_alone() {
    let h = Harness::new().await;
    let id = h.add_provider("one").await;
    h.token_store.fail_next_store();

    h.provider_service
        .update_provider_refresh_interval(id, 90)
        .await
        .unwrap();

    let saved = h.repository.get_provider(id).await.unwrap();
    assert_eq!(
        (saved.refresh_interval, saved.token.as_str()),
        (90, "token")
    );
    assert!(h.token_store.store_token(id, "probe").await.is_err());
}

#[tokio::test]
async fn a_failed_token_write_on_edit_keeps_the_stored_provider_and_the_running_one() {
    let h = Harness::new().await;
    let (id, _) = h.add_stub(StubProvider::new).await;
    let before = h.repository.get_provider(id).await.unwrap();
    let generation = h.provider_service.acquire(id).await.unwrap().generation;
    let mut edited = before.clone();
    edited.name = "renamed".to_string();
    edited.refresh_interval = 90;
    edited.token = "rotated".to_string();
    h.token_store.fail_next_store();

    assert!(h
        .provider_service
        .update_provider(id, edited.clone())
        .await
        .is_err());

    let saved = h.repository.get_provider(id).await.unwrap();
    assert_eq!(
        (
            saved.name.as_str(),
            saved.refresh_interval,
            saved.token.as_str()
        ),
        (
            before.name.as_str(),
            before.refresh_interval,
            before.token.as_str()
        )
    );
    assert_eq!(h.provider_service.generation_of(id).await, Some(generation));

    h.provider_service
        .update_provider(id, edited)
        .await
        .unwrap();
    let saved = h.repository.get_provider(id).await.unwrap();
    assert_eq!(
        (saved.name.as_str(), saved.token.as_str()),
        ("renamed", "rotated")
    );
}

#[tokio::test]
async fn a_failed_factory_on_add_leaves_no_provider_behind() {
    let h = Harness::new().await;
    h.stubs
        .fail_next_build(DomainError::InvalidConfig("factory down".to_string()));

    let error = h
        .provider_service
        .add_provider(h.config("one", 30))
        .await
        .unwrap_err();

    assert!(matches!(error, DomainError::InvalidConfig(_)), "{error}");
    assert!(h.repository.list_providers().await.unwrap().is_empty());
    assert!(h.token_store.get_all_tokens().await.unwrap().is_empty());
    h.provider_service
        .add_provider(h.config("one", 30))
        .await
        .unwrap();
}

async fn http_provider(h: &Harness, server_url: &str) -> i64 {
    let mut config = h.config("http", 30);
    config.provider_type = STUB_HTTP.to_string();
    config
        .config
        .insert("server_url".to_string(), server_url.to_string());

    h.repository.add_provider(&config).await.unwrap()
}

#[tokio::test]
async fn editing_a_provider_runs_both_probes_before_either_finishes() {
    let h = Harness::new().await;
    let server = FeedStub::start().await;
    let id = http_provider(&h, &server.base_url).await;
    let mut edited = h.repository.get_provider(id).await.unwrap();
    edited.refresh_interval = 90;
    let gate = Gate::new();
    server.set_gate(Some(&gate));
    let edit = tokio::spawn({
        let service = Arc::clone(&h.provider_service);
        async move { service.update_provider(id, edited).await }
    });

    tokio::time::timeout(Duration::from_secs(10), async {
        while server.request_count() < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both probes should be in flight before either is released");
    assert!(!edit.is_finished());

    gate.open();
    edit.await.unwrap().unwrap();
    let saved = h.repository.get_provider(id).await.unwrap();
    assert_eq!(saved.refresh_interval, 90);
}

#[tokio::test]
async fn editing_with_invalid_credentials_saves_nothing() {
    let h = Harness::new().await;
    let id = http_provider(&h, &unreachable_url()).await;
    let mut edited = h.repository.get_provider(id).await.unwrap();
    edited.refresh_interval = 90;

    let error = h
        .provider_service
        .update_provider(id, edited)
        .await
        .unwrap_err();

    assert!(
        matches!(&error, DomainError::InvalidConfig(text) if text.contains("Credential validation failed")),
        "{error}"
    );
    let saved = h.repository.get_provider(id).await.unwrap();
    assert_eq!(saved.refresh_interval, 30);
}
