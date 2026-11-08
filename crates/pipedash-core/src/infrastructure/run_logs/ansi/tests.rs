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
fn run_logs_ansi_drops_concealed_notes() {
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
    let (text, spans, _) = run(b"done.\x1b[K\x1b_bk;t=1745322210213\x07 next\x1b_bk;t=1\x1b\\ end");
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
