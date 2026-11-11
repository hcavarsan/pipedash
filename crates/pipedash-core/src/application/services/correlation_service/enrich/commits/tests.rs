use std::sync::atomic::Ordering;

use chrono::{
    Duration,
    Utc,
};
use pipedash_plugin_api::{
    ChangedFile,
    CommitSha,
    RateLimit,
    RepoId,
};

use super::super::tests::{
    commit_info,
    sha,
    Fixture,
};
use super::{
    Cached,
    Lru,
};
use crate::application::test_support::Gate;
use crate::infrastructure::database::{
    ApplicationRow,
    ChangeRow,
};

fn repo(path: &str) -> RepoId {
    RepoId::parse(&format!("https://{path}")).unwrap()
}

fn seen(lru: &mut Lru, key: &str) -> bool {
    lru.get(key).is_some()
}

#[test]
fn lru_evicts_the_least_recently_used_entry() {
    let mut lru = Lru::new(2);
    lru.put("a".into(), Cached::Seen);
    lru.put("b".into(), Cached::Seen);
    assert!(seen(&mut lru, "a"));
    lru.put("c".into(), Cached::Seen);
    assert!(seen(&mut lru, "a"));
    assert!(!seen(&mut lru, "b"));
    assert!(seen(&mut lru, "c"));
}

#[test]
fn lru_replaces_a_key_without_growing() {
    let mut lru = Lru::new(2);
    lru.put("a".into(), Cached::Seen);
    lru.put("a".into(), Cached::Changes(Vec::new()));
    lru.put("b".into(), Cached::Seen);
    assert!(matches!(lru.get("a"), Some(Cached::Changes(_))));
    assert!(seen(&mut lru, "b"));
}

#[tokio::test]
async fn routing_follows_source_hosts_and_lowest_provider_id() {
    let f = Fixture::new().await;
    f.stub(|stub| stub.with_source_hosts(&["elsewhere.test"]))
        .await;
    let (first, _) = f.stub(|stub| stub.with_source_hosts(&["git.test"])).await;
    f.stub(|stub| stub.with_source_hosts(&["git.test"])).await;

    let lease = f.service.route(&repo("git.test/shop/api")).await.unwrap();
    assert_eq!(lease.id, first);
    assert!(f
        .service
        .route(&repo("unknown.test/shop/api"))
        .await
        .is_none());
}

#[tokio::test]
async fn provider_without_source_hosts_is_never_routed() {
    let f = Fixture::new().await;
    f.stub(|stub| stub).await;
    assert!(f.service.route(&repo("git.test/shop/api")).await.is_none());
}

#[tokio::test]
async fn repeated_lookup_hits_the_cache_instead_of_the_provider() {
    let f = Fixture::new().await;
    let (_, stub) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_commit(commit_info('a'))
        })
        .await;
    let repo = repo("git.test/shop/api");

    let first = f.service.commit_info(&repo, "aaaaaaa").await.unwrap();
    let second = f.service.commit_info(&repo, "aaaaaaa").await.unwrap();

    assert_eq!(first, second);
    assert_eq!(stub.commit_calls.load(Ordering::SeqCst), 1);
    f.service.commit_info(&repo, "bbbbbbb").await.unwrap();
    assert_eq!(stub.commit_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn changes_are_cached_per_commit() {
    let f = Fixture::new().await;
    let files = vec![ChangedFile {
        path: "deploy.yaml".into(),
        patch: Some("+image: a".into()),
    }];
    let (_, stub) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_changes(files.clone())
        })
        .await;
    let repo = repo("git.test/shop/config");
    let commit = CommitSha::parse(&sha('c')).unwrap();

    assert_eq!(
        f.service
            .changes_of(&repo, &commit)
            .await
            .map(|(found, _)| found),
        Some(files)
    );
    f.service.changes_of(&repo, &commit).await.unwrap();
    assert_eq!(stub.change_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn exhausted_quota_returns_none_without_calling_the_provider() {
    let f = Fixture::new().await;
    let (_, stub) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_commit(commit_info('a'))
        })
        .await;
    *stub.rate_limit.lock().unwrap() = Some(RateLimit {
        limit: 100,
        remaining: 1,
        reset_at: Utc::now() + Duration::hours(1),
    });

    assert!(f
        .service
        .commit_info(&repo("git.test/shop/api"), "aaaaaaa")
        .await
        .is_none());
    assert_eq!(stub.commit_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn authored_at_is_filled_through_the_cached_lookup() {
    let f = Fixture::new().await;
    let info = commit_info('a');
    let (_, stub) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_commit(info.clone())
        })
        .await;
    let change_id = seed_change(&f).await;
    let mut conn = f.service.store.conn();

    f.service.fill_authored_at().await.unwrap();

    let stored = conn.get_change(change_id).await.unwrap().unwrap();
    assert_eq!(stored.authored_at, Some(info.authored_at));
    f.service.fill_authored_at().await.unwrap();
    assert_eq!(stub.commit_calls.load(Ordering::SeqCst), 1);
}

async fn seed_change(f: &Fixture) -> i64 {
    let mut conn = f.service.store.conn();
    let application_id = conn
        .upsert_application(&ApplicationRow {
            id: 0,
            identity_kind: "unit".into(),
            identity_key: "1:api".into(),
            name: "api".into(),
            hidden: false,
            archived_at: None,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    conn.upsert_change(&ChangeRow {
        id: 0,
        application_id,
        repo: "git.test/shop/api".into(),
        commit_sha: sha('a'),
        authored_at: None,
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn a_superseded_commit_response_stores_no_authored_at() {
    let f = Fixture::new().await;
    let gate = Gate::new();
    let (provider, _) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_commit(commit_info('a'))
                .source_gated(&gate)
        })
        .await;
    let change_id = seed_change(&f).await;
    let service = f.service.clone();
    let pass = tokio::spawn(async move { service.fill_authored_at().await });
    gate.arrived().await;
    f.h.provider_service
        .remove_provider(provider)
        .await
        .unwrap();
    gate.open();
    pass.await.unwrap().unwrap();

    let stored = f
        .service
        .store
        .conn()
        .get_change(change_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.authored_at, None);
}

#[tokio::test]
async fn a_removed_provider_cached_answers_are_not_served() {
    let f = Fixture::new().await;
    let (provider, _) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_commit(commit_info('a'))
        })
        .await;
    let repo = repo("git.test/shop/api");
    assert!(f.service.commit_info(&repo, "aaaaaaa").await.is_some());

    f.h.provider_service
        .remove_provider(provider)
        .await
        .unwrap();

    assert!(f.service.commit_info(&repo, "aaaaaaa").await.is_none());
}
