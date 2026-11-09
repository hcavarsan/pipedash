use std::collections::{
    HashMap,
    HashSet,
};
use std::sync::Arc;

use bytes::Bytes;

use super::*;
use crate::domain::{
    LogNodeKind,
    PipelineStatus,
    PlainDialect,
    ProviderConfig,
    RunLogNode,
    RunLogTree,
};
use crate::infrastructure::config::PipedashConfig;
use crate::infrastructure::migration::MigrationOptions;
use crate::infrastructure::run_logs::{
    BlockLimits,
    DocKey,
    Ingester,
    LogStore,
    PublishOutcome,
    StoredTree,
};
use crate::infrastructure::token_store::MemoryTokenStore;
use crate::test_env::EnvGuard;

async fn prepare_document(
    store: &Arc<LogStore>, key: &DocKey, complete: bool,
) -> (std::path::PathBuf, i64) {
    let revision = store.allocate_revision();
    let dir = store.tmp_dir().await.unwrap();
    let ingester = Ingester {
        key: key.clone(),
        revision,
        complete,
        dir: dir.clone(),
        dialect: &PlainDialect,
        steps: Vec::new(),
        received: Arc::default(),
        limits: BlockLimits::default(),
        base: None,
        restart: Arc::default(),
    };
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    tx.send(Bytes::from_static(b"retained build output\nsecond line\n"))
        .await
        .unwrap();
    drop(tx);
    tokio::task::spawn_blocking(move || ingester.run(&mut rx).unwrap().finish(None).unwrap())
        .await
        .unwrap();
    (dir, revision)
}

async fn publish_document(store: &Arc<LogStore>, key: &DocKey, complete: bool) -> i64 {
    let (dir, revision) = prepare_document(store, key, complete).await;
    assert_eq!(
        store
            .publish(
                key,
                &dir,
                revision,
                store.epoch(key.provider_id),
                None,
                HashSet::new()
            )
            .await
            .unwrap(),
        PublishOutcome::Published
    );
    revision
}

async fn assert_document(store: &Arc<LogStore>, key: &DocKey, revision: i64) {
    assert_eq!(store.current(key), Some(revision));
    let (document, _lease) = store.lease(key, revision).await.unwrap().unwrap();
    assert_eq!(&document.meta().key, key);
    assert!(document.meta().complete);
    let page = document
        .read_lines(store.block_cache(), 0, 2, 4096)
        .unwrap();
    assert_eq!(
        page.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        ["retained build output", "second line"]
    );
}

fn cached_tree(key: &DocKey) -> StoredTree {
    StoredTree {
        log_tree_version: 7,
        run_status: PipelineStatus::Success,
        run_concluded_at: Some(chrono::Utc::now()),
        tree: RunLogTree {
            nodes: vec![RunLogNode {
                id: key.node_id.clone(),
                parent_id: None,
                kind: LogNodeKind::Job,
                name: "Build".into(),
                status: PipelineStatus::Success,
                started_at: None,
                finished_at: None,
                duration_seconds: None,
                log: None,
                web_url: None,
                depth: 0,
                navigable: true,
            }],
            default_node_id: Some(key.node_id.clone()),
            complete: true,
            groups: Vec::new(),
            step_ids_by_node: Default::default(),
        },
    }
}

