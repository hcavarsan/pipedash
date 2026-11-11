use super::*;

const LOG: &[u8] = b"first\nsecond\n\nfourth\nfifth";

fn position_after(chunks: &[&[u8]]) -> LogPosition {
    let mut tally = Tally::default();
    for chunk in chunks {
        tally.feed(chunk);
    }
    tally.position(1, 2, 3)
}

fn first_line_hash(bytes: &'static [u8]) -> Option<u64> {
    let mut scan = BoundaryScan::new();
    match scan.feed(Bytes::from_static(bytes)) {
        Scan::Line(hash, _) => Some(hash),
        _ => scan.finish(),
    }
}

#[test]
fn position_does_not_depend_on_how_the_bytes_were_chunked() {
    let whole = position_after(&[LOG]);

    assert_eq!(whole.lines, 5);
    assert!(whole.open);
    for split in 0..=LOG.len() {
        assert_eq!(
            position_after(&[&LOG[..split], &LOG[split..]]),
            whole,
            "{split}"
        );
    }
    let bytewise: Vec<&[u8]> = LOG.chunks(1).collect();
    assert_eq!(position_after(&bytewise), whole);
}

#[test]
fn a_terminated_log_is_not_open_and_an_empty_one_has_no_lines() {
    let terminated = position_after(&[b"a\nb\n"]);
    let empty = position_after(&[]);

    assert_eq!((terminated.lines, terminated.open), (2, false));
    assert_eq!((empty.lines, empty.open), (0, false));
}

#[test]
fn resumed_tally_continues_the_line_count_and_last_line() {
    let mut prefix = position_after(&[b"a\nb\n"]);
    assert!(prefix.settle(first_line_hash(b"b\n").unwrap()));
    let mut resumed = Tally::resume(&prefix);
    resumed.feed(b"c\nd\n");

    let mut whole = position_after(&[b"a\nb\nc\nd\n"]);
    whole.first_line = Some(0);
    whole.anchor = None;
    assert_eq!(resumed.position(1, 2, 3), whole);
}

#[test]
fn the_anchor_is_the_latest_pair_of_neighbouring_lines_that_differ() {
    let position = position_after(&[b"a\nb\nb\nb\n"]);

    assert_eq!(position.probe_line(), Some(1));
    assert!(position_after(&[b"a\n"]).probe_line().is_none());
    assert!(position_after(&[b"z\nz\nz\n"]).probe_line().is_none());
    assert!(position_after(&[]).probe_line().is_none());
}

#[test]
fn probing_a_line_tells_the_first_line_number_of_the_server() {
    let hash_a = first_line_hash(b"a\n").unwrap();
    let hash_b = first_line_hash(b"b\n").unwrap();
    let hash_c = first_line_hash(b"c\n").unwrap();

    for (probed, first_line) in [(hash_b, Some(0)), (hash_a, Some(1)), (hash_c, None)] {
        let mut position = position_after(&[b"a\nb\n"]);

        assert_eq!(position.settle(probed), first_line.is_some());
        assert_eq!(position.first_line, first_line);
        assert_eq!(position.anchor.is_some(), first_line.is_none());
    }
}

#[test]
fn boundary_line_is_the_last_known_line_in_the_servers_numbering() {
    let mut position = position_after(&[b"a\nb\nc\n"]);

    assert_eq!(position.boundary_line(), None);
    position.first_line = Some(0);
    assert_eq!(position.boundary_line(), Some(2));
    position.first_line = Some(1);
    assert_eq!(position.boundary_line(), Some(3));
}

#[test]
fn boundary_scan_hashes_the_first_line_across_chunks_and_returns_the_rest() {
    let mut scan = BoundaryScan::new();

    assert!(matches!(scan.feed(Bytes::from_static(b"he")), Scan::More));
    let Scan::Line(hash, rest) = scan.feed(Bytes::from_static(b"llo\nworld\n")) else {
        panic!("the line ends in this chunk");
    };

    assert_eq!(Some(hash), first_line_hash(b"hello\n"));
    assert_eq!(&rest[..], b"world\n");
    assert_eq!(
        first_line_hash(b"hello"),
        first_line_hash(b"hello\n"),
        "an unterminated line at the end of the body is still a line"
    );
    assert_eq!(first_line_hash(b""), None);
    assert_eq!(
        first_line_hash(b"\n"),
        Some(CONTENT_HASH_SEED),
        "an empty first line is still a line"
    );
}

#[test]
fn boundary_scan_gives_up_on_a_line_longer_than_the_limit_without_keeping_it() {
    let chunk = Bytes::from(vec![b'x'; 64 * 1024]);
    let mut scan = BoundaryScan::new();
    let mut fed = 0;

    let verdict = loop {
        match scan.feed(chunk.clone()) {
            Scan::More => fed += chunk.len(),
            other => break other,
        }
    };

    assert!(matches!(verdict, Scan::TooLong));
    assert!(fed <= BOUNDARY_LINE_LIMIT);
    assert!(scan.scanned <= BOUNDARY_LINE_LIMIT + 1);
}

#[test]
fn boundary_scan_accepts_a_line_of_exactly_the_limit() {
    let mut scan = BoundaryScan::new();
    let mut line = vec![b'x'; BOUNDARY_LINE_LIMIT];
    line.push(b'\n');

    assert!(matches!(
        scan.feed(Bytes::from(line)),
        Scan::Line(_, rest) if rest.is_empty()
    ));
    let mut scan = BoundaryScan::new();
    assert!(matches!(
        scan.feed(Bytes::from(vec![b'x'; BOUNDARY_LINE_LIMIT + 1])),
        Scan::TooLong
    ));
}
