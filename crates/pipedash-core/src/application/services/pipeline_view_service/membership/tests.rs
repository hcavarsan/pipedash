use super::super::tests::{
    input,
    seed,
    service,
    tree,
};
use crate::application::test_support::Harness;
use crate::domain::{
    CreateViewInput,
    DomainError,
    ViewMembershipChange,
    ViewPatch,
    ViewRef,
    ViewRules,
    ViewSelectionEntry,
    ViewSelectionQuery,
};

fn change(ids: &[&String], member: bool) -> ViewMembershipChange {
    ViewMembershipChange {
        pipeline_ids: ids.iter().map(|id| (*id).clone()).collect(),
        member,
    }
}

fn entry<'a>(entries: &'a [ViewSelectionEntry], id: &ViewRef) -> &'a ViewSelectionEntry {
    entries.iter().find(|e| e.view.id == *id).unwrap()
}

fn counts(entry: &ViewSelectionEntry) -> (usize, usize) {
    (entry.selected_count, entry.direct_count)
}

fn rules(name: &str) -> ViewPatch {
    ViewPatch {
        rules: Some(ViewRules {
            name_contains: Some(name.into()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[tokio::test]
async fn selection_lists_custom_views_in_tree_order_with_depth_and_inheritance() {
    let h = Harness::new().await;
    let (_, ids) = seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;

    let after = views
        .set_memberships(t.a1x.clone(), change(&[&ids[0]], true))
        .await
        .unwrap();

    assert_eq!(
        after.iter().map(|e| e.view.id.clone()).collect::<Vec<_>>(),
        vec![
            t.a.clone(),
            t.a1.clone(),
            t.a1x.clone(),
            t.a2.clone(),
            t.b.clone()
        ]
    );
    assert_eq!(
        after.iter().map(|e| e.depth).collect::<Vec<_>>(),
        vec![1, 2, 3, 2, 1]
    );
    assert_eq!(counts(entry(&after, &t.a1x)), (1, 1));
    for inherited in [&t.a, &t.a1] {
        let found = entry(&after, inherited);
        assert_eq!(counts(found), (1, 0));
        assert!(found.included_by_sub_view && !found.included_by_rule);
    }
    assert_eq!(counts(entry(&after, &t.a2)), (0, 0));

    let fresh = views
        .selection(ViewSelectionQuery {
            pipeline_ids: vec![ids[0].clone(), ids[0].clone(), "stub__9__gone".into()],
        })
        .await
        .unwrap();
    assert_eq!(counts(entry(&fresh, &t.a)), (1, 0));
}

#[tokio::test]
async fn leaving_the_only_sub_view_that_brought_a_pipeline_into_a_parent_also_clears_the_parent() {
    let h = Harness::new().await;
    let (_, ids) = seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;

    let ticked = views
        .set_memberships(t.a1.clone(), change(&[&ids[0]], true))
        .await
        .unwrap();
    assert_eq!(counts(entry(&ticked, &t.a)), (1, 0));

    let cleared = views
        .set_memberships(t.a1.clone(), change(&[&ids[0]], false))
        .await
        .unwrap();
    assert_eq!(counts(entry(&cleared, &t.a1)), (0, 0));
    assert_eq!(counts(entry(&cleared, &t.a)), (0, 0));
}

#[tokio::test]
async fn a_parent_keeps_a_pipeline_after_one_sub_view_drops_it_when_it_is_still_own_or_in_a_sibling(
) {
    let h = Harness::new().await;
    let (_, ids) = seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;
    views
        .update_view(t.a.clone(), rules("build"))
        .await
        .unwrap();
    let both = change(&[&ids[0], &ids[1]], true);
    views.set_memberships(t.a1.clone(), both).await.unwrap();
    views
        .set_memberships(t.a2.clone(), change(&[&ids[1]], true))
        .await
        .unwrap();

    let after = views
        .set_memberships(t.a1.clone(), change(&[&ids[0], &ids[1]], false))
        .await
        .unwrap();

    let parent = entry(&after, &t.a);
    assert_eq!(counts(parent), (2, 1));
    assert!(parent.included_by_rule && parent.included_by_sub_view);
    assert_eq!(counts(entry(&after, &t.a1)), (0, 0));
    assert_eq!(counts(entry(&after, &t.a2)), (1, 1));
}

#[tokio::test]
async fn removing_a_pipeline_from_a_parent_removes_it_from_the_whole_subtree() {
    let h = Harness::new().await;
    let (_, ids) = seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;
    views
        .set_memberships(t.a1x.clone(), change(&[&ids[0]], true))
        .await
        .unwrap();
    views
        .set_memberships(t.b.clone(), change(&[&ids[0]], true))
        .await
        .unwrap();

    let after = views
        .set_memberships(t.a.clone(), change(&[&ids[0]], false))
        .await
        .unwrap();

    for cleared in [&t.a, &t.a1, &t.a1x] {
        assert_eq!(counts(entry(&after, cleared)), (0, 0));
    }
    assert_eq!(counts(entry(&after, &t.b)), (1, 1));
}

#[tokio::test]
async fn unticking_a_rule_match_excludes_it_from_the_view_and_its_sub_views() {
    let h = Harness::new().await;
    let (_, ids) = seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;
    views
        .update_view(t.a.clone(), rules("deploy"))
        .await
        .unwrap();
    views
        .update_view(t.a1.clone(), rules("deploy"))
        .await
        .unwrap();

    let before = views
        .selection(ViewSelectionQuery {
            pipeline_ids: vec![ids[1].clone()],
        })
        .await
        .unwrap();
    assert!(entry(&before, &t.a).included_by_rule);
    assert!(entry(&before, &t.a1).included_by_rule);

    let after = views
        .set_memberships(t.a.clone(), change(&[&ids[1]], false))
        .await
        .unwrap();
    for excluded in [&t.a, &t.a1] {
        let found = entry(&after, excluded);
        assert_eq!(counts(found), (0, 0));
        assert!(!found.included_by_rule);
    }
}

#[tokio::test]
async fn a_batch_takes_one_write_emits_once_and_validates_before_writing() {
    let h = Harness::new().await;
    let (_, ids) = seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;
    let emitted = h.bus.count("pipeline-views-changed");

    let after = views
        .set_memberships(t.b.clone(), change(&[&ids[0], &ids[1], &ids[2]], true))
        .await
        .unwrap();
    assert_eq!(counts(entry(&after, &t.b)), (3, 3));
    assert_eq!(h.bus.count("pipeline-views-changed"), emitted + 1);

    let gone = "stub__9__gone".to_string();
    let missing = views
        .set_memberships(t.a.clone(), change(&[&ids[0], &gone], true))
        .await
        .unwrap_err();
    assert!(matches!(missing, DomainError::NotFound(_)));
    let unchanged = views
        .selection(ViewSelectionQuery {
            pipeline_ids: vec![ids[0].clone()],
        })
        .await
        .unwrap();
    assert_eq!(counts(entry(&unchanged, &t.a)), (0, 0));
    assert_eq!(h.bus.count("pipeline-views-changed"), emitted + 1);

    let system = views
        .set_memberships(ViewRef::All, change(&[&ids[0]], true))
        .await
        .unwrap_err();
    assert!(matches!(system, DomainError::InvalidConfig(_)));
}

#[tokio::test]
async fn pinned_batch_can_unpin_pipelines_that_left_the_cache() {
    let h = Harness::new().await;
    let (provider, ids) = seed(&h).await;
    let views = service(&h);
    views
        .set_memberships(ViewRef::Pinned, change(&[&ids[0], &ids[1]], true))
        .await
        .unwrap();
    h.repository
        .update_pipelines_cache(provider, &[])
        .await
        .unwrap();

    views
        .set_memberships(ViewRef::Pinned, change(&[&ids[0], &ids[1]], false))
        .await
        .unwrap();

    assert!(views
        .list_members(ViewRef::Pinned)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn list_members_follows_the_manual_order() {
    let h = Harness::new().await;
    let (_, ids) = seed(&h).await;
    let views = service(&h);
    let created = views
        .create_view(CreateViewInput {
            members: vec![ids[2].clone(), ids[0].clone(), ids[1].clone()],
            ..input("Release")
        })
        .await
        .unwrap();
    let listing = &views;
    let order = |view: ViewRef| async move {
        listing
            .list_members(view)
            .await
            .unwrap()
            .into_iter()
            .map(|m| m.pipeline_id)
            .collect::<Vec<_>>()
    };

    assert_eq!(
        order(created.id.clone()).await,
        vec![ids[2].clone(), ids[0].clone(), ids[1].clone()]
    );

    views
        .update_view(
            created.id.clone(),
            ViewPatch {
                members: Some(vec![ids[1].clone(), ids[2].clone(), ids[0].clone()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        order(created.id).await,
        vec![ids[1].clone(), ids[2].clone(), ids[0].clone()]
    );
}
