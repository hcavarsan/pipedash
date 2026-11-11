use std::collections::HashMap;
use std::sync::Arc;

use super::PipelineViewService;
use crate::application::test_support::{
    pipeline,
    Harness,
};
use crate::domain::{
    CreateViewInput,
    DomainError,
    LegacyImportResult,
    LegacyMarks,
    MembershipSource,
    PinKind,
    PipelineStatus,
    PipelineViewPage,
    PipelineViewQuery,
    ProviderConfig,
    ViewKind,
    ViewMembership,
    ViewPatch,
    ViewRef,
    ViewRules,
};

pub(super) fn service(h: &Harness) -> PipelineViewService {
    PipelineViewService::new(
        Arc::clone(&h.repository),
        Arc::clone(&h.provider_service),
        Arc::clone(&h.pipeline_service),
        Arc::clone(&h.config_state),
        h.bus.clone(),
        Arc::clone(&h.presenter),
    )
}

pub(super) async fn seed(h: &Harness) -> (i64, Vec<String>) {
    let id = h.add_provider("ci").await;
    let pipelines = vec![
        pipeline(id, "build", PipelineStatus::Success),
        pipeline(id, "deploy", PipelineStatus::Failed),
        pipeline(id, "lint", PipelineStatus::Success),
    ];
    h.repository
        .update_pipelines_cache(id, &pipelines)
        .await
        .unwrap();
    (id, pipelines.into_iter().map(|p| p.id).collect())
}

pub(super) fn input(name: &str) -> CreateViewInput {
    CreateViewInput {
        name: name.to_string(),
        rules: ViewRules::default(),
        display: Default::default(),
        members: Vec::new(),
        parent: None,
    }
}

pub(super) fn child(name: &str, parent: &ViewRef) -> CreateViewInput {
    CreateViewInput {
        parent: Some(parent.clone()),
        ..input(name)
    }
}

pub(super) struct Tree {
    pub(super) a: ViewRef,
    pub(super) a1: ViewRef,
    pub(super) a1x: ViewRef,
    pub(super) a2: ViewRef,
    pub(super) b: ViewRef,
}

pub(super) async fn tree(views: &PipelineViewService) -> Tree {
    let a = views.create_view(input("a")).await.unwrap().id;
    let a1 = views.create_view(child("a1", &a)).await.unwrap().id;
    let a1x = views.create_view(child("a1x", &a1)).await.unwrap().id;
    let a2 = views.create_view(child("a2", &a)).await.unwrap().id;
    let b = views.create_view(input("b")).await.unwrap().id;
    Tree { a, a1, a1x, a2, b }
}

fn move_to(parent: Option<&ViewRef>) -> ViewPatch {
    ViewPatch {
        parent: Some(parent.cloned()),
        ..Default::default()
    }
}

fn invalid(error: DomainError, message: &str) {
    assert!(
        matches!(&error, DomainError::InvalidConfig(m) if m.contains(message)),
        "{error:?}"
    );
}

async fn all_page(views: &PipelineViewService) -> PipelineViewPage {
    views.page(PipelineViewQuery::default()).await.unwrap()
}

