use pipedash_plugin_api::LogDialect;
use serde_json::json;

use super::*;

fn entries(value: serde_json::Value) -> Vec<LogEntry> {
    parse_entries(&value.to_string()).unwrap()
}

#[test]
fn every_tag_maps_to_its_marker_and_commands_keep_their_prompt() {
    let cases: [(&[u8], LineMarker, &[u8]); 4] = [
        (b". cloning", LineMarker::Plain, b"cloning"),
        (
            b"$ docker build .",
            LineMarker::Command,
            b"$ docker build .",
        ),
        (b"! build failed", LineMarker::Error, b"build failed"),
        (b"~ docker inspect", LineMarker::Debug, b"docker inspect"),
    ];
    for (line, marker, body) in cases {
        let parsed = DIALECT.parse(line);
        assert_eq!((parsed.marker, parsed.body), (marker, body), "{line:?}");
    }
    let untagged = DIALECT.parse(b"~no space");
    assert_eq!(untagged.marker, LineMarker::Plain);
}

#[test]
fn entries_render_in_order_with_commands_once_per_batch() {
    let rendered = render(&entries(json!([
        {"command": "docker build .", "output": "step 2", "type": "stdout", "hidden": false, "batch": 1, "order": 3},
        {"command": "docker build .", "output": "step 1", "type": "stdout", "hidden": false, "batch": 1, "order": 2},
        {"command": null, "output": "Starting deployment", "type": "stdout", "hidden": false, "batch": 0, "order": null},
        {"command": "docker build .", "output": "boom\nretry", "type": "stderr", "hidden": false, "batch": 2, "order": 4},
        {"command": "docker inspect x", "output": "{}", "type": "stdout", "hidden": true, "batch": 3, "order": 5}
    ])));

    assert_eq!(
        String::from_utf8(rendered).unwrap(),
        ". Starting deployment\n$ docker build .\n. step 1\n. step 2\n$ docker build .\n! boom\n! retry\n$ docker inspect x\n~ {}\n"
    );
}

#[test]
fn blank_logs_have_no_entries_and_broken_logs_are_an_api_error() {
    assert!(parse_entries("").unwrap().is_empty());
    assert!(parse_entries("  \n").unwrap().is_empty());
    assert!(matches!(parse_entries("{"), Err(PluginError::ApiError(_))));
}
