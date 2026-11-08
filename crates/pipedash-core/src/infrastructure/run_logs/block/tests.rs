use super::*;

fn leb(value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    write_uleb(&mut out, value);
    out
}

fn pack(meta: &[u8], text: &[u8], lines: u32) -> Vec<u8> {
    let mut raw = lines.to_le_bytes().to_vec();
    raw.extend_from_slice(meta);
    raw.extend_from_slice(text);
    lz4_flex::block::compress_prepend_size(&raw)
}

fn one_line(text_len: u64, kind: u8, spans: &[(u64, u64, u64)], text: &[u8]) -> Vec<u8> {
    let mut meta = leb(text_len);
    meta.push(kind);
    meta.extend(leb(spans.len() as u64));
    for &(start, len, style) in spans {
        meta.extend(leb(start));
        meta.extend(leb(len));
        meta.extend(leb(style));
    }
    pack(&meta, text, 1)
}

#[test]
fn run_logs_block_round_trips_1024_lines() {
    let mut writer = BlockWriter::new(BLOCK_MAX_LINES, usize::MAX);
    let kinds = [LineKind::Plain, LineKind::Error, LineKind::Command];
    for i in 0..1024usize {
        let text = format!("line {i} é");
        let spans = [Span {
            start: 0,
            len: 4,
            style: 1,
        }];
        let ts = (i % 3 != 0).then_some(1_700_000_000_000 + (i as i64) * 7 - (i as i64 % 5));
        writer.push(
            kinds[i % 3],
            ts,
            &text,
            if i % 2 == 0 { &spans } else { &[] },
        );
    }
    assert_eq!(writer.lines(), 1024);
    assert!(writer.would_overflow(1, 0));
    let block = decode(&writer.finish(), 2).expect("round trip");
    assert_eq!(writer.lines(), 0);
    assert_eq!(block.lines.len(), 1024);
    for (i, line) in block.lines.iter().enumerate() {
        assert_eq!(block.line_text(line), format!("line {i} é"));
        assert_eq!(line.kind, kinds[i % 3]);
        let want = (i % 3 != 0).then_some(1_700_000_000_000 + (i as i64) * 7 - (i as i64 % 5));
        assert_eq!(line.ts, want);
        assert_eq!(block.line_spans(line).len(), usize::from(i % 2 == 0));
    }
}

#[test]
fn run_logs_block_rejects_bad_prefix_and_framing() {
    assert_eq!(decode(&[], 1).unwrap_err(), Corrupt);
    assert_eq!(decode(&[1, 2, 3], 1).unwrap_err(), Corrupt);
    assert_eq!(decode(&[0xFF; 8], 1).unwrap_err(), Corrupt);
    assert_eq!(decode(&[0, 0, 0, 0, 0xFF, 0xFF], 1).unwrap_err(), Corrupt);
    assert_eq!(decode(&pack(&[], &[], 0), 1).unwrap_err(), Corrupt);
    assert_eq!(decode(&pack(&[], &[], 1025), 1).unwrap_err(), Corrupt);
}

#[test]
fn run_logs_block_rejects_bad_leb128() {
    let truncated = pack(&[0x80, 0x80], &[], 1);
    assert_eq!(decode(&truncated, 1).unwrap_err(), Corrupt);
    let too_long = pack(&[0xFF; 6], &[], 1);
    assert_eq!(decode(&too_long, 1).unwrap_err(), Corrupt);
    let overflow = pack(&[0xFF, 0xFF, 0xFF, 0xFF, 0x1F], &[], 1);
    assert_eq!(decode(&overflow, 1).unwrap_err(), Corrupt);
    let mut ts_overflow = vec![0u8, HAS_TS];
    ts_overflow.extend_from_slice(&[0xFF; 9]);
    ts_overflow.push(0x7F);
    assert_eq!(decode(&pack(&ts_overflow, &[], 1), 1).unwrap_err(), Corrupt);
}

#[test]
fn run_logs_block_rejects_bad_kind_and_reserved_bits() {
    assert_eq!(
        decode(&one_line(1, 7, &[], b"a\n"), 1).unwrap_err(),
        Corrupt
    );
    assert_eq!(
        decode(&one_line(1, 0x08, &[], b"a\n"), 1).unwrap_err(),
        Corrupt
    );
    assert!(decode(&one_line(1, 2, &[], b"a\n"), 1).is_ok());
}

#[test]
fn run_logs_block_rejects_bad_spans() {
    let ok = one_line(4, 0, &[(0, 2, 0), (2, 2, 0)], b"abcd\n");
    assert!(decode(&ok, 1).is_ok());
    let overlap = one_line(4, 0, &[(0, 3, 0), (2, 1, 0)], b"abcd\n");
    assert_eq!(decode(&overlap, 1).unwrap_err(), Corrupt);
    let empty = one_line(4, 0, &[(0, 0, 0)], b"abcd\n");
    assert_eq!(decode(&empty, 1).unwrap_err(), Corrupt);
    let outside = one_line(4, 0, &[(3, 2, 0)], b"abcd\n");
    assert_eq!(decode(&outside, 1).unwrap_err(), Corrupt);
    let bad_style = one_line(4, 0, &[(0, 1, 1)], b"abcd\n");
    assert_eq!(decode(&bad_style, 1).unwrap_err(), Corrupt);
    let splits_char = one_line(3, 0, &[(0, 1, 0)], "\u{e9}a\n".as_bytes());
    assert_eq!(decode(&splits_char, 1).unwrap_err(), Corrupt);
}

#[test]
fn run_logs_block_rejects_bad_text() {
    assert_eq!(
        decode(&one_line(2, 0, &[], b"\xFF\xFE\n"), 1).unwrap_err(),
        Corrupt
    );
    assert_eq!(decode(&one_line(1, 0, &[], b"ab"), 1).unwrap_err(), Corrupt);
    assert_eq!(
        decode(&one_line(1, 0, &[], b"a\nzz"), 1).unwrap_err(),
        Corrupt
    );
    assert_eq!(
        decode(&one_line(5, 0, &[], b"a\n"), 1).unwrap_err(),
        Corrupt
    );
}

#[test]
fn run_logs_block_writer_splits_by_lines_and_bytes() {
    let mut writer = BlockWriter::new(4, 1 << 20);
    assert!(!writer.would_overflow(10, 0));
    for _ in 0..4 {
        writer.push(LineKind::Plain, None, "x", &[]);
    }
    assert!(writer.would_overflow(1, 0));
    let mut writer = BlockWriter::new(1024, 64);
    writer.push(LineKind::Plain, None, "x", &[]);
    assert!(!writer.would_overflow(1, 0));
    assert!(writer.would_overflow(100, 0));
}

#[test]
fn run_logs_block_index_record_round_trips() {
    let block_ref = BlockRef {
        offset: 1 << 40,
        compressed_len: 77,
        first_line: 4096,
    };
    assert_eq!(BlockRef::from_bytes(&block_ref.to_bytes()), Some(block_ref));
    assert_eq!(BlockRef::from_bytes(&[0; 15]), None);
}
