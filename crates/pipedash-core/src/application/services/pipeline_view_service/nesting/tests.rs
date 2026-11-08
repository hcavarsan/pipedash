use super::super::tests::{
    seed,
    service,
    tree,
};
use crate::application::test_support::Harness;
use crate::domain::{
    DomainError,
    ViewEditorOptions,
    ViewRef,
};

fn parents(options: &ViewEditorOptions) -> Vec<ViewRef> {
    options
        .eligible_parents
        .iter()
        .map(|v| v.id.clone())
        .collect()
}

#[tokio::test]
async fn a_new_view_can_sit_under_custom_views_of_depth_one_or_two() {
    let h = Harness::new().await;
    seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;

    let options = views.editor_options(None).await.unwrap();

    assert_eq!(
        parents(&options),
        vec![t.a.clone(), t.a1.clone(), t.a2.clone(), t.b.clone()]
    );
    assert!(!options.can_add_sub_view);
}

#[tokio::test]
async fn a_view_cannot_move_into_itself_its_subtree_or_past_the_maximum_depth() {
    let h = Harness::new().await;
    seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;
    let service = &views;
    let eligible =
        |view: ViewRef| async move { parents(&service.editor_options(Some(view)).await.unwrap()) };

    assert_eq!(eligible(t.a.clone()).await, Vec::<ViewRef>::new());
    assert_eq!(eligible(t.a1.clone()).await, vec![t.a.clone(), t.b.clone()]);
    assert_eq!(
        eligible(t.a1x.clone()).await,
        vec![t.a.clone(), t.a1.clone(), t.a2.clone(), t.b.clone()]
    );
    assert_eq!(
        eligible(t.b.clone()).await,
        vec![t.a.clone(), t.a1.clone(), t.a2.clone()]
    );
}

#[tokio::test]
async fn only_custom_views_shallower_than_the_maximum_depth_accept_sub_views() {
    let h = Harness::new().await;
    seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;

    for (view, expected) in [(&t.a, true), (&t.a1, true), (&t.a1x, false), (&t.b, true)] {
        let options = views.editor_options(Some(view.clone())).await.unwrap();
        assert_eq!(options.can_add_sub_view, expected, "{view}");
    }
    for system in [ViewRef::All, ViewRef::Pinned] {
        let denied = views.editor_options(Some(system)).await.unwrap_err();
        assert!(matches!(denied, DomainError::InvalidConfig(_)));
    }
    let unknown = views
        .editor_options(Some(ViewRef::Custom(9999)))
        .await
        .unwrap_err();
    assert!(matches!(unknown, DomainError::NotFound(_)));
}
