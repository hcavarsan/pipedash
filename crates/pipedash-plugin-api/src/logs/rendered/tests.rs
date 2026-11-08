use super::*;

#[test]
fn resumes_only_when_the_old_output_is_an_unchanged_prefix() {
    let cursor = RenderedCursor::of(b". a\n. b\n");

    assert_eq!(cursor.resume(b". a\n. b\n! c\n"), Some(8));
    assert_eq!(cursor.resume(b". a\n. b\n"), Some(8));
    assert_eq!(cursor.resume(b". a\n. x\n! c\n"), None);
    assert_eq!(cursor.resume(b". a\n"), None);
}
