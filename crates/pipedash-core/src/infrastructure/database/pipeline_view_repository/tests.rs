use super::{
    PipelineViewRepository,
    StoredMember,
    StoredPin,
    StoredView,
};
use crate::domain::{
    PinKind,
    SectionMode,
    SortKey,
    ViewDisplay,
    ViewKind,
    ViewLayout,
    ViewRules,
};
use crate::infrastructure::database::{
    init_database,
    DatabasePool,
};

fn kinds(views: &[StoredView]) -> Vec<ViewKind> {
    views.iter().map(|g| g.kind).collect()
}

async fn view_named(repo: &PipelineViewRepository, name: &str) -> StoredView {
    let views = repo.list_views().await.unwrap();
    views.into_iter().find(|v| v.name == name).unwrap()
}

async fn insert_plain(repo: &PipelineViewRepository, name: &str, parent: Option<i64>) -> i64 {
    repo.insert_custom(name, parent, &ViewRules::default(), &ViewDisplay::default())
        .await
        .unwrap()
}

async fn exercise(repo: &PipelineViewRepository) {
    repo.reset().await.unwrap();
    let views = repo.list_views().await.unwrap();
    let mut seeded = kinds(&views);
    seeded.sort_by_key(|k| k.as_str());
    assert_eq!(
        seeded,
        vec![ViewKind::All, ViewKind::Hidden, ViewKind::Pinned]
    );
    let pinned = views.iter().find(|g| g.kind == ViewKind::Pinned).unwrap();
    assert_eq!(pinned.display, ViewDisplay::pinned_default());

    let rules = ViewRules {
        repositories: vec!["acme/*".into()],
        ..Default::default()
    };
    let release = repo
        .insert_custom("Release", None, &rules, &ViewDisplay::default())
        .await
        .unwrap();
    let other = repo
        .insert_custom(
            "Other",
            None,
            &ViewRules::default(),
            &ViewDisplay::default(),
        )
        .await
        .unwrap();
    let views = repo.list_views().await.unwrap();
    let stored = views.iter().find(|g| g.id == release).unwrap();
    assert_eq!(stored.rules, rules);
    assert!(stored.position > pinned.position);

    let display = ViewDisplay {
        sort: SortKey::Status,
        sections: SectionMode::Provider,
        ..Default::default()
    };
    assert!(repo
        .update_view(release, "Releases", None, &rules, &display)
        .await
        .unwrap());
    repo.set_positions(&[(other, 0), (pinned.id, 1), (release, 2)])
        .await
        .unwrap();
    let views = repo.list_views().await.unwrap();
    let movable: Vec<i64> = views
        .iter()
        .filter(|g| matches!(g.kind, ViewKind::Pinned | ViewKind::Custom))
        .map(|g| g.id)
        .collect();
    assert_eq!(movable, vec![other, pinned.id, release]);
    let updated = views.iter().find(|g| g.id == release).unwrap();
    assert_eq!(updated.name, "Releases");
    assert_eq!(updated.display, display);

    repo.upsert_member(release, 1, "stub__1__a", false)
        .await
        .unwrap();
    repo.upsert_member(release, 1, "stub__1__b", true)
        .await
        .unwrap();
    repo.upsert_member(release, 1, "stub__1__b", false)
        .await
        .unwrap();
    let members: Vec<(String, bool)> = repo
        .list_members()
        .await
        .unwrap()
        .into_iter()
        .filter(|m| m.view_id == release)
        .map(|m| (m.pipeline_id, m.excluded))
        .collect();
    assert_eq!(
        members,
        vec![
            ("stub__1__a".to_string(), false),
            ("stub__1__b".to_string(), false)
        ]
    );
    repo.upsert_member(release, 1, "stub__1__c", true)
        .await
        .unwrap();

    repo.replace_manual_members(
        release,
        &[(1, "stub__1__c".to_string()), (2, "stub__2__d".to_string())],
        &[],
    )
    .await
    .unwrap();
    let members: Vec<(String, bool)> = repo
        .list_members()
        .await
        .unwrap()
        .into_iter()
        .filter(|m| m.view_id == release)
        .map(|m| (m.pipeline_id, m.excluded))
        .collect();
    assert_eq!(
        members,
        vec![
            ("stub__1__c".to_string(), false),
            ("stub__2__d".to_string(), false)
        ]
    );
    assert!(repo.delete_member(release, "stub__2__d").await.unwrap());
    assert!(!repo.delete_member(release, "stub__2__d").await.unwrap());

    repo.upsert_member(release, 2, "stub__2__a", false)
        .await
        .unwrap();
    repo.remove_provider(2).await.unwrap();
    assert!(repo
        .list_members()
        .await
        .unwrap()
        .iter()
        .all(|m| m.provider_id != 2));

    let exported = repo.export().await.unwrap();
    repo.import(&exported, true).await.unwrap();
    let reimported = repo.export().await.unwrap();
    assert_eq!(reimported.views.len(), exported.views.len());
    assert_eq!(reimported.members.len(), exported.members.len());

    let child = repo
        .insert_custom(
            "Child",
            Some(view_named(repo, "Releases").await.id),
            &ViewRules::default(),
            &ViewDisplay::default(),
        )
        .await
        .unwrap();
    repo.insert_custom(
        "Grand",
        Some(child),
        &ViewRules::default(),
        &ViewDisplay::default(),
    )
    .await
    .unwrap();

    let nested = repo.export().await.unwrap();
    repo.import(&nested, true).await.unwrap();
    let release = view_named(repo, "Releases").await.id;
    let child = view_named(repo, "Child").await;
    assert_eq!(child.parent_id, Some(release));
    assert_eq!(view_named(repo, "Grand").await.parent_id, Some(child.id));
    assert_eq!(view_named(repo, "Releases").await.parent_id, None);

    assert!(repo.delete_custom(child.id).await.unwrap());
    assert_eq!(view_named(repo, "Grand").await.parent_id, Some(release));

    let current = repo
        .list_views()
        .await
        .unwrap()
        .into_iter()
        .find(|g| g.name == "Other")
        .unwrap();
    assert!(repo.delete_custom(current.id).await.unwrap());
    assert!(!repo.delete_custom(pinned.id).await.unwrap());

    repo.reset().await.unwrap();
    let mut remaining = kinds(&repo.list_views().await.unwrap());
    remaining.sort_by_key(|k| k.as_str());
    assert_eq!(
        remaining,
        vec![ViewKind::All, ViewKind::Hidden, ViewKind::Pinned]
    );
    assert!(repo.list_members().await.unwrap().is_empty());
    let pinned = repo
        .list_views()
        .await
        .unwrap()
        .into_iter()
        .find(|g| g.kind == ViewKind::Pinned)
        .unwrap();
    assert_eq!(pinned.display, ViewDisplay::pinned_default());
}

