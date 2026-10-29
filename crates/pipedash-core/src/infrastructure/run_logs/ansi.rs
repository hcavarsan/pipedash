use std::collections::HashMap;

use super::MAX_SPANS_PER_LINE;
use crate::domain::{
    AnsiColor,
    LogStyle,
};

const MAX_STYLES: usize = 65_535;
const MAX_SGR_PARAMS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub start: u32,
    pub len: u32,
    pub style: u16,
}

#[derive(Default)]
pub(crate) struct StyleTable {
    map: HashMap<LogStyle, u16>,
    list: Vec<LogStyle>,
}

impl StyleTable {
    pub(crate) fn from_list(list: Vec<LogStyle>) -> Self {
        let map = list.iter().cloned().zip(0u16..).collect();
        Self { map, list }
    }

    pub(crate) fn len(&self) -> usize {
        self.list.len()
    }

    pub(crate) fn into_list(self) -> Vec<LogStyle> {
        self.list
    }

    fn intern(&mut self, style: &LogStyle) -> Option<u16> {
        if let Some(&index) = self.map.get(style) {
            return Some(index);
        }
        if self.list.len() >= MAX_STYLES {
            return None;
        }
        let index = u16::try_from(self.list.len()).ok()?;
        self.map.insert(style.clone(), index);
        self.list.push(style.clone());
        Some(index)
    }
}

#[derive(Clone, Copy, Default)]
struct Sgr {
    fg: Option<AnsiColor>,
    bg: Option<AnsiColor>,
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    inverse: bool,
    conceal: bool,
}

impl Sgr {
    fn style(&self) -> LogStyle {
        let (fg, bg) = if self.inverse {
            (self.bg, self.fg)
        } else {
            (self.fg, self.bg)
        };
        LogStyle {
            fg,
            bg,
            bold: self.bold,
            dim: self.dim,
            italic: self.italic,
            underline: self.underline,
            strike: self.strike,
        }
    }
}

pub(crate) fn render(
    body: &[u8], table: &mut StyleTable, text: &mut String, spans: &mut Vec<Span>,
) {
    let mut sgr = Sgr::default();
    let mut current: Option<u16> = None;
    let mut i = 0;
    while i < body.len() {
        let run_end = body[i..]
            .iter()
            .position(|&b| b < 0x20 && b != b'\t')
            .map_or(body.len(), |p| i + p);
        if run_end > i {
            if !sgr.conceal {
                push_run(&body[i..run_end], current, text, spans);
            }
            i = run_end;
            continue;
        }
        if body[i] != 0x1B {
            i += 1;
            continue;
        }
        match body.get(i + 1) {
            Some(b'[') => {
                let (next, params) = csi(body, i + 2);
                i = next;
                if let Some(params) = params {
                    apply_sgr(&mut sgr, params);
                    let style = sgr.style();
                    current = if style == LogStyle::default() {
                        None
                    } else {
                        table.intern(&style)
                    };
                }
            }
            Some(b']' | b'_') => i = osc(body, i + 2),
            _ => i = (i + 2).min(body.len()),
        }
    }
}

fn push_run(run: &[u8], style: Option<u16>, text: &mut String, spans: &mut Vec<Span>) {
    let start = text.len();
    text.push_str(&String::from_utf8_lossy(run));
    let (Some(style), true) = (style, text.len() > start) else {
        return;
    };
    let len = (text.len() - start) as u32;
    let start = start as u32;
    if let Some(last) = spans.last_mut() {
        if last.style == style && last.start + last.len == start {
            last.len += len;
            return;
        }
    }
    if spans.len() < MAX_SPANS_PER_LINE {
        spans.push(Span { start, len, style });
    }
}

fn csi(body: &[u8], start: usize) -> (usize, Option<&[u8]>) {
    let mut j = start;
    while j < body.len() && (0x30..=0x3F).contains(&body[j]) {
        j += 1;
    }
    let params_end = j;
    while j < body.len() && (0x20..=0x2F).contains(&body[j]) {
        j += 1;
    }
    if j < body.len() && (0x40..=0x7E).contains(&body[j]) {
        let sgr = body[j] == b'm' && params_end == j;
        let params = &body[start..params_end];
        let plain = !params.iter().any(|b| (b'<'..=b'?').contains(b));
        (j + 1, (sgr && plain).then_some(params))
    } else {
        (j, None)
    }
}

fn osc(body: &[u8], start: usize) -> usize {
    let mut j = start;
    while j < body.len() {
        match body[j] {
            0x07 => return j + 1,
            0x1B if body.get(j + 1) == Some(&b'\\') => return j + 2,
            _ => j += 1,
        }
    }
    body.len()
}

