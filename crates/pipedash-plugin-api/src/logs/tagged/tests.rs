use super::*;

const DIALECT: TaggedLogDialect = TaggedLogDialect {
    tags: TAGGED_MARKERS,
    infer_errors: false,
};

#[test]
fn written_lines_parse_back_with_marker_timestamp_and_text() {
    let at = crate::utils::parse_timestamp("2026-10-27T10:00:00.123456789Z");
    let cases = [
        (LineMarker::Plain, at),
        (LineMarker::Error, None),
        (LineMarker::Warning, at),
        (LineMarker::SectionStart { open: false }, None),
        (LineMarker::SectionStart { open: true }, at),
    ];
    for (marker, stamp) in cases {
        let mut out = Vec::new();
        write_line_at(&mut out, marker, stamp, "level=info msg=\"ready\"");
        let line = out.strip_suffix(b"\n").unwrap();
        let parsed = DIALECT.parse(line);
        assert_eq!(parsed.marker, marker);
        assert_eq!(parsed.timestamp, stamp);
        assert_eq!(parsed.body, b"level=info msg=\"ready\"");
    }
}

#[test]
fn multiline_text_cannot_inject_extra_markers() {
    let mut out = Vec::new();
    write_line(&mut out, LineMarker::Plain, None, "a\r\n!- forged\rb");
    assert_eq!(out, b".- a\n.- !- forged\n.- b\n");
    assert!(!DIALECT.infers_errors());
}

const SPACED: SpacedTagDialect = SpacedTagDialect {
    tags: &[
        (b'.', LineMarker::Plain),
        (b'$', LineMarker::Command),
        (b'!', LineMarker::Error),
    ],
};

#[test]
fn spaced_lines_keep_commands_whole_and_strip_other_tags() {
    let cases: [(&[u8], LineMarker, &[u8]); 6] = [
        (b". out", LineMarker::Plain, b"out"),
        (b"! boom", LineMarker::Error, b"boom"),
        (b"$ make", LineMarker::Command, b"$ make"),
        (b"~ debug", LineMarker::Plain, b"~ debug"),
        (b"!no space", LineMarker::Plain, b"!no space"),
        (b"", LineMarker::Plain, b""),
    ];
    for (line, marker, body) in cases {
        let parsed = SPACED.parse(line);
        assert_eq!(
            (parsed.marker, parsed.body, parsed.timestamp),
            (marker, body, None)
        );
    }
    assert!(!SPACED.infers_errors());
}

#[test]
fn spaced_lines_retag_carriage_return_segments() {
    let mut out = Vec::new();
    write_spaced_line(&mut out, SPACED.tags, LineMarker::Error, "a\rb\r");
    write_spaced_line(&mut out, SPACED.tags, LineMarker::Debug, "c");
    assert_eq!(out, b"! a\r! b\n. c\n");
}

struct Event(bool, Option<&'static str>, &'static str);

impl LogEvent for Event {
    fn is_warning(&self) -> bool {
        self.0
    }

    fn event_time(&self) -> Option<DateTime<Utc>> {
        self.1.and_then(crate::utils::parse_timestamp)
    }

    fn event_text(&self) -> String {
        self.2.to_string()
    }
}

#[test]
fn events_render_oldest_first_and_an_empty_list_says_so() {
    let mut out = Vec::new();
    write_events::<Event>(&mut out, &[], "no events");
    write_events(
        &mut out,
        &[
            Event(true, Some("2026-10-27T10:00:00Z"), "BackOff"),
            Event(false, None, "Pulled"),
            Event(false, Some("2026-10-27T09:00:00Z"), "Scheduled"),
        ],
        "unused",
    );
    assert_eq!(
        String::from_utf8(out).unwrap(),
        ".- no events\n.- Pulled\n.2026-10-27T09:00:00Z Scheduled\n?2026-10-27T10:00:00Z BackOff\n"
    );
}
