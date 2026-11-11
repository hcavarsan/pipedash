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
mod tests;
