use indexmap::IndexMap;

use crate::application::services::correlation_service::tests::{
    unit,
    Fx,
};
use crate::infrastructure::config::schema::applications::{
    ApplicationFileConfig,
    CorrelationFileConfig,
    MemberFileConfig,
    PipelineSelector,
};

fn config_app(name: &str, pattern: &str) -> ApplicationFileConfig {
    ApplicationFileConfig {
        name: name.into(),
        members: vec![MemberFileConfig {
            provider: "gitops".into(),
            pipeline: pattern.into(),
            stage: None,
        }],
        ..ApplicationFileConfig::default()
    }
}

async fn configure(fx: &Fx, apps: &[(&str, ApplicationFileConfig)]) {
    let applications: IndexMap<String, ApplicationFileConfig> = apps
        .iter()
        .map(|(key, app)| (key.to_string(), app.clone()))
        .collect();
    fx.h.config_state
        .remember_applications(CorrelationFileConfig::default(), applications)
        .await;
}

#[tokio::test]
async fn config_claim_replaces_the_automatic_application() {
    let fx = Fx::new().await;
    configure(
        &fx,
        &[("checkout", config_app("Checkout API", "checkout-*"))],
    )
    .await;
    let gitops = fx.provider("gitops").await;
    let api = unit(gitops, "checkout-api");
    fx.apply(gitops, vec![api.clone()]).await;
    let later = unit(gitops, "checkout-web");
    fx.apply(gitops, vec![api.clone(), later.clone()]).await;
    fx.svc.resolve_all().await.unwrap();

    let apps = fx.apps().await;
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].name, "Checkout API");
    assert_eq!(apps[0].identity_kind, "config");
    let mut members = fx.member_ids("Checkout API").await;
    members.sort();
    let mut expected = vec![api.id, later.id];
    expected.sort();
    assert_eq!(members, expected);
}

#[tokio::test]
async fn a_pipeline_matching_two_config_applications_is_claimed_by_neither() {
    let fx = Fx::new().await;
    configure(
        &fx,
        &[
            ("first", config_app("First", "svc-*")),
            ("second", config_app("Second", "*-api")),
        ],
    )
    .await;
    let gitops = fx.provider("gitops").await;
    let shared = unit(gitops, "svc-api");
    fx.apply(gitops, vec![shared.clone()]).await;
    fx.svc.resolve_all().await.unwrap();

    let first = fx.app("First").await;
    let second = fx.app("Second").await;
    assert!(fx.members(first.id).await.is_empty());
    assert!(fx.members(second.id).await.is_empty());
    for id in [first.id, second.id] {
        let state = fx.svc.application_state(id).await.unwrap();
        assert!(state.warnings.iter().any(|w| w.contains("svc-api")));
    }
    assert_eq!(fx.app("svc-api").await.identity_kind, "unit");
}

#[tokio::test]
async fn ignored_pipeline_never_creates_an_application() {
    let fx = Fx::new().await;
    let correlation = CorrelationFileConfig {
        ignore: vec![PipelineSelector {
            provider: "gitops".into(),
            pipeline: "sandbox-*".into(),
        }],
        ..CorrelationFileConfig::default()
    };
    fx.h.config_state
        .remember_applications(correlation, IndexMap::new())
        .await;
    let gitops = fx.provider("gitops").await;
    fx.apply(
        gitops,
        vec![unit(gitops, "sandbox-1"), unit(gitops, "real")],
    )
    .await;

    let names: Vec<String> = fx.apps().await.into_iter().map(|a| a.name).collect();
    assert_eq!(names, ["real"]);
}