#[tokio::test]
async fn menu_snapshot_preserves_hidden_pins_shortcuts_and_missing_members() {
    let h = Harness::new().await;
    let (provider, ids) = seed(&h).await;
    let views = service(&h);
    for id in &ids {
        views
            .set_membership(ViewRef::Pinned, id, true)
            .await
            .unwrap();
    }
    views.set_hidden(&ids[0], true).await.unwrap();
    assert_eq!(
        views
            .menu_snapshot()
            .await
            .unwrap()
            .pipelines
            .iter()
            .map(|p| p.pipeline_id.clone())
            .collect::<Vec<_>>(),
        all_page(&views)
            .await
            .pinned
            .iter()
            .map(|p| p.pipeline.id.clone())
            .collect::<Vec<_>>()
    );
    let custom = views.create_view(input("Release")).await.unwrap();
    views
        .set_pin(PinKind::View, &custom.id.to_string(), true)
        .await
        .unwrap();
    views
        .set_pin(PinKind::Repository, "acme/app", true)
        .await
        .unwrap();
    h.repository
        .update_pipelines_cache(
            provider,
            &[pipeline(provider, "build", PipelineStatus::Success)],
        )
        .await
        .unwrap();
    let menu = views.menu_snapshot().await.unwrap();
    assert_eq!(
        menu.pipelines
            .iter()
            .map(|p| &p.pipeline_id)
            .collect::<Vec<_>>(),
        ids.iter().collect::<Vec<_>>()
    );
    assert!(menu.pipelines[0].pipeline.is_some());
    assert!(menu.pipelines[0].provider_name.is_some());
    assert!(menu.pipelines[1..].iter().all(|p| p.pipeline.is_none()));
    assert_eq!(menu.shortcuts, views.list_pins().await.unwrap());
    assert_eq!(menu.shortcuts.len(), 2);
    assert_eq!(
        menu.pipelines[0].pipeline_id,
        all_page(&views).await.pinned[0].pipeline.id
    );
    views
        .set_membership(ViewRef::Pinned, &ids[1], false)
        .await
        .unwrap();
    views
        .set_membership(ViewRef::Pinned, &ids[1], false)
        .await
        .unwrap();
    assert!(!views
        .menu_snapshot()
        .await
        .unwrap()
        .pipelines
        .iter()
        .any(|p| p.pipeline_id == ids[1]));
    assert!(matches!(
        views.set_membership(ViewRef::Pinned, &ids[1], true).await,
        Err(DomainError::NotFound(_))
    ));
}