#[tokio::test]
async fn sqlite_views_seed_system_rows_and_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_database(dir.path().join("views.db")).await.unwrap();

    exercise(&PipelineViewRepository::new(DatabasePool::Sqlite(pool))).await;
}

async fn import_merges_or_replaces(
    source: &PipelineViewRepository, target: &PipelineViewRepository,
) {
    source.reset().await.unwrap();
    target.reset().await.unwrap();
    let release = source
        .insert_custom(
            "Release",
            None,
            &ViewRules::default(),
            &ViewDisplay::default(),
        )
        .await
        .unwrap();
    source
        .upsert_member(release, 1, "stub__1__a", false)
        .await
        .unwrap();
    source
        .insert_pin(PinKind::View, &release.to_string())
        .await
        .unwrap();
    source
        .insert_pin(PinKind::Repository, "acme/app")
        .await
        .unwrap();

    let mine = target
        .insert_custom("Mine", None, &ViewRules::default(), &ViewDisplay::default())
        .await
        .unwrap();
    target
        .upsert_member(mine, 1, "stub__1__b", false)
        .await
        .unwrap();
    let same = target
        .insert_custom(
            "release",
            None,
            &ViewRules::default(),
            &ViewDisplay::default(),
        )
        .await
        .unwrap();
    target
        .upsert_member(same, 1, "stub__1__c", false)
        .await
        .unwrap();
    target
        .insert_pin(PinKind::Repository, "acme/web")
        .await
        .unwrap();
    let all = source
        .list_views()
        .await
        .unwrap()
        .into_iter()
        .find(|v| v.kind == ViewKind::All)
        .unwrap();
    let cards = ViewDisplay {
        layout: ViewLayout::Cards,
        ..all.display
    };
    source
        .update_view(all.id, &all.name, None, &all.rules, &cards)
        .await
        .unwrap();

    let export = source.export().await.unwrap();
    let members_of = |members: &[StoredMember], view: i64| {
        let mut ids: Vec<String> = members
            .iter()
            .filter(|m| m.view_id == view)
            .map(|m| m.pipeline_id.clone())
            .collect();
        ids.sort();
        ids
    };
    let pins = |pins: Vec<StoredPin>| -> Vec<(PinKind, String)> {
        pins.into_iter().map(|p| (p.kind, p.reference)).collect()
    };

    target.import(&export, false).await.unwrap();
    let views = target.list_views().await.unwrap();
    let customs: Vec<&str> = views
        .iter()
        .filter(|v| v.kind == ViewKind::Custom)
        .map(|v| v.name.as_str())
        .collect();
    assert_eq!(customs, vec!["Mine", "release", "Release (2)"]);
    let imported = view_named(target, "Release (2)").await.id;
    let members = target.list_members().await.unwrap();
    assert_eq!(members_of(&members, mine), vec!["stub__1__b"]);
    assert_eq!(members_of(&members, same), vec!["stub__1__c"]);
    assert_eq!(members_of(&members, imported), vec!["stub__1__a"]);
    assert_eq!(
        pins(target.list_pins().await.unwrap()),
        vec![
            (PinKind::Repository, "acme/web".to_string()),
            (PinKind::View, imported.to_string()),
            (PinKind::Repository, "acme/app".to_string()),
        ]
    );
    assert_eq!(view_named(target, "All pipelines").await.display, cards);

    target.import(&export, true).await.unwrap();
    let views = target.list_views().await.unwrap();
    let release = views.iter().find(|v| v.kind == ViewKind::Custom).unwrap();
    assert_eq!(
        views.iter().filter(|v| v.kind == ViewKind::Custom).count(),
        1
    );
    assert_eq!(release.name, "Release");
    assert_eq!(
        members_of(&target.list_members().await.unwrap(), release.id),
        vec!["stub__1__a"]
    );
    assert_eq!(
        pins(target.list_pins().await.unwrap()),
        vec![
            (PinKind::View, release.id.to_string()),
            (PinKind::Repository, "acme/app".to_string()),
        ]
    );
}

