use super::*;

#[tokio::test]
async fn test_memory_store() {
    let store = MemoryTokenStore::new();

    store.store_token(1, "secret_token").await.unwrap();

    let token = store.get_token(1).await.unwrap();
    assert_eq!(token, "secret_token");

    let all = store.get_all_tokens().await.unwrap();
    assert_eq!(all.len(), 1);

    store.delete_token(1).await.unwrap();

    assert!(store.get_token(1).await.is_err());
}
