use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

const COMMAND_PREFIX: &[u8] = b"+ ";

pub struct BitbucketLogDialect;

impl LogDialect for BitbucketLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let is_command = line
            .strip_prefix(COMMAND_PREFIX)
            .is_some_and(|command| !command.trim_ascii().is_empty());
        ParsedLine {
            timestamp: None,
            marker: if is_command {
                LineMarker::Command
            } else {
                LineMarker::Plain
            },
            body: line,
        }
    }
}

#[cfg(test)]
mod tests;