#[tokio::test]
async fn pipeline_view_mutations_validate_and_emit() {
    let h = Harness::new().await;
    let (_, ids) = seed(&h).await;
    let views = service(&h);

    let created = views.create_view(input("Release")).await.unwrap();
    assert_eq!(h.bus.count("pipeline-views-changed"), 1);
    assert!(matches!(created.id, ViewRef::Custom(_)));

    let duplicate = views.create_view(input(" release ")).await.unwrap_err();
    assert!(matches!(duplicate, DomainError::InvalidConfig(_)));

    let missing = views
        .set_membership(created.id.clone(), "stub__1__missing", true)
        .await
        .unwrap_err();
    assert!(matches!(missing, DomainError::NotFound(_)));

    let rename = views
        .update_view(
            ViewRef::All,
            ViewPatch {
                name: Some("Everything".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&rename, DomainError::InvalidConfig(m) if m == "All pipelines can't be renamed")
    );
    assert_eq!(h.bus.count("pipeline-views-changed"), 1);

    views
        .set_membership(created.id.clone(), &ids[0], true)
        .await
        .unwrap();
    let page = views
        .page(PipelineViewQuery {
            view: created.id.clone(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.view.count, 1);
}

#[tokio::test]
async fn pipeline_view_editor_keeps_unavailable_members_and_excludes_removed_rule_matches() {
    let h = Harness::new().await;
    let (provider, ids) = seed(&h).await;
    let views = service(&h);
    let created = views
        .create_view(CreateViewInput {
            rules: ViewRules {
                name_contains: Some("deploy".into()),
                ..Default::default()
            },
            members: vec![ids[0].clone(), ids[1].clone()],
            ..input("Release")
        })
        .await
        .unwrap();
    let listed = |page: PipelineViewPage| -> Vec<String> {
        page.sections
            .into_iter()
            .flat_map(|s| s.pipelines)
            .map(|p| p.pipeline.name)
            .collect()
    };
    let page_of = || {
        views.page(PipelineViewQuery {
            view: created.id.clone(),
            ..Default::default()
        })
    };
    let members = |ids: &[&String]| ViewPatch {
        members: Some(ids.iter().map(|id| (*id).clone()).collect()),
        ..Default::default()
    };

    let cached = vec![
        pipeline(provider, "deploy", PipelineStatus::Failed),
        pipeline(provider, "lint", PipelineStatus::Success),
    ];
    h.repository
        .update_pipelines_cache(provider, &cached)
        .await
        .unwrap();

    views
        .update_view(created.id.clone(), members(&[&ids[0]]))
        .await
        .unwrap();
    assert!(listed(page_of().await.unwrap()).is_empty());
    let kept = views.list_members(created.id.clone()).await.unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].pipeline_id, ids[0]);
    assert!(!kept[0].available);

    views
        .update_view(created.id.clone(), members(&[&ids[0], &ids[1]]))
        .await
        .unwrap();
    assert_eq!(listed(page_of().await.unwrap()), vec!["deploy"]);

    let missing = views
        .update_view(
            created.id.clone(),
            members(&[&ids[0], &"stub__9__gone".to_string()]),
        )
        .await
        .unwrap_err();
    assert!(matches!(missing, DomainError::NotFound(_)));
}

#[tokio::test]
async fn pipeline_view_nesting_rejects_deep_trees_cycles_and_bad_parents() {
    let h = Harness::new().await;
    seed(&h).await;
    let views = service(&h);

    let top = views.create_view(input("Top")).await.unwrap();
    let mid = views.create_view(child("Mid", &top.id)).await.unwrap();
    let leaf = views.create_view(child("Leaf", &mid.id)).await.unwrap();
    assert_eq!(leaf.parent, Some(mid.id.clone()));

    let deeper = views
        .create_view(child("Deeper", &leaf.id))
        .await
        .unwrap_err();
    invalid(deeper, "Views can be nested at most 3 levels deep");

    let cycle = views
        .update_view(top.id.clone(), move_to(Some(&leaf.id)))
        .await
        .unwrap_err();
    invalid(cycle, "inside itself");
    let itself = views
        .update_view(top.id.clone(), move_to(Some(&top.id)))
        .await
        .unwrap_err();
    invalid(itself, "inside itself");

    let other = views.create_view(input("Other")).await.unwrap();
    views
        .create_view(child("Other child", &other.id))
        .await
        .unwrap();
    let too_tall = views
        .update_view(other.id.clone(), move_to(Some(&mid.id)))
        .await
        .unwrap_err();
    invalid(too_tall, "Views can be nested at most 3 levels deep");

    let unknown = views
        .create_view(child("Ghost", &ViewRef::Custom(9999)))
        .await
        .unwrap_err();
    assert!(matches!(unknown, DomainError::NotFound(_)));
    let system = views
        .create_view(child("Sys", &ViewRef::All))
        .await
        .unwrap_err();
    invalid(system, "can't contain sub-views");
    let config = views
        .create_view(child("Cfg", &ViewRef::Config("x".into())))
        .await
        .unwrap_err();
    assert!(matches!(config, DomainError::InvalidConfig(_)));
    let moved_system = views
        .update_view(ViewRef::All, move_to(Some(&top.id)))
        .await
        .unwrap_err();
    invalid(moved_system, "All pipelines can't be moved");

    let lifted = views
        .update_view(leaf.id.clone(), move_to(None))
        .await
        .unwrap();
    assert_eq!(lifted.parent, None);
    views
        .update_view(other.id.clone(), move_to(Some(&top.id)))
        .await
        .unwrap();
}

#[tokio::test]
async fn pipeline_view_parent_aggregates_children_and_removal_cascades() {
    let h = Harness::new().await;
    let (_, ids) = seed(&h).await;
    let views = service(&h);

    let parent = views.create_view(input("Parent")).await.unwrap();
    let other = views.create_view(input("Other")).await.unwrap();
    let sub = views.create_view(child("Sub", &parent.id)).await.unwrap();

    let listed = views.list_views().await.unwrap();
    let custom: Vec<&ViewRef> = listed
        .iter()
        .filter(|v| v.kind == ViewKind::Custom)
        .map(|v| &v.id)
        .collect();
    assert_eq!(custom, vec![&parent.id, &sub.id, &other.id]);
    let unchanged = listed.iter().find(|v| v.id == parent.id).unwrap();
    assert_eq!(unchanged.display, parent.display);

    views
        .set_membership(sub.id.clone(), &ids[0], true)
        .await
        .unwrap();
    views
        .set_membership(parent.id.clone(), &ids[1], true)
        .await
        .unwrap();
    let page_of = |view: &ViewRef| {
        views.page(PipelineViewQuery {
            view: view.clone(),
            ..Default::default()
        })
    };

    let page = page_of(&parent.id).await.unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.view.count, 2);
    let section_ids: Vec<&str> = page.sections.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        section_ids,
        vec![format!("view:{}", sub.id).as_str(), "repo:acme/app"]
    );
    let built = &page.sections[0].pipelines[0];
    assert_eq!(built.pipeline.id, ids[0]);
    assert_eq!(
        built.memberships,
        vec![
            ViewMembership {
                view: parent.id.clone(),
                source: MembershipSource::SubView
            },
            ViewMembership {
                view: sub.id.clone(),
                source: MembershipSource::Manual
            },
        ]
    );
    let counts: Vec<usize> = views
        .list_views()
        .await
        .unwrap()
        .iter()
        .filter(|v| v.id == parent.id || v.id == sub.id)
        .map(|v| v.count)
        .collect();
    assert_eq!(counts, vec![2, 1]);
    let own = views.list_members(parent.id.clone()).await.unwrap();
    assert_eq!(own.len(), 1);

    views
        .set_membership(parent.id.clone(), &ids[0], false)
        .await
        .unwrap();
    assert_eq!(page_of(&sub.id).await.unwrap().total, 0);
    assert_eq!(page_of(&parent.id).await.unwrap().total, 1);

    views.delete_view(parent.id.clone()).await.unwrap();
    let listed = views.list_views().await.unwrap();
    let orphan = listed.iter().find(|v| v.id == sub.id).unwrap();
    assert_eq!(orphan.parent, None);
}

