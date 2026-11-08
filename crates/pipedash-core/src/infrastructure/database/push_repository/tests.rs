use chrono::DateTime;

use super::PushRepository;
use crate::domain::PushSubscription;
use crate::infrastructure::database::{
    init_database,
    DatabasePool,
};

fn subscription(relay_subscription_id: &str, device_name: &str) -> PushSubscription {
    PushSubscription {
        id: PushSubscription::local_id(relay_subscription_id),
        relay_subscription_id: relay_subscription_id.into(),
        device_name: device_name.into(),
        platform: "ios".into(),
        created_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        last_success_at: None,
    }
}

async fn exercise(repository: &PushRepository) {
    repository
        .delete(&PushSubscription::local_id("relay-a"))
        .await
        .unwrap();
    repository
        .delete(&PushSubscription::local_id("relay-b"))
        .await
        .unwrap();

    let first = subscription("relay-a", "Henrique's iPhone");
    let mut second = subscription("relay-b", "iPad");
    second.created_at = DateTime::from_timestamp(1_700_000_100, 0).unwrap();
    repository
        .import(&[first.clone(), second.clone()])
        .await
        .unwrap();
    assert_ne!(first.id, first.relay_subscription_id);
    assert_eq!(
        repository.get(&first.id).await.unwrap(),
        Some(first.clone())
    );

    let listed: Vec<String> = repository
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .filter(|id| *id == first.id || *id == second.id)
        .collect();
    assert_eq!(listed, vec![first.id.clone(), second.id.clone()]);

    repository.mark_success(&first.id).await.unwrap();
    let marked = repository.get(&first.id).await.unwrap().unwrap();
    assert!(marked.last_success_at.is_some());
    assert_eq!(repository.get("missing").await.unwrap(), None);

    assert!(repository.delete(&first.id).await.unwrap());
    assert!(!repository.delete(&first.id).await.unwrap());
    assert_eq!(repository.get(&first.id).await.unwrap(), None);
    assert!(repository.delete(&second.id).await.unwrap());
}

#[tokio::test]
async fn sqlite_subscriptions_can_be_imported_marked_and_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_database(dir.path().join("push.db")).await.unwrap();

    exercise(&PushRepository::new(DatabasePool::Sqlite(pool))).await;
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn postgres_subscriptions_can_be_imported_marked_and_deleted() {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set to run the PostgreSQL tests");
    let pool = crate::infrastructure::database::init_postgres_database(&url)
        .await
        .unwrap();

    exercise(&PushRepository::new(DatabasePool::Postgres(pool))).await;
}

#[tokio::test]
async fn imported_subscriptions_keep_their_ids_capabilities_and_timestamps() {
    let dir = tempfile::tempdir().unwrap();
    let source = PushRepository::new(DatabasePool::Sqlite(
        init_database(dir.path().join("source.db")).await.unwrap(),
    ));
    let target = PushRepository::new(DatabasePool::Sqlite(
        init_database(dir.path().join("target.db")).await.unwrap(),
    ));
    source
        .import(&[
            subscription("relay-a", "iPhone"),
            subscription("relay-b", "iPad"),
        ])
        .await
        .unwrap();
    source
        .mark_success(&PushSubscription::local_id("relay-b"))
        .await
        .unwrap();
    let exported = source.list().await.unwrap();

    target.import(&exported).await.unwrap();
    target.import(&exported).await.unwrap();

    assert_eq!(target.list().await.unwrap(), exported);
    assert_eq!(target.delete_all().await.unwrap(), 2);
    assert!(target.list().await.unwrap().is_empty());
}
