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
mod tests {
    use super::*;

    fn parse(line: &[u8]) -> ParsedLine<'_> {
        BitbucketLogDialect.parse(line)
    }

    #[test]
    fn script_commands_are_commands_and_keep_their_text() {
        let lines: [&[u8]; 5] = [
            b"+ umask 000",
            b"+ git reset --hard 4c684f2c6785329c62e18489ded3dbaf3f76ac0f",
            b"+ GIT_LFS_SKIP_SMUDGE=1 retry 6 git clone --branch=\"main\" --depth 50 $BUILD_DIR",
            b"+ echo \".bitbucket/pipelines/generated\" >> .git/info/exclude",
            b"+ docker container run \\",
        ];
        for line in lines {
            let parsed = parse(line);
            assert_eq!(parsed.marker, LineMarker::Command, "{line:?}");
            assert_eq!(parsed.body, line);
            assert_eq!(parsed.timestamp, None);
        }
    }

    #[test]
    fn output_and_continuation_lines_stay_plain() {
        let lines: [&[u8]; 11] = [
            b"Cloning into '/opt/atlassian/pipelines/agent/build'...",
            b"HEAD is now at 4c684f2 fix syntax error",
            b"   --volume=/opt/atlassian/pipelines/agent/build:/opt/atlassian/pipelines/agent/build \\",
            b"Default variables:",
            b"    build : docker.io/atlassian/default-image@sha256:689e2c63",
            b"+",
            b"+ ",
            b"+   ",
            b"++ nested trace",
            b"+added line",
            b" + indented",
        ];
        for line in lines {
            let parsed = parse(line);
            assert_eq!(parsed.marker, LineMarker::Plain, "{line:?}");
            assert_eq!(parsed.body, line);
            assert_eq!(parsed.timestamp, None);
        }
    }

    #[test]
    fn colored_tool_output_is_not_a_command() {
        let parsed = parse(b"\x1b[32m+ added\x1b[0m");
        assert_eq!(parsed.marker, LineMarker::Plain);
    }
}
