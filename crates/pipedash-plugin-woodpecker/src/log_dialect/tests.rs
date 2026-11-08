use super::*;

#[test]
fn command_echo_lines_are_commands() {
    for line in ["▶  git init".as_bytes(), b"+ make test"] {
        let parsed = WoodpeckerLogDialect.parse(line);
        assert_eq!(parsed.marker, LineMarker::Command);
        assert_eq!(parsed.body, line);
    }
}

#[test]
fn other_lines_stay_plain() {
    for line in [&b"+not"[..], b"output", b""] {
        assert_eq!(WoodpeckerLogDialect.parse(line).marker, LineMarker::Plain);
    }
}