async fn migrate_downloaded_logs(migrate_cache: bool) {
    let root = tempfile::tempdir().unwrap();
    let config_path = root.path().join("config.toml");
    std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
    let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);
    let mut source = PipedashConfig::default();
    source.storage.data_dir = root.path().join("source").display().to_string();
    let mut target = source.clone();
    target.storage.data_dir = root.path().join("target").display().to_string();
    let tokens = Arc::new(MemoryTokenStore::new());
    let manager = StorageManager::with_token_store(source.clone(), tokens.clone(), true)
        .await
        .unwrap();
    let backend = manager.config_backend();
    let provider = |name: &str| ProviderConfig {
        id: None,
        name: name.into(),
        provider_type: "stub".into(),
        token: String::new(),
        config: HashMap::new(),
        refresh_interval: 30,
        version: None,
    };
    let deleted = backend.create_provider(&provider("deleted")).await.unwrap();
    let retained = backend
        .create_provider(&provider("retained"))
        .await
        .unwrap();
    backend.delete_provider(deleted).await.unwrap();
    manager
        .cache_backend()
        .put("pipeline-cache", b"cached pipelines", None)
        .await
        .unwrap();
    let source_store = LogStore::open(source.cache_dir().join("run-logs"), u64::MAX)
        .await
        .unwrap();
    let key = DocKey {
        provider_id: retained,
        pipeline_id: format!("stub__{retained}__build__nested"),
        run_number: 42,
        node_id: "job/step".into(),
    };
    let revision = publish_document(&source_store, &key, true).await;
    let archived = DocKey {
        node_id: "archived-job".into(),
        ..key.clone()
    };
    let archived_revision = publish_document(&source_store, &archived, true).await;
    let incomplete = DocKey {
        node_id: "running".into(),
        ..key.clone()
    };
    let incomplete_revision = publish_document(&source_store, &incomplete, false).await;
    let temporary = DocKey {
        node_id: "unpublished".into(),
        ..key.clone()
    };
    let (temporary_dir, _) = prepare_document(&source_store, &temporary, true).await;
    let orphan = DocKey {
        provider_id: deleted,
        pipeline_id: format!("stub__{deleted}__build"),
        ..key.clone()
    };
    publish_document(&source_store, &orphan, true).await;
    let tree = cached_tree(&key);
    source_store
        .write_tree(
            retained,
            &key.pipeline_id,
            key.run_number,
            &tree,
            source_store.epoch(retained),
        )
        .await
        .unwrap();
    assert_document(&source_store, &key, revision).await;
    drop(source_store);
    let orchestrator =
        MigrationOrchestrator::from_manager(&manager, None, Some(tokens.clone())).await;
    let options = MigrationOptions {
        migrate_cache,
        ..Default::default()
    };
    let plan = orchestrator
        .plan_migration(target.clone(), &options)
        .unwrap();
    let result = orchestrator.execute_migration(plan, options, true).await;
    assert!(result.success, "{:?}", result.errors);
    let target_id = result.provider_id_mapping[&retained];
    assert_ne!(retained, target_id);
    assert!(temporary_dir.exists());
    let target_store = LogStore::open(target.cache_dir().join("run-logs"), u64::MAX)
        .await
        .unwrap();
    let target_key = DocKey {
        provider_id: target_id,
        pipeline_id: format!("stub__{target_id}__build__nested"),
        ..key.clone()
    };
    let target_archived = DocKey {
        node_id: archived.node_id.clone(),
        ..target_key.clone()
    };
    if migrate_cache {
        assert_document(&target_store, &target_key, revision).await;
        assert_document(&target_store, &target_archived, archived_revision).await;
        let actual = target_store
            .read_tree(target_id, &target_key.pipeline_id, key.run_number)
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(&tree).unwrap()
        );
        assert_eq!(result.stats.cache_entries_migrated, 4);
        let target_manager = StorageManager::with_token_store(target, tokens, true)
            .await
            .unwrap();
        assert_eq!(
            target_manager
                .cache_backend()
                .get("pipeline-cache")
                .await
                .unwrap(),
            b"cached pipelines"
        );
    } else {
        assert_eq!(target_store.current(&target_key), None);
        assert_eq!(target_store.current(&target_archived), None);
        assert!(target_store
            .read_tree(target_id, &target_key.pipeline_id, key.run_number)
            .await
            .is_none());
        assert_eq!(result.stats.cache_entries_migrated, 0);
    }
    assert_eq!(target_store.current(&key), None);
    for node_id in [incomplete.node_id.as_str(), temporary.node_id.as_str()] {
        assert_eq!(
            target_store.current(&DocKey {
                node_id: node_id.into(),
                ..target_key.clone()
            }),
            None
        );
    }
    assert_eq!(
        target_store.current(&DocKey {
            provider_id: target_id,
            pipeline_id: format!("stub__{target_id}__build"),
            ..orphan.clone()
        }),
        None
    );
    drop(target_store);
    let source_store = LogStore::open(source.cache_dir().join("run-logs"), u64::MAX)
        .await
        .unwrap();
    assert_document(&source_store, &key, revision).await;
    assert_document(&source_store, &archived, archived_revision).await;
    assert_eq!(source_store.current(&incomplete), Some(incomplete_revision));
    assert!(source_store.current(&orphan).is_some());
}

