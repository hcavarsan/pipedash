use regex::Regex;
use tokio_util::sync::CancellationToken;

use super::super::testing::{
    cache,
    ingest,
    small_limits,
};
use super::super::{
    BlockLimits,
    MAX_LINE_BYTES,
    SEARCH_MATCH_CAP,
    SEARCH_MAX_QUERY_CHARS,
};
use super::search::search_regex;
use super::*;
use crate::domain::run_logs::{
    LogSearchQuery,
    LogSearchResult,
};
use crate::domain::{
    DomainError,
    LineKind,
};

fn sample() -> (String, Vec<(String, LineKind)>) {
    let mut input = String::new();
    let mut expected = Vec::new();
    for i in 0..40 {
        let stamp = format!("2026-01-01T10:00:{i:02}.000Z ");
        let (raw, emitted) = match i {
            2 => (
                "##[group]Outer".to_string(),
                Some(("Outer".to_string(), LineKind::Group)),
            ),
            3 => (
                "##[group]Inner".to_string(),
                Some(("Inner".to_string(), LineKind::Group)),
            ),
            5 | 6 => ("##[endgroup]".to_string(), None),
            10 | 20 | 30 => (
                format!("##[error]boom {i}"),
                Some((format!("boom {i}"), LineKind::Error)),
            ),
            21 => (
                "\u{1b}[31mred \u{e9}\u{1f600}\u{1b}[0m tail".to_string(),
                Some(("red \u{e9}\u{1f600} tail".to_string(), LineKind::Plain)),
            ),
            22 => (
                "a\rb\rfinal".to_string(),
                Some(("final".to_string(), LineKind::Plain)),
            ),
            35 => (
                "##[group]Unclosed".to_string(),
                Some(("Unclosed".to_string(), LineKind::Group)),
            ),
            _ => {
                let text = format!("plain line {i} \u{fc}n\u{ef}code");
                (text.clone(), Some((text, LineKind::Plain)))
            }
        };
        input.push_str(&stamp);
        input.push_str(&raw);
        input.push_str(if i % 2 == 0 { "\r\n" } else { "\n" });
        expected.extend(emitted);
    }
    (input, expected)
}

fn doc_from(lines: &[&str], limits: BlockLimits) -> (tempfile::TempDir, Document) {
    let dir = tempfile::tempdir().expect("tempdir");
    let data: String = lines.iter().map(|l| format!("{l}\n")).collect();
    ingest(dir.path(), data.as_bytes(), 1 << 16, Vec::new(), limits);
    let doc = Document::open(dir.path()).expect("open");
    (dir, doc)
}

fn query(text: &str, regex: bool, case_sensitive: bool) -> Regex {
    search_regex(&LogSearchQuery {
        text: text.into(),
        regex,
        case_sensitive,
    })
    .expect("pattern")
    .expect("non-empty")
}

fn run(doc: &Document, regex: &Regex) -> LogSearchResult {
    doc.search(regex, &CancellationToken::new())
        .expect("search")
}

#[test]
fn run_logs_document_reads_exact_text_across_blocks() {
    let (input, expected) = sample();
    let dir = tempfile::tempdir().expect("tempdir");
    let outcome = ingest(dir.path(), input.as_bytes(), 7, Vec::new(), small_limits());
    assert!(!outcome.truncated);
    let meta = &outcome.meta;
    assert_eq!(meta.line_count as usize, expected.len());
    assert_eq!(meta.line_count, 38);
    assert_eq!(meta.error_total, 3);
    let error_lines: Vec<u32> = expected
        .iter()
        .enumerate()
        .filter(|(_, (_, kind))| *kind == LineKind::Error)
        .map(|(i, _)| i as u32)
        .collect();
    assert_eq!(meta.error_lines, error_lines);
    assert_eq!(meta.sections.len(), 3);
    let unclosed = meta
        .sections
        .iter()
        .find(|s| s.title == "Unclosed")
        .expect("unclosed");
    assert_eq!(unclosed.end_line, 38);
    let inner = meta
        .sections
        .iter()
        .find(|s| s.title == "Inner")
        .expect("inner");
    assert_eq!(
        inner.parent,
        meta.sections
            .iter()
            .find(|s| s.title == "Outer")
            .map(|s| s.id)
    );

    let doc = Document::open(dir.path()).expect("open");
    assert!(doc.block_count() >= 9);
    let cache = cache();
    let page = doc.read_lines(&cache, 10, 15, 16384).expect("page");
    assert_eq!(page.start, 10);
    assert_eq!(page.lines.len(), 15);
    for (line, (text, kind)) in page.lines.iter().zip(&expected[10..25]) {
        assert_eq!(&line.text, text);
        assert_eq!(&line.kind, kind);
        assert!(line.ts.is_some() || text == "final");
    }
    let all = doc.read_lines(&cache, 0, 5000, 16384).expect("all");
    assert_eq!(all.lines.len(), 38);
    for (line, (text, kind)) in all.lines.iter().zip(&expected) {
        assert_eq!((&line.text, &line.kind), (text, kind));
    }
    let red = expected
        .iter()
        .position(|(t, _)| t.starts_with("red"))
        .expect("red");
    assert_eq!(all.lines[red].spans, vec![(0, 7, 0)]);
    assert!(doc
        .read_lines(&cache, 38, 10, 4096)
        .expect("past end")
        .lines
        .is_empty());
}