#[tokio::test]
async fn pipeline_view_legacy_marks_resolve_by_display_name() {
    let h = Harness::new().await;
    let id = h
        .repository
        .add_provider(&ProviderConfig {
            id: None,
            name: "stub-one".into(),
            provider_type: "stub".into(),
            token: "token".into(),
            config: HashMap::from([("display_name".to_string(), "Stub One".to_string())]),
            refresh_interval: 30,
            version: None,
        })
        .await
        .unwrap();
    let views = service(&h);

    let result = views
        .import_legacy_marks(LegacyMarks {
            pinned: vec!["Stub One:stub__x".into(), "Nobody:stub__y".into()],
            hidden: Vec::new(),
        })
        .await
        .unwrap();

    assert_eq!(
        result,
        LegacyImportResult {
            imported: 1,
            skipped: 1
        }
    );
    let members = views.list_members(ViewRef::Pinned).await.unwrap();
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].pipeline_id, format!("stub__{id}__x"));
    assert!(!members[0].available);
    assert_eq!(h.bus.count("pipeline-views-changed"), 1);
}

#[tokio::test]
async fn pipeline_view_pins_validate_list_unpin_and_follow_their_targets() {
    let h = Harness::new().await;
    seed(&h).await;
    let views = service(&h);
    let created = views.create_view(input("Release")).await.unwrap();
    let ViewRef::Custom(id) = created.id.clone() else {
        panic!("custom view expected");
    };

    let unknown = views
        .set_pin(PinKind::Repository, "acme/missing", true)
        .await
        .unwrap_err();
    assert!(matches!(unknown, DomainError::NotFound(_)));
    let system = views.set_pin(PinKind::View, "all", true).await.unwrap_err();
    assert!(matches!(system, DomainError::NotFound(_)));
    assert!(views.list_pins().await.unwrap().is_empty());

    views
        .set_pin(PinKind::Repository, "acme/app", true)
        .await
        .unwrap();
    views
        .set_pin(PinKind::View, &id.to_string(), true)
        .await
        .unwrap();
    views
        .set_pin(PinKind::Repository, "acme/app", true)
        .await
        .unwrap();
    let pins = views.list_pins().await.unwrap();
    assert_eq!(
        pins.iter()
            .map(|p| (p.kind, p.reference.as_str(), p.name.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (PinKind::Repository, "acme/app", "app"),
            (PinKind::View, id.to_string().as_str(), "Release"),
        ]
    );
    assert_eq!(h.bus.count("pipeline-views-changed"), 4);

    views
        .set_pin(PinKind::Repository, "acme/app", false)
        .await
        .unwrap();
    views
        .set_pin(PinKind::Repository, "acme/app", false)
        .await
        .unwrap();
    assert_eq!(views.list_pins().await.unwrap().len(), 1);

    views.delete_view(created.id).await.unwrap();
    assert!(views.list_pins().await.unwrap().is_empty());
    assert!(views.views.list_pins().await.unwrap().is_empty());
}

#[tokio::test]
async fn pipeline_view_pinned_blocks_list_repository_pipelines_after_filters() {
    let h = Harness::new().await;
    let (provider, ids) = seed(&h).await;
    let views = service(&h);
    views
        .set_pin(PinKind::Repository, "acme/app", true)
        .await
        .unwrap();

    let page = all_page(&views).await;
    assert_eq!(page.pinned_blocks.len(), 1);
    let block = &page.pinned_blocks[0];
    assert_eq!(
        (block.kind, block.reference.as_str()),
        (PinKind::Repository, "acme/app")
    );
    assert_eq!(block.name, "app");
    assert_eq!(block.pipelines.len(), 3);

    let searched = views
        .page(PipelineViewQuery {
            search: Some("deploy".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(searched.pinned_blocks[0].pipelines.len(), 1);

    let none = views
        .page(PipelineViewQuery {
            search: Some("nothing-matches".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(none.pinned_blocks.is_empty());

    let filtered = views
        .page(PipelineViewQuery {
            provider_id: Some(provider),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(filtered.pinned_blocks.is_empty());

    views.set_hidden(&ids[0], true).await.unwrap();
    assert_eq!(all_page(&views).await.pinned_blocks[0].pipelines.len(), 2);
    let shown = views
        .page(PipelineViewQuery {
            show_hidden: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(shown.pinned_blocks[0].pipelines.len(), 3);

    let pinned = views
        .page(PipelineViewQuery {
            view: ViewRef::Pinned,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(pinned.pinned_blocks.len(), 1);

    let custom = views.create_view(input("Release")).await.unwrap();
    let other = views
        .page(PipelineViewQuery {
            view: custom.id,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(other.pinned_blocks.is_empty());

    views.reset().await.unwrap();
    assert!(views.list_pins().await.unwrap().is_empty());
}

#[tokio::test]
async fn pipeline_view_config_views_reload_from_config_file() {
    let h = Harness::new().await;
    let path = h._temp_dir.path().join("config.toml");
    std::fs::write(
        &path,
        "[views.release]\nname = \"Release\"\nstatuses = [\"failed\"]\nsort = \"status\"\n",
    )
    .unwrap();

    h.provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();

    let views = service(&h).list_views().await.unwrap();
    let release = views
        .iter()
        .find(|g| g.id == ViewRef::Config("release".into()))
        .unwrap();
    assert_eq!(release.kind, ViewKind::Config);
    assert_eq!(release.display.sort, crate::domain::SortKey::Status);
    assert!(!release.capabilities.edit_display);
    assert_eq!(h.bus.count("pipeline-views-changed"), 1);
}

#[tokio::test]
async fn pipeline_view_rules_drop_a_removed_provider_and_stay_editable() {
    let h = Harness::new().await;
    let kept = h.add_provider("kept").await;
    let removed = h.add_provider("removed").await;
    let views = service(&h);
    let shared = views
        .create_view(CreateViewInput {
            rules: ViewRules {
                provider_ids: vec![kept, removed],
                ..Default::default()
            },
            ..input("Release")
        })
        .await
        .unwrap();
    let only_removed = views
        .create_view(CreateViewInput {
            rules: ViewRules {
                provider_ids: vec![removed],
                name_contains: Some("deploy".into()),
                ..Default::default()
            },
            ..input("Removed deploys")
        })
        .await
        .unwrap();

    h.provider_service.remove_provider(removed).await.unwrap();

    let stored = views.list_views().await.unwrap();
    let rules_of = |id: &ViewRef| stored.iter().find(|v| v.id == *id).unwrap().rules.clone();
    assert_eq!(rules_of(&shared.id).provider_ids, vec![kept]);
    assert_eq!(rules_of(&only_removed.id), ViewRules::default());

    let edited = views
        .update_view(
            shared.id.clone(),
            ViewPatch {
                rules: Some(ViewRules {
                    name_contains: Some("deploy".into()),
                    ..rules_of(&shared.id)
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(edited.rules.provider_ids, vec![kept]);
    assert_eq!(edited.rules.name_contains.as_deref(), Some("deploy"));
}