#[tokio::test]
async fn sqlite_import_keeps_target_views_unless_cleaning() {
    let dir = tempfile::tempdir().unwrap();
    let source = init_database(dir.path().join("source.db")).await.unwrap();
    let target = init_database(dir.path().join("target.db")).await.unwrap();

    import_merges_or_replaces(
        &PipelineViewRepository::new(DatabasePool::Sqlite(source)),
        &PipelineViewRepository::new(DatabasePool::Sqlite(target)),
    )
    .await;
}

#[tokio::test]
async fn sqlite_import_keeps_source_tree_depth_beside_same_named_target_view() {
    let dir = tempfile::tempdir().unwrap();
    let source = PipelineViewRepository::new(DatabasePool::Sqlite(
        init_database(dir.path().join("source.db")).await.unwrap(),
    ));
    let target = PipelineViewRepository::new(DatabasePool::Sqlite(
        init_database(dir.path().join("target.db")).await.unwrap(),
    ));
    let a = insert_plain(&target, "A", None).await;
    let b = insert_plain(&target, "B", Some(a)).await;
    let target_x = insert_plain(&target, "X", Some(b)).await;
    let x = insert_plain(&source, "X", None).await;
    let y = insert_plain(&source, "Y", Some(x)).await;
    insert_plain(&source, "Z", Some(y)).await;

    target
        .import(&source.export().await.unwrap(), false)
        .await
        .unwrap();

    let views = target.list_views().await.unwrap();
    let parent_of = |name: &str| views.iter().find(|v| v.name == name).unwrap().parent_id;
    let id_of = |name: &str| views.iter().find(|v| v.name == name).unwrap().id;
    assert_eq!(views.iter().filter(|v| v.name == "X").count(), 1);
    assert_eq!(id_of("X"), target_x);
    assert_eq!(parent_of("X"), Some(b));
    assert_eq!(parent_of("X (2)"), None);
    assert_eq!(parent_of("Y"), Some(id_of("X (2)")));
    assert_eq!(parent_of("Z"), Some(id_of("Y")));
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL"]
async fn postgres_views_seed_system_rows_and_round_trip() {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set to run the PostgreSQL tests");
    let pool = crate::infrastructure::database::init_postgres_database(&url)
        .await
        .unwrap();

    let target = PipelineViewRepository::new(DatabasePool::Postgres(pool));
    exercise(&target).await;

    let dir = tempfile::tempdir().unwrap();
    let source = init_database(dir.path().join("source.db")).await.unwrap();
    import_merges_or_replaces(
        &PipelineViewRepository::new(DatabasePool::Sqlite(source)),
        &target,
    )
    .await;
}