#[test]
fn run_logs_document_cuts_long_lines_by_utf16_units() {
    let ascii = "x".repeat(1000);
    let wide = "\u{e9}".repeat(1000);
    let emoji = format!("\u{1b}[31m{}\u{1b}[0m", "\u{1f600}".repeat(300));
    let lines = [ascii.as_str(), wide.as_str(), emoji.as_str()];
    let (_dir, doc) = doc_from(&lines, BlockLimits::default());
    let page = doc.read_lines(&cache(), 0, 3, 256).expect("page");
    assert_eq!(page.lines[0].text.len(), 256);
    assert_eq!(page.lines[0].omitted, 744);
    assert_eq!(page.lines[1].text.chars().count(), 256);
    assert_eq!(page.lines[1].omitted, 744);
    assert_eq!(page.lines[2].text.chars().count(), 128);
    assert_eq!(page.lines[2].omitted, 600 - 256);
    assert_eq!(page.lines[2].spans, vec![(0, 256, 0)]);
    let full = doc.read_lines(&cache(), 0, 1, 1 << 20).expect("full");
    assert_eq!(full.lines[0].omitted, 0);
}

#[test]
fn run_logs_document_splits_giant_line_on_char_boundaries() {
    let giant = "\u{20ac}".repeat(1_048_576);
    let dir = tempfile::tempdir().expect("tempdir");
    let outcome = ingest(
        dir.path(),
        giant.as_bytes(),
        1 << 16,
        Vec::new(),
        BlockLimits::default(),
    );
    assert!(outcome.meta.line_count >= 3);
    let doc = Document::open(dir.path()).expect("open");
    let cache = cache();
    let mut joined = String::new();
    let mut start = 0;
    while start < outcome.meta.line_count {
        let page = doc.read_lines(&cache, start, 10, 1 << 20).expect("page");
        for line in &page.lines {
            assert!(line.text.len() <= MAX_LINE_BYTES);
            assert_eq!(line.omitted, 0);
            joined.push_str(&line.text);
        }
        start += page.lines.len() as u32;
    }
    assert_eq!(joined, giant);
}

#[test]
fn run_logs_document_truncates_at_text_limit() {
    let lines: Vec<String> = (0..50).map(|_| "0123456789".to_string()).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let limits = BlockLimits {
        max_doc_text_bytes: 100,
        ..small_limits()
    };
    let (dir, doc) = doc_from(&refs, limits);
    assert!(doc.meta().truncated);
    assert_eq!(doc.meta().line_count, 9);
    assert_eq!(doc.meta().text_bytes, 99);
    let page = doc.read_lines(&cache(), 0, 100, 4096).expect("page");
    assert_eq!(page.lines.len(), 9);
    drop(dir);
}

#[test]
fn run_logs_document_empty_input_is_readable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let outcome = ingest(dir.path(), b"", 16, Vec::new(), BlockLimits::default());
    assert_eq!(outcome.meta.line_count, 0);
    let doc = Document::open(dir.path()).expect("open");
    assert_eq!(doc.block_count(), 0);
    assert!(doc
        .read_lines(&cache(), 0, 10, 4096)
        .expect("page")
        .lines
        .is_empty());
    assert_eq!(run_empty(&doc), LogSearchResult::default());
}

fn run_empty(doc: &Document) -> LogSearchResult {
    run(doc, &query("x", false, true))
}

