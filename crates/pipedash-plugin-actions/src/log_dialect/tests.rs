use super::*;

fn parse(line: &[u8]) -> ParsedLine<'_> {
    ActionsLogDialect.parse(line)
}

#[test]
fn bom_timestamp_and_group_are_stripped() {
    let parsed = parse(b"\xEF\xBB\xBF2024-05-01T10:00:05.1234567Z ##[group]Run make");
    assert_eq!(parsed.marker, LineMarker::GroupStart);
    assert_eq!(parsed.body, b"Run make");
    assert_eq!(
        parsed.timestamp.unwrap().timestamp_millis(),
        DateTime::parse_from_rfc3339("2024-05-01T10:00:05.123Z")
            .unwrap()
            .timestamp_millis()
    );
}

#[test]
fn bare_command_prefix_is_a_command() {
    let parsed = parse(b"2024-05-01T10:00:05.0000000Z [command]/usr/bin/git fetch");
    assert_eq!(parsed.marker, LineMarker::Command);
    assert_eq!(parsed.body, b"/usr/bin/git fetch");
}

#[test]
fn line_without_timestamp_keeps_its_body() {
    let parsed = parse(b"plain output line here");
    assert_eq!(parsed.timestamp, None);
    assert_eq!(parsed.marker, LineMarker::Plain);
    assert_eq!(parsed.body, b"plain output line here");
}

#[test]
fn malformed_timestamp_leaves_the_line_unchanged() {
    let line = b"2024-05-01T99:99:99.0000000Z hello";
    let parsed = parse(line);
    assert_eq!(parsed.timestamp, None);
    assert_eq!(parsed.body, line);
}

#[test]
fn error_body_follows_the_marker() {
    let parsed =
        parse(b"2024-05-01T10:00:05.0000000Z ##[error]Process completed with exit code 1.");
    assert_eq!(parsed.marker, LineMarker::Error);
    assert_eq!(parsed.body, b"Process completed with exit code 1.");
}

#[test]
fn run_groups_and_post_cleanup_start_steps() {
    let starts = |line: &[u8]| ActionsLogDialect.is_step_start(&parse(line));
    assert!(starts(
        b"2024-05-01T10:00:05.0Z ##[group]Run actions/checkout@v4"
    ));
    assert!(starts(b"2024-05-01T10:00:05.0Z Post job cleanup."));
    assert!(!starts(
        b"2024-05-01T10:00:05.0Z ##[group]Runner Image Provisioner"
    ));
    assert!(!starts(b"2024-05-01T10:00:05.0Z Run actions/checkout@v4"));
    assert!(!starts(b"2024-05-01T10:00:05.0Z ##[error]Run failed"));
}
