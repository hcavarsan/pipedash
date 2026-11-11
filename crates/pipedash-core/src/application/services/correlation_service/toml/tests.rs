use indexmap::IndexMap;

use super::*;
use crate::infrastructure::config::schema::applications::{
    ApplicationFileConfig,
    MemberFileConfig,
    TargetFileConfig,
};
use crate::infrastructure::PipedashConfig;

fn target<'a>(
    cluster: &'a str, namespace: Option<&'a str>, name: &'a str, group_key: &'a str,
    group_label: &'a str,
) -> TomlTarget<'a> {
    TomlTarget {
        cluster,
        namespace,
        name,
        group_key,
        group_label,
        short_lived: false,
    }
}

fn parse(text: &str) -> ApplicationFileConfig {
    let config: PipedashConfig = ::toml::from_str(text).unwrap();
    assert_eq!(config.applications.len(), 1);
    config.applications.into_iter().next().unwrap().1
}

fn member(provider: &str, pipeline: &str, stage: Option<Stage>) -> MemberFileConfig {
    MemberFileConfig {
        provider: provider.into(),
        pipeline: pipeline.into(),
        stage,
    }
}

#[test]
fn slug_lowercases_and_replaces_non_alphanumerics() {
    for (name, key) in [
        ("Checkout API", "checkout-api"),
        ("shop/web_v2", "shop-web-v2"),
        ("QA", "qa"),
    ] {
        assert_eq!(slug(name), key);
    }
}

#[test]
fn copy_as_toml_loads_as_the_same_members_targets_and_directions() {
    let mut short = target("uid:3", Some("pr-7"), "pr-7", "short_lived", "short-lived");
    short.short_lived = true;
    let application = TomlApplication {
        name: "Checkout API",
        hidden: false,
        members: vec![
            TomlMember {
                provider: "ci",
                local_id: "acme/checkout/build",
                stage: Stage::Build,
            },
            TomlMember {
                provider: "gitops",
                local_id: "ns/checkout[prod]",
                stage: Stage::Deploy,
            },
        ],
        targets: vec![
            target(
                "uid:1",
                Some("checkout-qa"),
                "QA",
                "uid:1|checkout-qa",
                "QA",
            ),
            target(
                "uid:2",
                Some("checkout"),
                "Production",
                "uid:2|checkout",
                "Production",
            ),
            target("uid:4", Some("eu"), "EU", "parent:regions", "Regions"),
            target("uid:5", Some("eu"), "EU 2", "parent:regions", "Regions"),
            short,
        ],
        directions: vec![("uid:1|checkout-qa", "uid:2|checkout")],
    };
    let loaded = parse(&render(&application));

    let mut targets = IndexMap::new();
    targets.insert(
        "qa".to_string(),
        TargetFileConfig {
            name: "QA".into(),
            clusters: vec!["uid:1".into()],
            namespaces: vec!["checkout-qa".into()],
            after: Vec::new(),
            group: None,
        },
    );
    targets.insert(
        "production".to_string(),
        TargetFileConfig {
            name: "Production".into(),
            clusters: vec!["uid:2".into()],
            namespaces: vec!["checkout".into()],
            after: vec!["qa".into()],
            group: None,
        },
    );
    targets.insert(
        "regions".to_string(),
        TargetFileConfig {
            name: "Regions".into(),
            clusters: vec!["uid:4".into(), "uid:5".into()],
            namespaces: vec!["eu".into()],
            after: Vec::new(),
            group: None,
        },
    );
    let expected = ApplicationFileConfig {
        name: "Checkout API".into(),
        hidden: false,
        members: vec![
            member("ci", "acme/checkout/build", Some(Stage::Build)),
            member("gitops", "ns/checkout[[]prod[]]", Some(Stage::Deploy)),
        ],
        exclude: Vec::new(),
        targets,
    };
    assert_eq!(loaded, expected);
    assert!(loaded.members[1].matches("gitops", "ns/checkout[prod]"));
    assert!(!loaded.members[1].matches("gitops", "ns/checkouto"));
}

fn mixed_scenario() -> TomlApplication<'static> {
    TomlApplication {
        name: "Mixed",
        hidden: true,
        members: Vec::new(),
        targets: vec![
            target("uid:1", Some("a"), "One", "parent:g", "Group"),
            target("uid:2", Some("b"), "Two", "parent:g", "Group"),
            target("uid:1", Some("b"), "Other", "uid:1|b", "Other"),
            target("uid:3", None, "Bare", "parent:h", "Mixed"),
            target("uid:4", Some("c"), "Named", "parent:h", "Mixed"),
            target("uid:3", Some("c"), "Neighbour", "uid:3|c", "Neighbour"),
        ],
        directions: vec![("uid:1|b", "parent:g"), ("parent:g", "parent:h")],
    }
}

#[test]
fn a_group_that_is_not_a_product_is_an_owner_table_plus_member_tables() {
    let loaded = parse(&render(&mixed_scenario()));
    assert!(loaded.hidden);
    let mut keys: Vec<&str> = loaded.targets.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "group",
            "group-two",
            "mixed",
            "mixed-named",
            "neighbour",
            "other"
        ]
    );
    let group_of = |key: &str| loaded.targets[key].group.as_deref();
    assert_eq!(group_of("group"), None);
    assert_eq!(group_of("group-two"), Some("group"));
    assert_eq!(group_of("mixed-named"), Some("mixed"));
    assert_eq!(loaded.targets["group"].name, "Group");
    assert!(loaded.targets["group-two"].name.is_empty());
}

#[test]
fn every_selector_matches_only_targets_of_its_own_group() {
    let application = mixed_scenario();
    let loaded = parse(&render(&application));
    assert_eq!(loaded.targets["mixed"].namespaces, [""]);
    let owners = [
        ("One", "group"),
        ("Two", "group"),
        ("Other", "other"),
        ("Bare", "mixed"),
        ("Named", "mixed"),
        ("Neighbour", "neighbour"),
    ];
    for target in &application.targets {
        let hits: Vec<&str> = loaded
            .targets
            .iter()
            .filter(|(_, table)| {
                table.matches(target.cluster, target.namespace.unwrap_or(""), target.name)
            })
            .map(|(key, table)| table.owner(key))
            .collect();
        let owner = owners
            .iter()
            .find(|(name, _)| *name == target.name)
            .unwrap()
            .1;
        assert_eq!(hits, [owner], "{}", target.name);
    }
}

#[test]
fn a_direction_is_emitted_once_on_the_owner_of_the_group_it_points_to() {
    let loaded = parse(&render(&mixed_scenario()));
    let after = |key: &str| loaded.targets[key].after.clone();
    assert_eq!(after("group"), ["other"]);
    assert_eq!(after("mixed"), ["group"]);
    for key in ["group-two", "mixed-named", "other", "neighbour"] {
        assert!(after(key).is_empty(), "{key}");
    }
}

#[test]
fn an_application_without_members_or_targets_is_just_a_name() {
    let application = TomlApplication {
        name: "Empty",
        hidden: false,
        members: Vec::new(),
        targets: Vec::new(),
        directions: Vec::new(),
    };
    let text = render(&application);
    assert_eq!(text.trim(), "[applications.empty]\nname = \"Empty\"");
}
