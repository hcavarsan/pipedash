use super::*;

#[test]
fn br_and_closing_p_become_newlines() {
    assert_eq!(
        to_plain("one<br />two<BR>three <p>four</p>five", 500),
        "one\ntwo\nthree four\nfive"
    );
}

#[test]
fn tags_are_stripped_before_entities_are_decoded() {
    assert_eq!(to_plain("&lt;b&gt;bold&lt;/b&gt;", 500), "<b>bold</b>");
    assert_eq!(to_plain("<b>x</b>&amp;lt;", 500), "x&lt;");
    assert_eq!(to_plain("a &lt;br&gt; b", 500), "a <br> b");
}

#[test]
fn named_and_numeric_entities_decode() {
    assert_eq!(
        to_plain(
            "Tom &amp; Jerry &quot;x&quot; &#39;y&#39; &#x41;&#66; a&nbsp;b &gt; &lt;",
            500
        ),
        "Tom & Jerry \"x\" 'y' AB a b > <"
    );
}

#[test]
fn invalid_numeric_entities_become_replacement_character() {
    assert_eq!(to_plain("a&#xD800;b", 500), "a\u{FFFD}b");
    assert_eq!(to_plain("a&#1114112;b", 500), "a\u{FFFD}b");
    assert_eq!(to_plain("a&#0;b", 500), "a\u{FFFD}b");
    assert_eq!(to_plain("a&#99999999999;b", 500), "a\u{FFFD}b");
}

#[test]
fn unknown_entities_and_stray_markers_stay_literal() {
    assert_eq!(
        to_plain("a &foo; b & c &#; &#x; 1 < 2", 500),
        "a &foo; b & c &#; &#x; 1 < 2"
    );
    assert_eq!(to_plain("broken <a href", 500), "broken <a href");
}

#[test]
fn control_and_bidi_characters_are_removed() {
    assert_eq!(
        to_plain("a\u{0}b\u{7}c\u{85}d\u{9F}e\u{7F}f", 500),
        "abcdef"
    );
    assert_eq!(
        to_plain("a\u{202E}b\u{2066}c\u{2069}d\u{202A}e", 500),
        "abcde"
    );
    assert_eq!(to_plain("x&#7;y&#x202E;z", 500), "xyz");
}

#[test]
fn whitespace_collapses_and_newlines_survive() {
    assert_eq!(to_plain("  a \t\r  b  \n\n  \n c  ", 500), "a b\nc");
}

#[test]
fn output_is_capped_by_characters() {
    assert_eq!(to_plain("ééééé", 3), "ééé");
    assert_eq!(to_plain("ab cd", 3), "ab");
    assert_eq!(
        to_plain(&"x".repeat(900), UPDATE_MAX_CHARS).chars().count(),
        500
    );
    assert_eq!(
        to_plain(&"y".repeat(900), NAME_MAX_CHARS).chars().count(),
        200
    );
}

#[test]
fn pathological_input_stays_linear() {
    let text = "<a".repeat(200_000) + &"&".repeat(200_000);

    assert_eq!(to_plain(&text, 500).chars().count(), 500);
}