#[tokio::test]
async fn migration_keeps_completed_logs_and_trees_with_remapped_ids() {
    migrate_downloaded_logs(true).await;
}

#[tokio::test]
async fn migration_without_cache_leaves_downloaded_logs_at_source() {
    migrate_downloaded_logs(false).await;
}

#[tokio::test]
async fn migrating_logs_to_the_same_cache_preserves_documents() {
    let root = tempfile::tempdir().unwrap();
    let store = LogStore::open(root.path().to_path_buf(), u64::MAX)
        .await
        .unwrap();
    let key = DocKey {
        provider_id: 3,
        pipeline_id: "stub__3__build".into(),
        run_number: 42,
        node_id: "job".into(),
    };
    let revision = publish_document(&store, &key, true).await;
    let (temporary, _) = prepare_document(&store, &key, true).await;
    let count = LogStore::transfer_completed(
        root.path().to_path_buf(),
        root.path().join("."),
        &HashMap::from([(3, 3)]),
        |key| Some(key.clone()),
    )
    .await
    .unwrap();
    assert_eq!(count, 0);
    assert_document(&store, &key, revision).await;
    assert!(temporary.exists());
}

#[tokio::test]
async fn migrating_logs_in_place_remaps_keys_without_removing_source() {
    let root = tempfile::tempdir().unwrap();
    let store = LogStore::open(root.path().to_path_buf(), u64::MAX)
        .await
        .unwrap();
    let key = DocKey {
        provider_id: 3,
        pipeline_id: "stub__3__build".into(),
        run_number: 42,
        node_id: "job".into(),
    };
    let revision = publish_document(&store, &key, true).await;
    let target_key = DocKey {
        provider_id: 8,
        pipeline_id: "stub__8__build".into(),
        ..key.clone()
    };
    let count = LogStore::transfer_completed(
        root.path().to_path_buf(),
        root.path().to_path_buf(),
        &HashMap::from([(3, 8)]),
        |_| Some(target_key.clone()),
    )
    .await
    .unwrap();
    assert_eq!(count, 1);
    assert_document(&store, &key, revision).await;
    assert_document(&store, &target_key, revision).await;
}

#[tokio::test]
async fn log_transfer_reports_target_errors_without_removing_source() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    let store = LogStore::open(source.clone(), u64::MAX).await.unwrap();
    let key = DocKey {
        provider_id: 3,
        pipeline_id: "stub__3__build".into(),
        run_number: 42,
        node_id: "job".into(),
    };
    let revision = publish_document(&store, &key, true).await;
    std::fs::write(&target, b"not a cache directory").unwrap();
    assert!(
        LogStore::transfer_completed(source, target, &HashMap::from([(3, 3)]), |key| Some(
            key.clone()
        ))
        .await
        .is_err()
    );
    assert_document(&store, &key, revision).await;
}

