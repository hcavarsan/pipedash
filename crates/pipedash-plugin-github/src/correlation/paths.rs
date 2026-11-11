use std::collections::{
    BTreeSet,
    HashMap,
};

use pipedash_plugin_api::PathScope;
use serde::de::IgnoredAny;
use serde::Deserialize;

const CODE_EVENTS: [&str; 2] = ["push", "pull_request"];

#[derive(Deserialize)]
struct WorkflowFile {
    #[serde(default)]
    on: Option<Triggers>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Triggers {
    Events(HashMap<String, Option<Event>>),
    Other(IgnoredAny),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Event {
    Filter(Filter),
    Other(IgnoredAny),
}

#[derive(Deserialize)]
struct Filter {
    #[serde(default)]
    paths: Option<Vec<String>>,
}

pub(crate) fn trigger_scopes(workflow_source: &str) -> Vec<PathScope> {
    let Ok(file) = serde_saphyr::from_str::<WorkflowFile>(workflow_source) else {
        return Vec::new();
    };
    let Some(Triggers::Events(events)) = file.on else {
        return Vec::new();
    };

    let mut scopes = BTreeSet::new();
    for name in CODE_EVENTS {
        let Some(event) = events.get(name) else {
            continue;
        };
        let patterns: Vec<&str> = match event {
            Some(Event::Filter(Filter { paths: Some(paths) })) => paths
                .iter()
                .map(String::as_str)
                .filter(|path| !path.starts_with('!'))
                .collect(),
            _ => Vec::new(),
        };
        if patterns.is_empty() {
            return Vec::new();
        }
        scopes.extend(patterns.into_iter().map(PathScope::from_glob));
    }

    if scopes.iter().any(|scope| scope.as_str().is_empty()) {
        return Vec::new();
    }
    scopes.into_iter().collect()
}

#[cfg(test)]
mod tests;
