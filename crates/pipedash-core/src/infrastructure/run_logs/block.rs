use std::ops::Range;

use super::ansi::Span;
use super::{
    BLOCK_MAX_LINES,
    BLOCK_MAX_RAW_BYTES,
    MAX_SPANS_PER_LINE,
};
use crate::domain::LineKind;

const HAS_TS: u8 = 0x80;
const RESERVED_BITS: u8 = 0x78;
const KIND_MASK: u8 = 0x07;
const LINE_OVERHEAD: usize = 8;
const SPAN_OVERHEAD: usize = 8;

pub(crate) const INDEX_RECORD_BYTES: usize = 16;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Corrupt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BlockRef {
    pub offset: u64,
    pub compressed_len: u32,
    pub first_line: u32,
}

impl BlockRef {
    pub(crate) fn to_bytes(self) -> [u8; INDEX_RECORD_BYTES] {
        let mut out = [0u8; INDEX_RECORD_BYTES];
        out[..8].copy_from_slice(&self.offset.to_le_bytes());
        out[8..12].copy_from_slice(&self.compressed_len.to_le_bytes());
        out[12..].copy_from_slice(&self.first_line.to_le_bytes());
        out
    }

    pub(crate) fn from_bytes(record: &[u8]) -> Option<Self> {
        let record: &[u8; INDEX_RECORD_BYTES] = record.try_into().ok()?;
        Some(Self {
            offset: u64::from_le_bytes(record[..8].try_into().ok()?),
            compressed_len: u32::from_le_bytes(record[8..12].try_into().ok()?),
            first_line: u32::from_le_bytes(record[12..].try_into().ok()?),
        })
    }
}

pub(crate) struct BlockWriter {
    meta: Vec<u8>,
    text: Vec<u8>,
    lines: usize,
    prev_ts: i64,
    max_lines: usize,
    target_bytes: usize,
}

impl BlockWriter {
    pub(crate) fn new(max_lines: usize, target_bytes: usize) -> Self {
        Self {
            meta: Vec::new(),
            text: Vec::new(),
            lines: 0,
            prev_ts: 0,
            max_lines: max_lines.clamp(1, BLOCK_MAX_LINES),
            target_bytes,
        }
    }

    pub(crate) fn lines(&self) -> usize {
        self.lines
    }

    pub(crate) fn would_overflow(&self, text_len: usize, span_count: usize) -> bool {
        self.lines >= 1
            && (self.lines >= self.max_lines
                || 4 + self.meta.len()
                    + self.text.len()
                    + text_len
                    + 1
                    + LINE_OVERHEAD
                    + span_count * SPAN_OVERHEAD
                    > self.target_bytes)
    }

    pub(crate) fn push(&mut self, kind: LineKind, ts: Option<i64>, text: &str, spans: &[Span]) {
        write_uleb(&mut self.meta, text.len() as u64);
        match ts {
            Some(ts) => {
                self.meta.push(kind as u8 | HAS_TS);
                let delta = ts.wrapping_sub(self.prev_ts);
                write_uleb(&mut self.meta, ((delta << 1) ^ (delta >> 63)) as u64);
                self.prev_ts = ts;
            }
            None => self.meta.push(kind as u8),
        }
        write_uleb(&mut self.meta, spans.len() as u64);
        for span in spans {
            write_uleb(&mut self.meta, u64::from(span.start));
            write_uleb(&mut self.meta, u64::from(span.len));
            write_uleb(&mut self.meta, u64::from(span.style));
        }
        self.text.extend_from_slice(text.as_bytes());
        self.text.push(b'\n');
        self.lines += 1;
    }

    pub(crate) fn finish(&mut self) -> Vec<u8> {
        let mut raw = Vec::with_capacity(4 + self.meta.len() + self.text.len());
        raw.extend_from_slice(&(self.lines as u32).to_le_bytes());
        raw.extend_from_slice(&self.meta);
        raw.extend_from_slice(&self.text);
        self.meta.clear();
        self.text.clear();
        self.lines = 0;
        self.prev_ts = 0;
        lz4_flex::block::compress_prepend_size(&raw)
    }
}

fn write_uleb(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8 & 0x7F) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

#[derive(Debug, Clone)]
pub(crate) struct LineMeta {
    pub text_start: u32,
    pub text_len: u32,
    pub kind: LineKind,
    pub ts: Option<i64>,
    pub spans: Range<u32>,
}

#[derive(Debug)]
pub(crate) struct DecodedBlock {
    pub lines: Vec<LineMeta>,
    pub spans: Vec<Span>,
    pub text: String,
}

impl DecodedBlock {
    pub(crate) fn line_text(&self, line: &LineMeta) -> &str {
        let start = line.text_start as usize;
        self.text
            .get(start..start + line.text_len as usize)
            .unwrap_or_default()
    }

