use super::*;

fn subscription(token: &str) -> Subscription {
    Subscription {
        device_token: token.to_string(),
        environment: Environment::Sandbox,
        bundle_id: "com.example.app".to_string(),
    }
}

#[tokio::test]
async fn a_subscription_round_trips_and_can_rotate_and_be_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("relay.db")).await.unwrap();
    let key = subscription_key("secret-id");

    assert_eq!(store.get(&key).await.unwrap(), None);
    store
        .insert(&key, &subscription("aa"), 1, 10)
        .await
        .unwrap();
    assert_eq!(store.get(&key).await.unwrap(), Some(subscription("aa")));

    assert!(store.update_token(&key, "bb", 2).await.unwrap());
    assert_eq!(store.get(&key).await.unwrap(), Some(subscription("bb")));
    assert!(!store.update_token("unknown", "cc", 2).await.unwrap());

    assert!(store.delete(&key).await.unwrap());
    assert!(!store.delete(&key).await.unwrap());
    assert_eq!(store.get(&key).await.unwrap(), None);
}

#[tokio::test]
async fn delete_if_token_leaves_a_subscription_whose_token_changed() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("relay.db")).await.unwrap();
    let key = subscription_key("secret-id");
    store
        .insert(&key, &subscription("aa"), 1, 10)
        .await
        .unwrap();
    store.update_token(&key, "bb", 2).await.unwrap();

    assert!(!store.delete_if_token(&key, "aa").await.unwrap());
    assert_eq!(store.get(&key).await.unwrap(), Some(subscription("bb")));
    assert!(store.delete_if_token(&key, "bb").await.unwrap());
    assert_eq!(store.get(&key).await.unwrap(), None);
}

#[tokio::test]
async fn the_subscription_id_itself_is_never_stored() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("relay.db")).await.unwrap();
    let key = subscription_key("secret-id");
    store
        .insert(&key, &subscription("aa"), 1, 10)
        .await
        .unwrap();

    let stored: Vec<(String,)> = sqlx::query_as("SELECT id_hash FROM subscriptions")
        .fetch_all(&store.pool)
        .await
        .unwrap();

    assert_eq!(stored, vec![(key.clone(),)]);
    assert_eq!(key.len(), 64);
    assert!(!key.contains("secret-id"));
}

#[tokio::test]
async fn only_the_newest_subscriptions_of_one_device_token_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("relay.db")).await.unwrap();
    let other = subscription_key("other-token");
    store
        .insert(&other, &subscription("bb"), 1, 2)
        .await
        .unwrap();

    for i in 0..5 {
        let key = subscription_key(&format!("id-{i}"));
        store.insert(&key, &subscription("aa"), 1, 2).await.unwrap();
    }

    for (i, kept) in [(0, false), (1, false), (2, false), (3, true), (4, true)] {
        let key = subscription_key(&format!("id-{i}"));
        assert_eq!(store.get(&key).await.unwrap().is_some(), kept, "id-{i}");
    }
    assert!(store.get(&other).await.unwrap().is_some());
}

#[tokio::test]
async fn inactive_subscriptions_expire_but_rotation_and_pushes_keep_them_alive() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("relay.db")).await.unwrap();
    let [stale, rotated, pushed] = ["stale", "rotated", "pushed"].map(subscription_key);
    for key in [&stale, &rotated, &pushed] {
        store
            .insert(key, &subscription(key), 100, 10)
            .await
            .unwrap();
    }
    store.update_token(&rotated, "new", 500).await.unwrap();
    store.touch(&pushed, 500).await.unwrap();

    assert_eq!(store.delete_inactive_since(500).await.unwrap(), 1);

    assert!(store.get(&stale).await.unwrap().is_none());
    assert!(store.get(&rotated).await.unwrap().is_some());
    assert!(store.get(&pushed).await.unwrap().is_some());
    assert_eq!(store.delete_inactive_since(501).await.unwrap(), 2);
}