fn apply_sgr(sgr: &mut Sgr, params: &[u8]) {
    let mut codes = [0u32; MAX_SGR_PARAMS];
    let mut count = 0;
    let mut current: u32 = 0;
    for &b in params {
        if b.is_ascii_digit() {
            current = current
                .saturating_mul(10)
                .saturating_add(u32::from(b - b'0'));
        } else {
            if count < MAX_SGR_PARAMS {
                codes[count] = current;
                count += 1;
            }
            current = 0;
        }
    }
    if count < MAX_SGR_PARAMS {
        codes[count] = current;
        count += 1;
    }

    let mut i = 0;
    while i < count {
        let code = codes[i];
        match code {
            0 => *sgr = Sgr::default(),
            1 => sgr.bold = true,
            2 => sgr.dim = true,
            3 => sgr.italic = true,
            4 => sgr.underline = true,
            9 => sgr.strike = true,
            7 => sgr.inverse = true,
            8 => sgr.conceal = true,
            28 => sgr.conceal = false,
            22 => {
                sgr.bold = false;
                sgr.dim = false;
            }
            23 => sgr.italic = false,
            24 => sgr.underline = false,
            29 => sgr.strike = false,
            27 => sgr.inverse = false,
            30..=37 => sgr.fg = Some(palette(code - 30)),
            90..=97 => sgr.fg = Some(palette(code - 90)),
            39 => sgr.fg = None,
            40..=47 => sgr.bg = Some(palette(code - 40)),
            100..=107 => sgr.bg = Some(palette(code - 100)),
            49 => sgr.bg = None,
            38 | 48 => {
                let (color, used) = extended(&codes[i + 1..count]);
                if let Some(color) = color {
                    if code == 38 {
                        sgr.fg = Some(color);
                    } else {
                        sgr.bg = Some(color);
                    }
                }
                i += used;
            }
            _ => {}
        }
        i += 1;
    }
}

fn extended(rest: &[u32]) -> (Option<AnsiColor>, usize) {
    match rest.first() {
        Some(5) => {
            let color = rest
                .get(1)
                .and_then(|&n| u8::try_from(n).ok())
                .map(from_256);
            (color, rest.len().min(2))
        }
        Some(2) => {
            let channel = |index: usize| rest.get(index).and_then(|&n| u8::try_from(n).ok());
            let color = match (channel(1), channel(2), channel(3)) {
                (Some(r), Some(g), Some(b)) => Some(from_rgb(r, g, b)),
                _ => None,
            };
            (color, rest.len().min(4))
        }
        _ => (None, 0),
    }
}

fn palette(index: u32) -> AnsiColor {
    match index {
        0 => AnsiColor::Black,
        1 => AnsiColor::Red,
        2 => AnsiColor::Green,
        3 => AnsiColor::Yellow,
        4 => AnsiColor::Blue,
        5 => AnsiColor::Magenta,
        6 => AnsiColor::Cyan,
        _ => AnsiColor::White,
    }
}

fn from_256(n: u8) -> AnsiColor {
    match n {
        0..=7 => palette(u32::from(n)),
        8..=15 => palette(u32::from(n - 8)),
        16..=231 => {
            let cube = n - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            from_rgb(level(cube / 36), level(cube / 6 % 6), level(cube % 6))
        }
        232..=243 => AnsiColor::Black,
        244..=255 => AnsiColor::White,
    }
}