    pub(crate) fn line_spans(&self, line: &LineMeta) -> &[Span] {
        self.spans
            .get(line.spans.start as usize..line.spans.end as usize)
            .unwrap_or_default()
    }
}

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn byte(&mut self) -> Result<u8, Corrupt> {
        let byte = *self.buf.get(self.pos).ok_or(Corrupt)?;
        self.pos += 1;
        Ok(byte)
    }

    fn u32_le(&mut self) -> Result<u32, Corrupt> {
        let end = self.pos.checked_add(4).ok_or(Corrupt)?;
        let bytes = self.buf.get(self.pos..end).ok_or(Corrupt)?;
        self.pos = end;
        Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| Corrupt)?))
    }

    fn uleb(&mut self, bits: u32) -> Result<u64, Corrupt> {
        let mut value = 0u64;
        let mut shift = 0u32;
        loop {
            if shift >= bits {
                return Err(Corrupt);
            }
            let byte = self.byte()?;
            let part = u64::from(byte & 0x7F);
            if shift + 7 > bits && part >> (bits - shift) != 0 {
                return Err(Corrupt);
            }
            value |= part << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
        }
    }

    fn uleb32(&mut self) -> Result<u32, Corrupt> {
        u32::try_from(self.uleb(32)?).map_err(|_| Corrupt)
    }
}

pub(crate) fn decode(compressed: &[u8], style_count: usize) -> Result<DecodedBlock, Corrupt> {
    let prefix: [u8; 4] = compressed
        .get(..4)
        .ok_or(Corrupt)?
        .try_into()
        .map_err(|_| Corrupt)?;
    if u32::from_le_bytes(prefix) as usize > BLOCK_MAX_RAW_BYTES {
        return Err(Corrupt);
    }
    let mut raw = lz4_flex::block::decompress_size_prepended(compressed).map_err(|_| Corrupt)?;

    let mut cursor = Cursor { buf: &raw, pos: 0 };
    let line_count = cursor.u32_le()? as usize;
    if !(1..=BLOCK_MAX_LINES).contains(&line_count) {
        return Err(Corrupt);
    }
    let mut lines = Vec::with_capacity(line_count);
    let mut spans: Vec<Span> = Vec::new();
    let mut text_cursor: u32 = 0;
    let mut prev_ts: i64 = 0;
    for _ in 0..line_count {
        let text_len = cursor.uleb32()?;
        let kind_byte = cursor.byte()?;
        if kind_byte & RESERVED_BITS != 0 {
            return Err(Corrupt);
        }
        let kind = LineKind::from_u8(kind_byte & KIND_MASK).ok_or(Corrupt)?;
        let ts = if kind_byte & HAS_TS != 0 {
            let zigzag = cursor.uleb(64)?;
            let delta = ((zigzag >> 1) as i64) ^ -((zigzag & 1) as i64);
            prev_ts = prev_ts.checked_add(delta).ok_or(Corrupt)?;
            Some(prev_ts)
        } else {
            None
        };
        let span_count = cursor.uleb32()? as usize;
        if span_count > MAX_SPANS_PER_LINE {
            return Err(Corrupt);
        }
        let span_from = u32::try_from(spans.len()).map_err(|_| Corrupt)?;
        let mut prev_end = 0u32;
        for _ in 0..span_count {
            let start = cursor.uleb32()?;
            let len = cursor.uleb32()?;
            let style = cursor.uleb32()?;
            let end = start.checked_add(len).ok_or(Corrupt)?;
            if len == 0 || start < prev_end || end > text_len || style as usize >= style_count {
                return Err(Corrupt);
            }
            prev_end = end;
            spans.push(Span {
                start,
                len,
                style: u16::try_from(style).map_err(|_| Corrupt)?,
            });
        }
        let span_to = u32::try_from(spans.len()).map_err(|_| Corrupt)?;
        lines.push(LineMeta {
            text_start: text_cursor,
            text_len,
            kind,
            ts,
            spans: span_from..span_to,
        });
        text_cursor = text_cursor
            .checked_add(text_len)
            .and_then(|v| v.checked_add(1))
            .ok_or(Corrupt)?;
    }

    let text_offset = cursor.pos;
    if raw.len() - text_offset != text_cursor as usize {
        return Err(Corrupt);
    }
    raw.drain(..text_offset);
    let text = String::from_utf8(raw).map_err(|_| Corrupt)?;
    let bytes = text.as_bytes();
    for line in &lines {
        let end = (line.text_start + line.text_len) as usize;
        if bytes.get(end) != Some(&b'\n') {
            return Err(Corrupt);
        }
        for span in &spans[line.spans.start as usize..line.spans.end as usize] {
            let from = (line.text_start + span.start) as usize;
            let to = from + span.len as usize;
            if !text.is_char_boundary(from) || !text.is_char_boundary(to) {
                return Err(Corrupt);
            }
        }
    }
    Ok(DecodedBlock { lines, spans, text })
}

#[cfg(test)]
mod tests {
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
}
