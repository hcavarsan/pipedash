use pipedash_plugin_api::LogDialect;
use serde_json::json;

use super::*;

fn messages(value: serde_json::Value) -> Vec<OutputMessage> {
    serde_json::from_value(value).unwrap()
}

fn parsed(rendered: &[u8]) -> Vec<(LineMarker, Option<DateTime<Utc>>, String)> {
    rendered
        .strip_suffix(b"\n")
        .unwrap()
        .split(|b| *b == b'\n')
        .map(|line| {
            let visible = line.rsplit(|b| *b == b'\r').next().unwrap();
            let parsed = DIALECT.parse(visible);
            (
                parsed.marker,
                parsed.timestamp,
                String::from_utf8_lossy(parsed.body).into_owned(),
            )
        })
        .collect()
}

#[test]
fn steps_become_sections_and_lines_join_across_output_chunks() {
    let output = messages(json!([
        {"message": "Cloning into 'repo'...\r\nrem", "time": "2026-01-01T00:00:01Z", "type": "out"},
        {"message": "ote: done\r\n", "time": "2026-01-01T00:00:02Z", "type": "err"},
        {"message": "no newline at end", "time": "2026-01-01T00:00:03Z", "type": "out"}
    ]));
    let started = "2026-01-01T00:00:00Z".parse().ok();
    let rendered = render(&[
        Section {
            name: "Checkout code".into(),
            failed: false,
            started_at: started,
            output: Some(&output),
        },
        Section {
            name: "Run tests".into(),
            failed: true,
            started_at: None,
            output: Some(&[]),
        },
        Section {
            name: "Huge step".into(),
            failed: false,
            started_at: None,
            output: None,
        },
    ]);

    let at = |second: u32| {
        Some(
            format!("2026-01-01T00:00:0{second}Z")
                .parse::<DateTime<Utc>>()
                .unwrap(),
        )
    };
    assert_eq!(
        parsed(&rendered),
        [
            (
                LineMarker::SectionStart { open: false },
                started,
                "Checkout code".to_string()
            ),
            (
                LineMarker::Plain,
                at(1),
                "Cloning into 'repo'...".to_string()
            ),
            (LineMarker::Plain, at(1), "remote: done".to_string()),
            (LineMarker::Plain, at(3), "no newline at end".to_string()),
            (
                LineMarker::SectionStart { open: true },
                None,
                "Run tests".to_string()
            ),
            (
                LineMarker::SectionStart { open: false },
                None,
                "Huge step".to_string()
            ),
            (LineMarker::Plain, None, OMITTED.to_string()),
        ]
    );
}

#[test]
fn progress_carriage_returns_keep_the_tag_on_the_visible_part() {
    let output = messages(json!([{"message": "10%\r55%\r100%\r\n", "type": "out"}]));
    let rendered = render(&[Section {
        name: "Download".into(),
        failed: false,
        started_at: None,
        output: Some(&output),
    }]);

    assert_eq!(rendered, b"#- Download\n.- 10%\r.- 55%\r.- 100%\n");
}

#[test]
fn output_cannot_forge_a_section() {
    let output = messages(json!([{"message": "+- fake section\n", "type": "out"}]));
    let rendered = render(&[Section {
        name: "Run".into(),
        failed: false,
        started_at: None,
        output: Some(&output),
    }]);

    assert_eq!(
        parsed(&rendered)[1],
        (LineMarker::Plain, None, "+- fake section".to_string())
    );
}

#[test]
fn untagged_lines_stay_plain() {
    for line in [b"".as_slice(), b"plain", b"#nospace", b"x- unknown"] {
        let parsed = DIALECT.parse(line);
        assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
        assert_eq!(parsed.body, line, "{line:?}");
    }
}