#[test]
fn run_logs_document_rejects_corrupt_files() {
    let lines: Vec<String> = (0..20).map(|i| format!("line {i}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (dir, _) = doc_from(&refs, small_limits());
    let index_path = dir.path().join(INDEX_FILE);
    let index = fs::read(&index_path).expect("index");
    let blocks_path = dir.path().join(BLOCKS_FILE);
    let blocks = fs::read(&blocks_path).expect("blocks");

    let mut longer = index.clone();
    longer.push(0);
    fs::write(&index_path, longer).expect("write");
    assert!(matches!(Document::open(dir.path()), Err(DocError::Corrupt)));

    let mut shifted = index.clone();
    shifted[12] = 1;
    fs::write(&index_path, shifted).expect("write");
    assert!(matches!(Document::open(dir.path()), Err(DocError::Corrupt)));

    fs::write(&index_path, &index[..INDEX_RECORD_BYTES]).expect("write");
    let one_block = Document::open(dir.path()).expect("open");
    assert_eq!(one_block.block_count(), 1);
    fs::write(&index_path, &index).expect("write");

    fs::write(&blocks_path, &blocks[..blocks.len() - 1]).expect("write");
    assert!(matches!(Document::open(dir.path()), Err(DocError::Corrupt)));

    fs::write(&blocks_path, vec![0u8; blocks.len()]).expect("write");
    let doc = Document::open(dir.path()).expect("open");
    assert!(matches!(
        doc.read_lines(&cache(), 0, 5, 4096),
        Err(DocError::Corrupt)
    ));
    assert!(matches!(doc.text_block(0), Err(DocError::Corrupt)));
}

#[test]
fn run_logs_search_matches_in_order_across_blocks() {
    let lines = [
        "start",
        "an Error here",
        "x",
        "y",
        "z",
        "error again",
        "x",
        "y",
        "z",
        "ERROR end",
        "x",
        "y",
    ];
    let (_dir, doc) = doc_from(&lines, small_limits());
    assert!(doc.block_count() >= 3);
    let found = run(&doc, &query("error", false, false));
    assert_eq!(found.matches, vec![(1, 3, 8), (5, 0, 5), (9, 0, 5)]);
    assert_eq!((found.total, found.capped), (3, false));
    let sensitive = run(&doc, &query("error", false, true));
    assert_eq!(sensitive.matches, vec![(5, 0, 5)]);
}

#[test]
fn run_logs_search_regex_and_anchors() {
    let (_dir, doc) = doc_from(
        &["err", "error", "xx", "boom", "boomerang", "a boom", "boom"],
        small_limits(),
    );
    let found = run(&doc, &query("err(or)?", true, true));
    assert_eq!(found.matches, vec![(0, 0, 3), (1, 0, 5)]);
    let exact = run(&doc, &query("^boom$", true, true));
    assert_eq!(exact.matches, vec![(3, 0, 4), (6, 0, 4)]);
    let empties = run(&doc, &query("x*", true, true));
    assert_eq!(empties.matches, vec![(2, 0, 2)]);
}

#[test]
fn run_logs_search_rejects_bad_patterns() {
    let bad = |text: &str, regex: bool| {
        search_regex(&LogSearchQuery {
            text: text.into(),
            regex,
            case_sensitive: true,
        })
    };
    assert!(
        matches!(bad("(", true), Err(DomainError::InvalidConfig(m)) if m.starts_with("Invalid search pattern"))
    );
    assert!(matches!(
        bad(r"(?-u:\xA9)", true),
        Err(DomainError::InvalidConfig(_))
    ));
    assert!(bad("(", false).expect("literal").is_some());
    assert!(bad("", true).expect("empty").is_none());
    let long = "a".repeat(SEARCH_MAX_QUERY_CHARS + 1);
    assert!(
        matches!(bad(&long, false), Err(DomainError::InvalidConfig(m)) if m == "Search text is too long")
    );
    assert!(bad(&long[1..], false).is_ok());
}

#[test]
fn run_logs_search_reports_utf16_offsets() {
    let (_dir, doc) = doc_from(&["a\u{e9}\u{1f600}b b", "\u{1f600}"], small_limits());
    assert_eq!(
        run(&doc, &query("b", false, true)).matches,
        vec![(0, 4, 5), (0, 6, 7)]
    );
    assert_eq!(
        run(&doc, &query("\u{1f600}", false, true)).matches,
        vec![(0, 2, 4), (1, 0, 2)]
    );
    assert_eq!(
        run(&doc, &query("\u{e9}", false, true)).matches,
        vec![(0, 1, 2)]
    );
}

#[test]
fn run_logs_search_is_independent_of_block_size() {
    let lines: Vec<String> = (0..100)
        .map(|i| {
            if i % 7 == 0 {
                format!("caf\u{e9} {i} match")
            } else {
                format!("noise {i}")
            }
        })
        .collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (_a, small) = doc_from(&refs, small_limits());
    let (_b, large) = doc_from(&refs, BlockLimits::default());
    assert!(small.block_count() > large.block_count());
    let pattern = query("match|noise 9", true, false);
    assert_eq!(run(&small, &pattern), run(&large, &pattern));
    assert_eq!(run(&small, &pattern).total, 24);
}

#[test]
fn run_logs_search_caps_matches_but_counts_all() {
    let lines = vec!["x"; 12_000];
    let (_dir, doc) = doc_from(&lines, BlockLimits::default());
    let found = run(&doc, &query("x", false, true));
    assert_eq!(found.matches.len(), SEARCH_MATCH_CAP);
    assert_eq!(found.total, 12_000);
    assert!(found.capped);
    assert!(found.matches.windows(2).all(|w| w[0].0 < w[1].0));
}

#[test]
fn run_logs_search_honours_cancellation() {
    let (_dir, doc) = doc_from(&["a", "b", "c", "d", "e"], small_limits());
    let token = CancellationToken::new();
    token.cancel();
    assert!(matches!(
        doc.search(&query("a", false, true), &token),
        Err(DocError::Cancelled)
    ));
}

#[test]
fn run_logs_document_text_block_renders_timestamps() {
    let input = "2026-01-01T10:00:00.250Z hello\nno stamp\n";
    let dir = tempfile::tempdir().expect("tempdir");
    ingest(
        dir.path(),
        input.as_bytes(),
        5,
        Vec::new(),
        BlockLimits::default(),
    );
    let doc = Document::open(dir.path()).expect("open");
    let text = doc.text_block(0).expect("block");
    assert_eq!(&text[..], b"2026-01-01T10:00:00.250Z hello\nno stamp\n");
    assert!(doc.text_block(1).is_err());
}
