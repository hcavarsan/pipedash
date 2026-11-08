use std::collections::BTreeSet;

use super::*;
use crate::args::{
    Archetype,
    Args,
};

fn spec(archetype: Archetype) -> Spec {
    let args = Args {
        id: Some("gamma".to_string()),
        description: Some("Monitor Gamma pipelines".to_string()),
        archetype: Some(archetype),
        ..Args::default()
    };
    crate::spec::from_args(&args, &BTreeSet::new()).unwrap()
}

fn paths(files: &[(String, String)]) -> Vec<&str> {
    let mut paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
    paths.sort_unstable();
    paths
}

#[test]
fn rest_renders_the_client_and_plugin() {
    let files = files(&spec(Archetype::Rest)).unwrap();

    assert_eq!(
        paths(&files),
        [
            "Cargo.toml",
            "assets/icon.svg",
            "src/client.rs",
            "src/config.rs",
            "src/config/tests.rs",
            "src/lib.rs",
            "src/metadata.rs",
            "src/plugin.rs",
            "src/schema.rs",
        ]
    );
}

#[test]
fn kubernetes_has_no_client_module() {
    let files = files(&spec(Archetype::Kubernetes)).unwrap();

    assert_eq!(files.len(), 8);
    assert!(!paths(&files).contains(&"src/client.rs"));
}

#[test]
fn a_given_icon_is_copied_verbatim() {
    let mut spec = spec(Archetype::Rest);
    spec.icon_svg = Some("<svg id=\"given\"/>".to_string());

    let files = files(&spec).unwrap();

    let icon = files
        .iter()
        .find(|(path, _)| path == "assets/icon.svg")
        .unwrap();
    assert_eq!(icon.1, "<svg id=\"given\"/>");
}
