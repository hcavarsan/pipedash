use crate::application::services::correlation_service::tests::{
    unit,
    Fx,
};
use crate::application::services::correlation_service::vocab::SAME_LABEL;

async fn labelled(count: usize) -> Fx {
    let fx = Fx::new().await;
    let gitops = fx.provider("gitops").await;
    let units = (0..count)
        .map(|index| {
            let mut pipeline = unit(gitops, &format!("svc{index}"));
            pipeline.correlation.app_label = Some("shop".to_string());
            pipeline
        })
        .collect();
    fx.apply(gitops, units).await;
    fx
}

#[tokio::test]
async fn hub_supports_no_suggestion() {
    let hub = labelled(6).await;
    assert!(hub.edges(SAME_LABEL).await.is_empty());
    assert!(hub.open().await.is_empty());

    let small = labelled(5).await;
    assert_eq!(small.edges(SAME_LABEL).await.len(), 10);
}
