use super::DevicePushRepository;
use crate::infrastructure::database::{
    init_database,
    DatabasePool,
};

#[tokio::test]
async fn sqlite_pushes_upsert_evict_duplicates_and_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let repository = DevicePushRepository::new(DatabasePool::Sqlite(
        init_database(dir.path().join("pushes.db")).await.unwrap(),
    ));

    repository.set("d1", "sub-1").await.unwrap();
    repository.mark_success("d1").await.unwrap();
    assert!(repository
        .get("d1")
        .await
        .unwrap()
        .unwrap()
        .last_success_at
        .is_some());

    repository.set("d1", "sub-2").await.unwrap();
    let replaced = repository.get("d1").await.unwrap().unwrap();
    assert_eq!(replaced.relay_subscription_id, "sub-2");
    assert_eq!(replaced.last_success_at, None);

    repository.set("d2", "sub-2").await.unwrap();
    assert_eq!(repository.get("d1").await.unwrap(), None);
    let listed = repository.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].device_id, "d2");

    repository.import(&listed).await.unwrap();
    repository.import(&listed).await.unwrap();
    assert_eq!(repository.list().await.unwrap(), listed);

    assert!(repository.delete("d2").await.unwrap());
    assert!(!repository.delete("d2").await.unwrap());
    repository.set("d3", "sub-3").await.unwrap();
    assert_eq!(repository.delete_all().await.unwrap(), 1);
    assert!(repository.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn sqlite_delete_matching_needs_both_ids_to_match() {
    let dir = tempfile::tempdir().unwrap();
    let repository = DevicePushRepository::new(DatabasePool::Sqlite(
        init_database(dir.path().join("matching.db")).await.unwrap(),
    ));
    repository.set("d1", "sub-1").await.unwrap();

    assert!(!repository.delete_matching("d1", "sub-2").await.unwrap());
    assert!(!repository.delete_matching("d2", "sub-1").await.unwrap());
    assert!(repository.get("d1").await.unwrap().is_some());
    assert!(repository.delete_matching("d1", "sub-1").await.unwrap());
    assert_eq!(repository.get("d1").await.unwrap(), None);
}

#[tokio::test]
async fn sqlite_undelivered_seq_keeps_the_lowest_and_clears_through_the_acked_one() {
    let dir = tempfile::tempdir().unwrap();
    let repository = DevicePushRepository::new(DatabasePool::Sqlite(
        init_database(dir.path().join("undelivered.db"))
            .await
            .unwrap(),
    ));
    async fn seq(repository: &DevicePushRepository) -> Option<i64> {
        repository.list().await.unwrap()[0].undelivered_seq
    }
    repository.set("d1", "sub-1").await.unwrap();
    assert_eq!(seq(&repository).await, None);

    repository.mark_undelivered("d1", "other", 5).await.unwrap();
    repository.mark_undelivered("d2", "sub-1", 5).await.unwrap();
    assert_eq!(seq(&repository).await, None);

    repository.mark_undelivered("d1", "sub-1", 7).await.unwrap();
    repository.mark_undelivered("d1", "sub-1", 9).await.unwrap();
    repository.mark_undelivered("d1", "sub-1", 4).await.unwrap();
    assert_eq!(seq(&repository).await, Some(4));

    repository.set("d1", "sub-2").await.unwrap();
    assert_eq!(seq(&repository).await, Some(4));

    repository.ack_undelivered("d2", 100).await.unwrap();
    repository.ack_undelivered("d1", 3).await.unwrap();
    assert_eq!(seq(&repository).await, Some(4));
    repository.ack_undelivered("d1", 4).await.unwrap();
    assert_eq!(seq(&repository).await, Some(5));
    repository.ack_undelivered("d1", 9).await.unwrap();
    assert_eq!(seq(&repository).await, None);

    repository.mark_undelivered("d1", "sub-2", 8).await.unwrap();
    let exported = repository.list().await.unwrap();
    repository.delete_all().await.unwrap();
    repository.import(&exported).await.unwrap();
    assert_eq!(seq(&repository).await, Some(8));
    assert_eq!(
        repository.list().await.unwrap()[0].undelivered_max_seq,
        Some(8)
    );
}

#[tokio::test]
async fn sqlite_a_partial_ack_keeps_the_rest_of_the_failed_alerts() {
    let dir = tempfile::tempdir().unwrap();
    let repository = DevicePushRepository::new(DatabasePool::Sqlite(
        init_database(dir.path().join("partial.db")).await.unwrap(),
    ));
    async fn range(repository: &DevicePushRepository) -> (Option<i64>, Option<i64>) {
        let row = &repository.list().await.unwrap()[0];
        (row.undelivered_seq, row.undelivered_max_seq)
    }
    repository.set("d1", "sub-1").await.unwrap();
    for seq in 1..=25 {
        repository
            .mark_undelivered("d1", "sub-1", seq)
            .await
            .unwrap();
    }
    assert_eq!(range(&repository).await, (Some(1), Some(25)));

    repository.ack_undelivered("d1", 20).await.unwrap();
    assert_eq!(range(&repository).await, (Some(21), Some(25)));

    repository
        .mark_undelivered("d1", "sub-1", 30)
        .await
        .unwrap();
    assert_eq!(range(&repository).await, (Some(21), Some(30)));

    repository.ack_undelivered("d1", 25).await.unwrap();
    assert_eq!(range(&repository).await, (Some(26), Some(30)));

    repository.ack_undelivered("d1", 30).await.unwrap();
    assert_eq!(range(&repository).await, (None, None));
}

#[tokio::test]
async fn sqlite_import_overwrites_a_stale_registration_for_the_same_device() {
    let dir = tempfile::tempdir().unwrap();
    let source = DevicePushRepository::new(DatabasePool::Sqlite(
        init_database(dir.path().join("source.db")).await.unwrap(),
    ));
    let target = DevicePushRepository::new(DatabasePool::Sqlite(
        init_database(dir.path().join("target.db")).await.unwrap(),
    ));
    target.set("d1", "stale").await.unwrap();
    target.set("d2", "taken").await.unwrap();
    source.set("d1", "fresh").await.unwrap();
    source.set("d3", "taken").await.unwrap();

    target.import(&source.list().await.unwrap()).await.unwrap();

    assert_eq!(
        target
            .get("d1")
            .await
            .unwrap()
            .unwrap()
            .relay_subscription_id,
        "fresh"
    );
    assert_eq!(target.get("d2").await.unwrap(), None);
    assert_eq!(
        target
            .get("d3")
            .await
            .unwrap()
            .unwrap()
            .relay_subscription_id,
        "taken"
    );
}