fn from_rgb(r: u8, g: u8, b: u8) -> AnsiColor {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max - min < 40 {
        return if max < 128 {
            AnsiColor::Black
        } else {
            AnsiColor::White
        };
    }
    let (rf, gf, bf) = (f32::from(r), f32::from(g), f32::from(b));
    let delta = f32::from(max - min);
    let hue = if max == r {
        ((gf - bf) / delta).rem_euclid(6.0) * 60.0
    } else if max == g {
        ((bf - rf) / delta + 2.0) * 60.0
    } else {
        ((rf - gf) / delta + 4.0) * 60.0
    }
    .rem_euclid(360.0);
    match hue {
        h if !(20.0..330.0).contains(&h) => AnsiColor::Red,
        h if h < 70.0 => AnsiColor::Yellow,
        h if h < 170.0 => AnsiColor::Green,
        h if h < 200.0 => AnsiColor::Cyan,
        h if h < 260.0 => AnsiColor::Blue,
        _ => AnsiColor::Magenta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(body: &[u8]) -> (String, Vec<Span>, Vec<LogStyle>) {
        let mut table = StyleTable::default();
        let mut text = String::new();
        let mut spans = Vec::new();
        render(body, &mut table, &mut text, &mut spans);
        (text, spans, table.into_list())
    }

    fn color_of(body: &[u8]) -> Option<AnsiColor> {
        run(body).2.first().and_then(|s| s.fg)
    }

    #[test]
    fn run_logs_ansi_red_span() {
        let (text, spans, styles) = run(b"\x1b[31mred\x1b[0m ok");
        assert_eq!(text, "red ok");
        assert_eq!(
            spans,
            vec![Span {
                start: 0,
                len: 3,
                style: 0
            }]
        );
        assert_eq!(styles[0].fg, Some(AnsiColor::Red));
    }

    #[test]
    fn run_logs_ansi_extended_colors_quantize() {
        assert_eq!(color_of(b"\x1b[38;5;196mx"), Some(AnsiColor::Red));
        assert_eq!(color_of(b"\x1b[38;2;0;200;0mx"), Some(AnsiColor::Green));
        assert_eq!(color_of(b"\x1b[38;5;232mx"), Some(AnsiColor::Black));
        assert_eq!(color_of(b"\x1b[38;5;250mx"), Some(AnsiColor::White));
        assert_eq!(color_of(b"\x1b[38;2;100;100;100mx"), Some(AnsiColor::Black));
        assert_eq!(color_of(b"\x1b[92mx"), Some(AnsiColor::Green));
        let bg = run(b"\x1b[48;5;21mx").2;
        assert_eq!(bg[0].bg, Some(AnsiColor::Blue));
    }

    #[test]
    fn run_logs_ansi_inverse_swaps_colors() {
        let styles = run(b"\x1b[31;42;7mx").2;
        assert_eq!(styles[0].fg, Some(AnsiColor::Green));
        assert_eq!(styles[0].bg, Some(AnsiColor::Red));
    }

    #[test]
    fn run_logs_ansi_drops_osc_and_controls() {
        let (text, spans, _) = run(b"a\x1b]0;title\x07b\x1b]8;;x\x1b\\c\x08\x07d\x1b[2Ke\x1bXf\tg");
        assert_eq!(text, "abcdef\tg");
        assert!(spans.is_empty());
    }

    #[test]
    fn run_logs_ansi_drops_concealed_jenkins_notes() {
        let note = "\x1b[8mha:////4IkPeNOtKgYQ==\x1b[0m";
        assert_eq!(
            run(format!("{note}[Pipeline] Start").as_bytes()).0,
            "[Pipeline] Start"
        );
        assert_eq!(
            run(format!("Started by user {note}admin").as_bytes()).0,
            "Started by user admin"
        );
        assert_eq!(run(format!("{note}a{note}b").as_bytes()).0, "ab");
    }

    #[test]
    fn run_logs_ansi_conceal_ends_at_28_or_reset_or_line_end() {
        assert_eq!(run(b"a\x1b[8msecret\x1b[28mb").0, "ab");
        assert_eq!(run(b"a\x1b[8msecret\x1b[0mb").0, "ab");
        assert_eq!(run(b"a\x1b[8msecret\x1b[mb").0, "ab");
        assert_eq!(run(b"a\x1b[8msecret to the end").0, "a");
        assert_eq!(run(b"next line").0, "next line");
    }

    #[test]
    fn run_logs_ansi_concealed_text_keeps_other_styles() {
        let (text, spans, styles) = run(b"\x1b[31m\x1b[8mhidden\x1b[28mshown");
        assert_eq!(text, "shown");
        assert_eq!(spans.len(), 1);
        assert_eq!(styles[spans[0].style as usize].fg, Some(AnsiColor::Red));
    }

    #[test]
    fn run_logs_ansi_drops_apc_strings() {
        let (text, spans, _) =
            run(b"done.\x1b[K\x1b_bk;t=1745322210213\x07 next\x1b_bk;t=1\x1b\\ end");
        assert_eq!(text, "done. next end");
        assert!(spans.is_empty());
    }

    #[test]
    fn run_logs_ansi_invalid_utf8_offsets() {
        let (text, spans, _) = run(b"\x1b[1mab\xFFcd\x1b[0mef");
        assert_eq!(text, "ab\u{FFFD}cdef");
        assert_eq!(
            spans,
            vec![Span {
                start: 0,
                len: 7,
                style: 0
            }]
        );
        assert!(text.is_char_boundary(2) && text.is_char_boundary(5));
    }

    #[test]
    fn run_logs_ansi_merges_adjacent_equal_styles() {
        let (_, spans, _) = run(b"\x1b[31ma\x1b[0m\x1b[31mb\x1b[0mc\x1b[31md");
        assert_eq!(
            spans,
            vec![
                Span {
                    start: 0,
                    len: 2,
                    style: 0
                },
                Span {
                    start: 3,
                    len: 1,
                    style: 0
                }
            ]
        );
    }

    #[test]
    fn run_logs_ansi_caps_spans_per_line() {
        let mut body = Vec::new();
        for i in 0..5000 {
            body.extend_from_slice(if i % 2 == 0 {
                b"\x1b[31ma"
            } else {
                b"\x1b[32mb"
            });
        }
        let (text, spans, _) = run(&body);
        assert_eq!(text.len(), 5000);
        assert_eq!(spans.len(), MAX_SPANS_PER_LINE);
    }

    #[test]
    fn run_logs_ansi_unterminated_sequences_are_dropped() {
        assert_eq!(run(b"ab\x1b[31").0, "ab");
        assert_eq!(run(b"ab\x1b").0, "ab");
        assert_eq!(run(b"ab\x1b]0;never ends").0, "ab");
    }
}
