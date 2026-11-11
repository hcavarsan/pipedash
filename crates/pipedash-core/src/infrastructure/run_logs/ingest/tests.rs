use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
    PlainDialect,
};

use super::super::document::Document;
use super::super::testing::{
    doc_key,
    feed,
    finish,
    ingester,
};
use super::super::StoredDocMeta;
use super::*;

const MARKERS: [(&[u8], LineMarker); 8] = [
    (b"::section::", LineMarker::SectionStart { open: false }),
    (b"::section-open::", LineMarker::SectionStart { open: true }),
    (b"::continue::", LineMarker::OpenSection),
    (b"::endgroup::", LineMarker::GroupEnd),
    (b"::error::", LineMarker::Error),
    (b"::warning::", LineMarker::Warning),
    (b"::cmd::", LineMarker::Command),
    (b"::notice::", LineMarker::Notice),
];

struct StubDialect {
    infers_errors: bool,
}

impl LogDialect for StubDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let (timestamp, body) = split_stamp(line);
        for (prefix, marker) in MARKERS {
            if let Some(rest) = body.strip_prefix(prefix) {
                let body = rest.split(|&b| b == b'\r').next().unwrap_or(rest);
                return ParsedLine {
                    timestamp,
                    marker,
                    body,
                };
            }
        }
        ParsedLine {
            timestamp,
            marker: LineMarker::Plain,
            body,
        }
    }

    fn infers_errors(&self) -> bool {
        self.infers_errors
    }
}

fn split_stamp(line: &[u8]) -> (Option<DateTime<Utc>>, &[u8]) {
    let Some(rest) = line.strip_prefix(b"@") else {
        return (None, line);
    };
    let Some(space) = rest.iter().position(|&b| b == b' ') else {
        return (None, line);
    };
    std::str::from_utf8(&rest[..space])
        .ok()
        .and_then(|millis| millis.parse::<i64>().ok())
        .and_then(DateTime::from_timestamp_millis)
        .map_or((None, line), |ts| (Some(ts), &rest[space + 1..]))
}

static STUB: StubDialect = StubDialect {
    infers_errors: true,
};
static STUB_NATIVE: StubDialect = StubDialect {
    infers_errors: false,
};
static PLAIN: PlainDialect = PlainDialect;

type SectionRow = (String, Option<u32>, Option<u32>, u32, bool);

fn ingest_log(dialect: &'static dyn LogDialect, log: &str) -> (StoredDocMeta, Vec<String>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut ingester = ingester(dir.path(), doc_key(), 1, Vec::new(), BlockLimits::default());
    ingester.dialect = dialect;
    let meta = finish(feed(ingester, log.as_bytes(), log.len().max(1)), None).meta;
    let text = Document::open(dir.path())
        .expect("open")
        .text_block(0)
        .expect("block");
    let lines = std::str::from_utf8(&text)
        .expect("utf8")
        .lines()
        .map(str::to_string)
        .collect();
    (meta, lines)
}

fn section_rows(meta: &StoredDocMeta) -> Vec<SectionRow> {
    meta.sections
        .iter()
        .map(|s| (s.title.clone(), s.parent, s.header_line, s.end_line, s.open))
        .collect()
}

fn error_texts(dialect: &'static dyn LogDialect, log: &str) -> Vec<String> {
    let (meta, lines) = ingest_log(dialect, log);
    assert_eq!(meta.error_lines.len() as u32, meta.error_total);
    meta.error_lines
        .iter()
        .map(|&line| lines[line as usize].clone())
        .collect()
}

fn assert_errors(found: &[String], expected: &[&str]) {
    assert_eq!(found.len(), expected.len(), "{found:#?}");
    for (line, suffix) in found.iter().zip(expected) {
        assert!(
            line.ends_with(suffix),
            "{line:?} does not end with {suffix:?}"
        );
    }
}

const FLAT_LOG: &str = concat!(
    "@1745322209921 ::section::Setup\r\n",
    "@1745322209921 ./setup\r\n",
    "@1745322210694 ::section::Build\r\r\n",
    "@1745322210698 copy 50%\r@1745322210699 copy 100%\r\r\n",
    "@1745322210800 ::section-open::Test\r\n",
    "@1745322210900 ok\r\n",
    "@1745322211000 ::section::Lint\r\n",
    "@1745322212000 ::error::exited with status 1\r\n",
    "@1745322212100 ::section::Upload\r\n",
);

#[test]
fn section_starts_become_flat_sections() {
    let (meta, lines) = ingest_log(&STUB, FLAT_LOG);

    assert_eq!(
        section_rows(&meta),
        vec![
            ("Setup".to_string(), None, Some(0), 2, false),
            ("Build".to_string(), None, Some(2), 4, false),
            ("Test".to_string(), None, Some(4), 6, true),
            ("Lint".to_string(), None, Some(6), 8, true),
            ("Upload".to_string(), None, Some(8), 9, false),
        ]
    );
    assert_eq!(meta.sections[2].duration_ms, Some(100));
    assert_eq!(meta.sections[3].duration_ms, Some(1000));
    assert_eq!(meta.line_count, 9);
    assert_eq!(meta.error_lines, vec![7]);
    assert_eq!(meta.focus_line, Some(7));
    assert_eq!(meta.first_timestamp_ms, Some(1_745_322_209_921));

    assert_eq!(lines.len(), 9);
    assert_eq!(lines[3], "2025-04-22T11:43:30.699Z copy 100%");
    assert!(lines[7].ends_with(" exited with status 1"));
    assert!(lines.iter().all(|line| !line.contains("::")));
}

