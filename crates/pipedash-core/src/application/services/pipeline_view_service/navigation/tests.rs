use super::super::tests::{
    seed,
    service,
    tree,
};
use crate::application::test_support::Harness;
use crate::domain::{
    PinKind,
    PipelineNavigation,
    ViewKind,
    ViewRef,
};

fn custom(nav: &PipelineNavigation) -> Vec<ViewRef> {
    nav.views
        .iter()
        .filter(|v| v.kind == ViewKind::Custom)
        .map(|v| v.id.clone())
        .collect()
}

#[tokio::test]
async fn navigation_without_a_search_lists_everything_except_the_all_view() {
    let h = Harness::new().await;
    seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;
    views
        .set_pin(PinKind::Repository, "acme/app", true)
        .await
        .unwrap();

    let nav = views.navigation(None).await.unwrap();

    assert!(nav.views.iter().all(|v| v.kind != ViewKind::All));
    assert!(nav.views.iter().any(|v| v.kind == ViewKind::Pinned));
    assert_eq!(
        custom(&nav),
        vec![
            t.a.clone(),
            t.a1.clone(),
            t.a1x.clone(),
            t.a2.clone(),
            t.b.clone()
        ]
    );
    assert_eq!(nav.pins.len(), 1);
    assert_eq!(nav.providers.len(), 1);
    assert_eq!(
        nav.total_pipelines,
        nav.providers
            .iter()
            .map(|p| p.pipeline_count)
            .sum::<usize>()
    );
    assert!(nav.total_pipelines > 0);

    let blank = views.navigation(Some("  ".into())).await.unwrap();
    assert_eq!(custom(&blank), custom(&nav));
    assert_eq!(blank.pins, nav.pins);
}

#[tokio::test]
async fn navigation_search_keeps_matching_views_with_their_ancestors() {
    let h = Harness::new().await;
    seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;

    let by_leaf = views.navigation(Some(" A1X ".into())).await.unwrap();
    assert_eq!(
        custom(&by_leaf),
        vec![t.a.clone(), t.a1.clone(), t.a1x.clone()]
    );

    let by_sibling = views.navigation(Some("a2".into())).await.unwrap();
    assert_eq!(custom(&by_sibling), vec![t.a.clone(), t.a2.clone()]);
}

#[tokio::test]
async fn navigation_search_filters_pins_and_providers_but_not_the_total() {
    let h = Harness::new().await;
    seed(&h).await;
    let views = service(&h);
    let t = tree(&views).await;
    let ViewRef::Custom(b) = t.b else {
        panic!("custom view expected");
    };
    views
        .set_pin(PinKind::View, &b.to_string(), true)
        .await
        .unwrap();
    views
        .set_pin(PinKind::Repository, "acme/app", true)
        .await
        .unwrap();
    let everything = views.navigation(None).await.unwrap();

    let pinned_view = views.navigation(Some("b".into())).await.unwrap();
    assert_eq!(
        pinned_view
            .pins
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["b"]
    );

    let none = views.navigation(Some("zzz".into())).await.unwrap();
    assert!(none.views.is_empty());
    assert!(none.pins.is_empty());
    assert!(none.providers.is_empty());
    assert_eq!(none.total_pipelines, everything.total_pipelines);

    let provider = everything.providers[0].name.to_uppercase();
    let found = views.navigation(Some(provider)).await.unwrap();
    assert_eq!(found.providers.len(), 1);
}