#[tokio::test]
async fn migration_pins_completed_documents_while_source_cleanup_runs() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    let store = LogStore::open(source.clone(), u64::MAX).await.unwrap();
    let key = DocKey {
        provider_id: 3,
        pipeline_id: "stub__3__build".into(),
        run_number: 42,
        node_id: "job".into(),
    };
    let revision = publish_document(&store, &key, true).await;
    let handle = tokio::runtime::Handle::current();
    let count =
        LogStore::transfer_completed(source, target.clone(), &HashMap::from([(3, 3)]), |key| {
            store.mark_corrupt(key, revision);
            std::thread::scope(|scope| {
                scope
                    .spawn(|| handle.block_on(store.sweep()))
                    .join()
                    .unwrap()
                    .unwrap();
            });
            Some(key.clone())
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
    let target_store = LogStore::open(target, u64::MAX).await.unwrap();
    assert_document(&target_store, &key, revision).await;
}

#[tokio::test]
async fn migration_transfers_a_tree_without_downloaded_documents() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    let store = LogStore::open(source.clone(), u64::MAX).await.unwrap();
    let key = DocKey {
        provider_id: 3,
        pipeline_id: "stub__3__build".into(),
        run_number: 42,
        node_id: "job".into(),
    };
    let tree = cached_tree(&key);
    store
        .write_tree(3, &key.pipeline_id, 42, &tree, store.epoch(3))
        .await
        .unwrap();
    let mapped = DocKey {
        provider_id: 8,
        pipeline_id: "stub__8__build".into(),
        ..key
    };
    let count =
        LogStore::transfer_completed(source, target.clone(), &HashMap::from([(3, 8)]), |_| {
            Some(mapped.clone())
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
    let target_store = LogStore::open(target, u64::MAX).await.unwrap();
    let actual = target_store
        .read_tree(8, &mapped.pipeline_id, 42)
        .await
        .unwrap();
    assert_eq!(actual.tree, tree.tree);
    assert_eq!(actual.log_tree_version, 7);
}

#[tokio::test]
async fn migration_keeps_readable_logs_when_the_cached_tree_is_damaged() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    let store = LogStore::open(source.clone(), u64::MAX).await.unwrap();
    let key = DocKey {
        provider_id: 3,
        pipeline_id: "stub__3__build".into(),
        run_number: 42,
        node_id: "job".into(),
    };
    let revision = publish_document(&store, &key, true).await;
    store
        .write_tree(3, &key.pipeline_id, 42, &cached_tree(&key), store.epoch(3))
        .await
        .unwrap();
    tokio::fs::write(
        source.join("v1/3").join(key.run_key()).join("tree.json"),
        b"invalid JSON",
    )
    .await
    .unwrap();
    let count =
        LogStore::transfer_completed(source, target.clone(), &HashMap::from([(3, 3)]), |key| {
            Some(key.clone())
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
    let target_store = LogStore::open(target, u64::MAX).await.unwrap();
    assert_document(&target_store, &key, revision).await;
    assert!(target_store
        .read_tree(3, &key.pipeline_id, 42)
        .await
        .is_none());
}

#[tokio::test]
async fn migration_remaps_legacy_trees_using_downloaded_log_identity() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    let store = LogStore::open(source.clone(), u64::MAX).await.unwrap();
    let key = DocKey {
        provider_id: 3,
        pipeline_id: "stub__3__build".into(),
        run_number: 42,
        node_id: "job".into(),
    };
    let revision = publish_document(&store, &key, true).await;
    let tree = cached_tree(&key);
    store
        .write_tree(3, &key.pipeline_id, 42, &tree, store.epoch(3))
        .await
        .unwrap();
    tokio::fs::write(
        source.join("v1/3").join(key.run_key()).join("tree.json"),
        serde_json::to_vec(&tree).unwrap(),
    )
    .await
    .unwrap();
    let count =
        LogStore::transfer_completed(source, target.clone(), &HashMap::from([(3, 8)]), |key| {
            Some(DocKey {
                provider_id: 8,
                pipeline_id: "stub__8__build".into(),
                ..key.clone()
            })
        })
        .await
        .unwrap();
    assert_eq!(count, 2);
    let target_store = LogStore::open(target, u64::MAX).await.unwrap();
    let mapped = DocKey {
        provider_id: 8,
        pipeline_id: "stub__8__build".into(),
        ..key
    };
    assert_document(&target_store, &mapped, revision).await;
    assert_eq!(
        target_store
            .read_tree(8, &mapped.pipeline_id, 42)
            .await
            .unwrap()
            .tree,
        tree.tree
    );
}

#[tokio::test]
async fn migration_preserves_legacy_tree_only_runs_with_unchanged_provider_ids() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    let store = LogStore::open(source.clone(), u64::MAX).await.unwrap();
    let key = DocKey {
        provider_id: 3,
        pipeline_id: "stub__3__build".into(),
        run_number: 42,
        node_id: "job".into(),
    };
    let tree = cached_tree(&key);
    store
        .write_tree(3, &key.pipeline_id, 42, &tree, store.epoch(3))
        .await
        .unwrap();
    tokio::fs::write(
        source.join("v1/3").join(key.run_key()).join("tree.json"),
        serde_json::to_vec(&tree).unwrap(),
    )
    .await
    .unwrap();
    let count =
        LogStore::transfer_completed(source, target.clone(), &HashMap::from([(3, 3)]), |key| {
            Some(key.clone())
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
    let target_store = LogStore::open(target, u64::MAX).await.unwrap();
    assert_eq!(
        target_store
            .read_tree(3, &key.pipeline_id, 42)
            .await
            .unwrap()
            .tree,
        tree.tree
    );
}

#[tokio::test]
async fn migration_refuses_to_lose_unidentified_legacy_trees_when_ids_change() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    let store = LogStore::open(source.clone(), u64::MAX).await.unwrap();
    let key = DocKey {
        provider_id: 3,
        pipeline_id: "stub__3__build".into(),
        run_number: 42,
        node_id: "job".into(),
    };
    let tree = cached_tree(&key);
    store
        .write_tree(3, &key.pipeline_id, 42, &tree, store.epoch(3))
        .await
        .unwrap();
    tokio::fs::write(
        source.join("v1/3").join(key.run_key()).join("tree.json"),
        serde_json::to_vec(&tree).unwrap(),
    )
    .await
    .unwrap();
    let result = LogStore::transfer_completed(source, target, &HashMap::from([(3, 8)]), |key| {
        Some(DocKey {
            provider_id: 8,
            pipeline_id: "stub__8__build".into(),
            ..key.clone()
        })
    })
    .await;
    assert!(matches!(
        result,
        Err(crate::domain::DomainError::InvalidConfig(_))
    ));
    assert_eq!(
        store.read_tree(3, &key.pipeline_id, 42).await.unwrap().tree,
        tree.tree
    );
}

#[tokio::test]
async fn migration_uses_the_current_readable_revision_without_cleaning_the_source() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let target = root.path().join("target");
    let store = LogStore::open(source.clone(), u64::MAX).await.unwrap();
    let key = DocKey {
        provider_id: 3,
        pipeline_id: "stub__3__build".into(),
        run_number: 42,
        node_id: "job".into(),
    };
    let revision = publish_document(&store, &key, true).await;
    let mut docs = tokio::fs::read_dir(source.join("v1/3").join(key.run_key()).join("docs"))
        .await
        .unwrap();
    let doc = docs.next_entry().await.unwrap().unwrap().path();
    let damaged = doc.join((revision + 1).to_string());
    tokio::fs::create_dir_all(&damaged).await.unwrap();
    tokio::fs::write(damaged.join("meta.json"), b"invalid JSON")
        .await
        .unwrap();
    drop(store);
    let count =
        LogStore::transfer_completed(source, target.clone(), &HashMap::from([(3, 3)]), |key| {
            Some(key.clone())
        })
        .await
        .unwrap();
    assert_eq!(count, 1);
    let target_store = LogStore::open(target, u64::MAX).await.unwrap();
    assert_document(&target_store, &key, revision).await;
    assert!(damaged.exists());
}