#[test]
fn text_after_a_group_end_follows_the_section() {
    let (meta, lines) = ingest_log(
        &STUB,
        concat!(
            "@1700000000000 ::section::Build\n",
            "compiling\n",
            "@1700000010000 ::endgroup::Done\n",
            "after the section\n",
        ),
    );

    assert_eq!(
        section_rows(&meta),
        vec![("Build".to_string(), None, Some(0), 2, false)]
    );
    assert_eq!(
        lines,
        vec![
            "2023-11-14T22:13:20.000Z Build",
            "compiling",
            "2023-11-14T22:13:30.000Z Done",
            "after the section",
        ]
    );
}

#[test]
fn continued_headers_open_the_section_without_adding_a_line() {
    let (meta, lines) = ingest_log(
        &STUB,
        concat!(
            "before\n",
            "::section::fetch\n",
            "::continue::Fetching sources\n",
            "progress\n",
            "::endgroup::\n",
            "after\n",
            "::section::build\n",
            "::continue::\n",
            "make\n",
        ),
    );

    assert_eq!(
        section_rows(&meta),
        vec![
            ("Fetching sources".to_string(), None, Some(1), 3, true),
            ("build".to_string(), None, Some(4), 6, true),
        ]
    );
    assert_eq!(
        lines,
        vec!["before", "fetch", "progress", "after", "build", "make"]
    );
}

#[test]
fn a_section_header_survives_a_carriage_return_clear() {
    let (meta, lines) = ingest_log(&STUB, "::section::Lint\rcleared\nmake\n");

    assert_eq!(
        section_rows(&meta),
        vec![("Lint".to_string(), None, Some(0), 2, false)]
    );
    assert_eq!(lines, vec!["Lint", "make"]);
}

#[test]
fn tool_failures_are_flagged_after_the_dialect_strips_its_framing() {
    let found = error_texts(
        &STUB,
        concat!(
            "@1700000000000 ::cmd::+ npm test\n",
            "@1700000001000 FAIL src/login.test.ts\n",
            "@1700000002000 npm ERR! Test failed.  See above for more details.\n",
            "@1700000003000 test result: FAILED. 2 passed; 1 failed\n",
            "@1700000004000 {\"level\":\"error\",\"msg\":\"tests failed\"}\n",
            "::cmd::make: *** [Makefile:12: build] Error 2\n",
            "@1700000005000 make: *** [Makefile:12: build] Error 2\n",
            "@1700000006000 exitcode '1': failure\n",
            "@1700000007000 Searching for test report files in directories named [test-results]\n",
        ),
    );

    assert_errors(
        &found,
        &[
            "FAIL src/login.test.ts",
            "npm ERR! Test failed.  See above for more details.",
            "test result: FAILED. 2 passed; 1 failed",
            "{\"level\":\"error\",\"msg\":\"tests failed\"}",
            "make: *** [Makefile:12: build] Error 2",
            "exitcode '1': failure",
        ],
    );
}

#[test]
fn dialects_with_native_error_markers_do_not_infer_more() {
    let found = error_texts(
        &STUB_NATIVE,
        concat!(
            "@1700000000000 ::cmd::+ make test\n",
            "@1700000001000 test result: FAILED. 0 passed; 1 failed\n",
            "@1700000002000 ERROR: script returned exit code 2\n",
            "@1700000003000 ::error::script exited with code 2\n",
            "@1700000004000 ::warning::flaky\n",
        ),
    );

    assert_errors(&found, &["script exited with code 2"]);
}

#[test]
fn errors_are_inferred_from_the_rendered_text() {
    let found = error_texts(
        &PLAIN,
        "\x1b[8mnote:////4IkPeNOtKgYQ==\x1b[0mERROR: Job failed: exit code 1\n",
    );

    assert_eq!(found, ["ERROR: Job failed: exit code 1"]);
}

#[test]
fn ordinary_output_is_not_flagged_as_an_error() {
    let found = error_texts(
        &PLAIN,
        concat!(
            "0 errors\n",
            "error_handler.rs\n",
            "cargo test --fail-fast\n",
            "errors: none\n",
            "test errors::parse_error ... ok\n",
            "PASS src/error.test.ts\n",
            "✓ handles error responses\n",
            "Compiling error-chain v0.12.0\n",
            "ERROR_COUNT=0\n",
            "No errors found\n",
            "0 FAILED\n",
            "Process exited with code 0\n",
            "exit status 0\n",
            "log_level=error\n",
            "Error handling guide\n",
            "level=info msg=\"retrying after error\"\n",
            "Finished in 0.5s (0 failures)\n",
            "exit code 2 is documented below\n",
        ),
    );

    assert_errors(&found, &[]);
}
