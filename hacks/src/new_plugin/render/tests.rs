use std::collections::BTreeSet;

use super::*;
use crate::args::{
    Archetype,
    Args,
    Capability,
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

fn content<'a>(files: &'a [(String, String)], path: &str) -> &'a str {
    files
        .iter()
        .find(|(file, _)| file == path)
        .map(|(_, content)| content.as_str())
        .unwrap()
}

#[test]
fn generated_methods_and_metadata_follow_the_capabilities() {
    let all = [Capability::Trigger, Capability::Cancel, Capability::Logs];
    for archetype in [Archetype::Rest, Archetype::Kubernetes] {
        for mask in 0..8 {
            let chosen: Vec<Capability> = (0..3)
                .filter(|bit| mask & (1 << bit) != 0)
                .map(|bit| all[bit])
                .collect();
            let mut spec = spec(archetype);
            spec.capabilities = crate::spec::Capabilities::from_list(&chosen);
            let caps = spec.capabilities;
            let files = files(&spec).unwrap();
            let plugin = content(&files, "src/plugin.rs");
            let metadata = content(&files, "src/metadata.rs");
            let case = format!("{archetype:?} {chosen:?}");

            assert_eq!(
                plugin.contains("PluginError::NotSupported"),
                !caps.trigger,
                "{case}"
            );
            assert_eq!(
                plugin.contains("async fn cancel_run"),
                caps.cancel,
                "{case}"
            );
            assert_eq!(
                plugin.contains("async fn fetch_log_nodes"),
                caps.logs,
                "{case}"
            );
            assert_eq!(plugin.contains("async fn read_log"), caps.logs, "{case}");
            assert_eq!(metadata.contains("trigger: false"), !caps.trigger, "{case}");
            assert_eq!(metadata.contains("cancel: false"), !caps.cancel, "{case}");
            assert_eq!(metadata.contains("logs: false"), !caps.logs, "{case}");
            let overridden = !(caps.trigger && caps.cancel && caps.logs) || spec.kubernetes;
            assert_eq!(
                metadata.contains("PluginCapabilities::standard(),"),
                !overridden,
                "{case}"
            );
        }
    }
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

    assert_eq!(content(&files, "assets/icon.svg"), "<svg id=\"given\"/>");
}
