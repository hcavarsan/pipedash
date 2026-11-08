use super::*;

fn parse(line: &[u8]) -> ParsedLine<'_> {
    GitlabLogDialect.parse(line)
}

fn millis(parsed: &ParsedLine<'_>) -> Option<i64> {
    parsed.timestamp.map(|time| time.timestamp_millis())
}

#[test]
fn runner_timestamp_header_is_parsed_and_stripped() {
    for (line, body) in [
        (
            b"2024-05-22T08:46:32.123456Z 00O Running with gitlab-runner 17.0.0".as_slice(),
            b"Running with gitlab-runner 17.0.0".as_slice(),
        ),
        (b"2024-05-22T08:46:32.123456Z 01E boom", b"boom"),
        (
            b"2024-05-22T08:46:32.123456Z 00O+\x1b[0K\x1b[36;1mPreparing\x1b[0;m",
            b"\x1b[0K\x1b[36;1mPreparing\x1b[0;m",
        ),
        (b"2024-05-22T08:46:32.123456Z 00O ", b""),
    ] {
        let parsed = parse(line);
        assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
        assert_eq!(parsed.body, body, "{line:?}");
        assert_eq!(millis(&parsed), Some(1_716_367_592_123), "{line:?}");
    }
}

#[test]
fn lines_without_a_valid_header_keep_their_bytes() {
    for line in [
        b"plain output".as_slice(),
        b"",
        b"2024-05-22T08:46:32.123456Z 00O",
        b"2024-05-22T08:46:32.123456Z 0zO text",
        b"2024-05-22T08:46:32.123456Z 00X text",
        b"2024-05-22T08:46:32.123456Z 00O?text",
        b"2024-05-22T08:46:32.123Z 00O text 12345",
        b"2024-13-22T08:46:32.123456Z 00O text",
    ] {
        let parsed = parse(line);
        assert_eq!(parsed.timestamp, None, "{line:?}");
        assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
        assert_eq!(parsed.body, line);
    }
}

#[test]
fn section_start_opens_a_section_with_its_header() {
    let parsed = parse(
        b"section_start:1700000000:prepare_executor\r\x1b[0K\x1b[0K\x1b[36;1mPreparing the \"docker\" executor\x1b[0;m",
    );
    assert_eq!(parsed.marker, LineMarker::SectionStart { open: true });
    assert_eq!(
        parsed.body,
        b"\x1b[0K\x1b[36;1mPreparing the \"docker\" executor\x1b[0;m"
    );
    assert_eq!(millis(&parsed), Some(1_700_000_000_000));
}

#[test]
fn collapsed_option_closes_the_section() {
    let cases: [(&[u8], bool); 5] = [
        (b"section_start:1:a[collapsed=true]\r\x1b[0KA", false),
        (b"section_start:1:a[collapsed=false]\r\x1b[0KA", true),
        (
            b"section_start:1:a[hide_duration=true, collapsed=true]\r\x1b[0KA",
            false,
        ),
        (
            b"section_start:1:a[collapsed=true,hide_duration=true]\r\x1b[0KA",
            false,
        ),
        (b"section_start:1:a[hide_duration=true]\r\x1b[0KA", true),
    ];
    for (line, open) in cases {
        let parsed = parse(line);
        assert_eq!(parsed.marker, LineMarker::SectionStart { open }, "{line:?}");
        assert_eq!(parsed.body, b"A", "{line:?}");
    }
}

#[test]
fn section_start_without_a_header_is_titled_by_its_name() {
    let parsed = parse(b"2024-05-22T08:46:32.123456Z 00O section_start:1:prepare_script\r\x1b[0K");
    assert_eq!(parsed.marker, LineMarker::SectionStart { open: true });
    assert_eq!(parsed.body, b"prepare_script");
    assert_eq!(millis(&parsed), Some(1_716_367_592_123));
}

#[test]
fn section_end_closes_the_section() {
    let parsed = parse(b"section_end:1700000007:step_script\r\x1b[0K");
    assert_eq!(parsed.marker, LineMarker::GroupEnd);
    assert_eq!(parsed.body, b"");
    assert_eq!(millis(&parsed), Some(1_700_000_007_000));

    let stamped = parse(b"2024-05-22T08:46:32.123456Z 00O \x1b[0Ksection_end:1:a\r\x1b[0K\r");
    assert_eq!(stamped.marker, LineMarker::GroupEnd);
    assert_eq!(millis(&stamped), Some(1_716_367_592_123));
}

#[test]
fn end_and_start_on_one_line_start_the_next_section() {
    let parsed = parse(
        b"section_end:1700000005:prepare_executor\r\x1b[0Ksection_start:1700000006:prepare_script\r\x1b[0KPreparing environment\r",
    );
    assert_eq!(parsed.marker, LineMarker::SectionStart { open: true });
    assert_eq!(parsed.body, b"Preparing environment");
    assert_eq!(millis(&parsed), Some(1_700_000_006_000));
}

#[test]
fn text_after_a_section_end_closes_the_section_and_stays_a_line() {
    let parsed = parse(
        b"section_end:1700000010:cleanup_file_variables\r\x1b[0K\x1b[32;1mJob succeeded\x1b[0;m",
    );
    assert_eq!(parsed.marker, LineMarker::GroupEnd);
    assert_eq!(parsed.body, b"\x1b[32;1mJob succeeded\x1b[0;m");
}

#[test]
fn markers_follow_color_resets_but_not_output() {
    let parsed = parse(b"\x1b[0;msection_start:1:build\r\x1b[0KBuild");
    assert_eq!(parsed.marker, LineMarker::SectionStart { open: true });
    assert_eq!(parsed.body, b"Build");

    let output = b"echo section_start:1:build\r\x1b[0KBuild";
    assert_eq!(parse(output).marker, LineMarker::Plain);
    assert_eq!(parse(output).body, output);
}

#[test]
fn malformed_markers_are_output() {
    for line in [
        b"section_start:1:my_$ection\r\x1b[0Khello".as_slice(),
        b"section_start:1:<a_tag>\r\x1b[0Khello",
        b"section_start:1:name hello",
        b"section_start:1:name\rhello",
        b"section_start::name\r\x1b[0Khello",
        b"section_start:1:\r\x1b[0Khello",
        b"section_start:1:name[collapsed=true\r\x1b[0Khello",
        b"section_end:x:name\r\x1b[0K",
    ] {
        let parsed = parse(line);
        assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
        assert_eq!(parsed.body, line);
    }
}
