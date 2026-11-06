use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

const COMMAND_PREFIXES: [&[u8]; 2] = ["▶  ".as_bytes(), b"+ "];

pub struct WoodpeckerLogDialect;

impl LogDialect for WoodpeckerLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let marker = if COMMAND_PREFIXES
            .iter()
            .any(|prefix| line.starts_with(prefix))
        {
            LineMarker::Command
        } else {
            LineMarker::Plain
        };
        ParsedLine {
            timestamp: None,
            marker,
            body: line,
        }
    }
}

#[cfg(test)]
mod tests {
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
}
