use super::*;

fn scopes(source: &str) -> Vec<String> {
    trigger_scopes(source)
        .iter()
        .map(|scope| scope.as_str().to_string())
        .collect()
}

#[test]
fn push_and_pull_request_paths_become_sorted_unique_scopes() {
    let source = "on:\n  push:\n    branches: [main]\n    paths:\n      - 'services/api/**'\n      - Cargo.toml\n  pull_request:\n    paths:\n      - 'services/api/**'\n      - 'docs/*.md'\n      - '!docs/internal/**'\njobs: {}\n";

    assert_eq!(scopes(source), ["Cargo.toml", "docs", "services/api"]);
}

#[test]
fn an_event_without_a_paths_filter_covers_the_whole_repository() {
    let cases = [
        "on: push\n",
        "on: [push, pull_request]\n",
        "on:\n  push:\n  pull_request:\n    paths: ['src/**']\n",
        "on:\n  push:\n    paths: ['src/**']\n  pull_request:\n    branches: [main]\n",
        "on:\n  push:\n    paths: ['!docs/**']\n",
        "on:\n  push:\n    paths: ['**/*.rs']\n",
    ];
    for source in cases {
        assert!(scopes(source).is_empty(), "{source}");
    }
}

#[test]
fn workflows_without_code_triggers_or_with_broken_yaml_have_no_scopes() {
    let cases = [
        "on:\n  schedule:\n    - cron: '0 0 * * *'\n  workflow_dispatch:\n",
        "name: only a name\n",
        "on: [unclosed\n",
        "",
    ];
    for source in cases {
        assert!(scopes(source).is_empty(), "{source}");
    }
}

#[test]
fn unrelated_event_shapes_do_not_hide_the_path_filters() {
    let source = "on:\n  schedule:\n    - cron: '0 0 * * *'\n  push:\n    paths: ['apps/web/**']\n  pull_request:\n    paths: ['apps/web/**']\n";

    assert_eq!(scopes(source), ["apps/web"]);
}
