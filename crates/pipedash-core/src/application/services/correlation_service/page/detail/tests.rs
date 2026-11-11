use std::collections::{
    BTreeMap,
    BTreeSet,
};

use super::toml_of;
use crate::application::services::correlation_service::state::ApplicationState;
use crate::application::services::correlation_service::tests::{
    target,
    unit,
    Fx,
};
use crate::domain::application::overrides::ApplicationEdit;

const PROVIDERS: &str =
    "[providers.ci]\nname = \"CI\"\ntype = \"stub\"\ntoken = \"t\"\nrefresh_interval = 30\n\n";

const EXPECTED: &str = r#"[applications.shop-api]
name = "shop-api"

[[applications.shop-api.members]]
provider = "ci"
pipeline = "shop-api"
stage = "deploy"

[applications.shop-api.targets.pair]
name = "Pair"
clusters = ["uid:1"]
namespaces = ["a"]
after = ["other"]

[applications.shop-api.targets.pair-two]
group = "pair"
clusters = ["uid:2"]
namespaces = ["b"]

[applications.shop-api.targets.other]
name = "Other"
clusters = ["uid:1"]
namespaces = ["b"]

[applications.shop-api.targets.sink]
name = "Sink"
clusters = ["uid:3"]
namespaces = ["c"]
after = ["pair"]
"#;

#[derive(Debug, PartialEq)]
struct Shape {
    groups: BTreeSet<(String, BTreeSet<String>)>,
    names: BTreeMap<String, String>,
    directions: BTreeSet<(BTreeSet<String>, BTreeSet<String>)>,
}

fn keys_of(state: &ApplicationState, group: &str) -> BTreeSet<String> {
    state
        .targets
        .iter()
        .filter(|t| t.group_key == group)
        .map(|t| t.target_key.clone())
        .collect()
}

fn shape(state: &ApplicationState) -> Shape {
    Shape {
        groups: state
            .groups
            .iter()
            .map(|g| (g.label.clone(), keys_of(state, &g.key)))
            .collect(),
        names: state
            .targets
            .iter()
            .map(|t| (t.target_key.clone(), t.name.clone()))
            .collect(),
        directions: state
            .directions
            .iter()
            .map(|d| (keys_of(state, &d.from_group), keys_of(state, &d.to_group)))
            .collect(),
    }
}

fn keys(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|k| (*k).to_string()).collect()
}

#[tokio::test]
async fn a_group_that_is_not_a_product_survives_copy_as_toml_and_a_config_reload() {
    let fx = Fx::new().await;
    let path = fx.h.config_state.config_path();
    std::fs::write(path, PROVIDERS).unwrap();
    fx.h.provider_service
        .apply_config_file(&fx.h.config_state)
        .await
        .unwrap();
    let ci =
        fx.h.repository
            .config_backend()
            .list_providers()
            .await
            .unwrap()
            .into_iter()
            .find(|p| p.name == "ci")
            .and_then(|p| p.id)
            .unwrap();

    let mut api = unit(ci, "shop-api");
    api.correlation.targets = vec![
        target("1", "a", "One"),
        target("2", "b", "Two"),
        target("1", "b", "Other"),
        target("3", "c", "Sink"),
    ];
    fx.apply(ci, vec![api]).await;
    fx.settle().await;
    let app = fx.app("shop-api").await;

    let edits = [
        ApplicationEdit::TargetGroup {
            target_keys: vec!["uid:1|a".into(), "uid:2|b".into()],
            group_name: Some("Pair".into()),
        },
        ApplicationEdit::Direction {
            from_group: "uid:1|b".into(),
            to_group: "user:pair".into(),
            present: true,
        },
        ApplicationEdit::Direction {
            from_group: "user:pair".into(),
            to_group: "uid:3|c".into(),
            present: true,
        },
    ];
    let mut detail = None;
    for edit in edits {
        detail = Some(fx.svc.apply_edit(app.id, edit, None).await.unwrap());
    }
    fx.settle().await;
    let text = detail.unwrap().toml.unwrap();
    assert_eq!(text, EXPECTED);
    let source = shape(&fx.svc.application_state(app.id).await.unwrap());
    assert_eq!(source.groups.len(), 3);
    assert_eq!(source.directions.len(), 2);
    assert!(source
        .groups
        .contains(&("Pair".to_string(), keys(&["uid:1|a", "uid:2|b"]))));

    std::fs::write(path, format!("{PROVIDERS}{text}")).unwrap();
    fx.h.provider_service
        .apply_config_file(&fx.h.config_state)
        .await
        .unwrap();
    fx.settle().await;

    let config: Vec<_> = fx
        .apps()
        .await
        .into_iter()
        .filter(|a| a.identity_kind == "config")
        .collect();
    assert_eq!(config.len(), 1);
    let facts = fx.svc.facts(Some(config[0].id)).await.unwrap();
    let state = facts.states.first().unwrap();
    assert_eq!(shape(state), source);
    assert_eq!(toml_of(&facts, state), text);
}
