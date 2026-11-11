use pipedash_plugin_actions::ActionsLogDialect;
use pipedash_plugin_api::{
    LineMarker,
    LogDialect,
    ParsedLine,
};

pub struct GiteaLogDialect;

impl LogDialect for GiteaLogDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        let parsed = ActionsLogDialect.parse(line);
        if parsed.marker != LineMarker::Plain {
            return parsed;
        }
        match workflow_command(parsed.body) {
            Some((marker, body)) => ParsedLine {
                marker,
                body,
                ..parsed
            },
            None => parsed,
        }
    }
}

fn workflow_command(body: &[u8]) -> Option<(LineMarker, &[u8])> {
    let rest = body.strip_prefix(b"::")?;
    let end = rest.windows(2).position(|pair| pair == b"::")?;
    let name = rest[..end].split(|b| *b == b' ').next()?;
    let marker = match name {
        b"group" => LineMarker::GroupStart,
        b"endgroup" => LineMarker::GroupEnd,
        b"error" => LineMarker::Error,
        b"warning" => LineMarker::Warning,
        b"notice" => LineMarker::Notice,
        b"debug" => LineMarker::Debug,
        _ => return None,
    };
    Some((marker, &rest[end + 2..]))
}

#[cfg(test)]
mod tests;
