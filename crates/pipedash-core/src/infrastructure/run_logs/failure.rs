use std::sync::LazyLock;

use memchr::memmem;

const HEAD_BYTES: usize = 160;
const STAMP_TOKENS: usize = 4;

const UPPER_LEVELS: [&[u8]; 9] = [
    b"ERROR",
    b"ERRO",
    b"FATAL",
    b"CRITICAL",
    b"PANIC",
    b"SEVERE",
    b"FAIL",
    b"FAILED",
    b"FAILURE",
];
const LOWER_LEVELS: [&[u8]; 6] = [b"error", b"Error", b"fatal", b"Fatal", b"panic", b"Panic"];
const STRUCTURED_LEVELS: [&[u8]; 5] = [b"error", b"fatal", b"panic", b"critical", b"eror"];
const PREFIXES: [&[u8]; 10] = [
    b"npm ERR!",
    b"npm error",
    b"Traceback (most recent call last)",
    b"Exception in thread",
    b"Segmentation fault",
    b"make: ***",
    b"--- FAIL",
    b"test result: FAILED",
    b"BUILD FAIL",
    "❌".as_bytes(),
];
const EXIT_PHRASES: [&[u8]; 5] = [
    b"exit code ",
    b"exit status ",
    b"exited with code ",
    b"exited with status ",
    b"non-zero code: ",
];

static LEVEL_KEY: LazyLock<memmem::Finder<'static>> =
    LazyLock::new(|| memmem::Finder::new(b"level"));

pub(super) fn is_error(text: &str) -> bool {
    let bytes = text.as_bytes();
    let head = &bytes[..bytes.len().min(HEAD_BYTES)];
    leading_error(skip_stamps(head)) || structured_level(head) || failed_tail(bytes)
}

fn skip_stamps(mut rest: &[u8]) -> &[u8] {
    for _ in 0..STAMP_TOKENS {
        let text = rest.trim_ascii_start();
        let text = text.strip_prefix(b"[").unwrap_or(text);
        let end = text
            .iter()
            .position(|b| matches!(b, b' ' | b']'))
            .unwrap_or(text.len());
        let token = &text[..end];
        let is_stamp = token.first().is_some_and(u8::is_ascii_digit)
            && token.iter().any(|b| matches!(b, b'-' | b':' | b'/' | b'.'));
        if !is_stamp {
            break;
        }
        rest = text[end..].strip_prefix(b"]").unwrap_or(&text[end..]);
    }
    let rest = rest.trim_ascii_start();
    rest.strip_prefix(b"[").unwrap_or(rest)
}

fn leading_error(text: &[u8]) -> bool {
    if PREFIXES.iter().any(|prefix| text.starts_with(prefix)) {
        return true;
    }
    let name_len = text
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$'))
        .count();
    let name = &text[..name_len];
    let next = text.get(name_len).copied();
    if UPPER_LEVELS.contains(&name) {
        return matches!(next, None | Some(b':' | b' ' | b'\t' | b']' | b'!' | b'['));
    }
    if LOWER_LEVELS.contains(&name) {
        return matches!(next, Some(b':' | b'['));
    }
    next == Some(b':')
        && name.len() > 5
        && (name.ends_with(b"Error") || name.ends_with(b"Exception"))
}

fn structured_level(head: &[u8]) -> bool {
    LEVEL_KEY.find_iter(head).any(|at| {
        let rest = &head[at + 5..];
        let value = if at > 0 && head[at - 1] == b'"' {
            rest.strip_prefix(b"\":")
        } else if at == 0 || !(head[at - 1].is_ascii_alphanumeric() || head[at - 1] == b'_') {
            rest.strip_prefix(b"=")
        } else {
            None
        };
        let Some(value) = value else {
            return false;
        };
        let value = value.trim_ascii_start();
        let value = value.strip_prefix(b"\"").unwrap_or(value);
        STRUCTURED_LEVELS.iter().any(|level| {
            value.len() >= level.len()
                && value[..level.len()].eq_ignore_ascii_case(level)
                && !value
                    .get(level.len())
                    .is_some_and(u8::is_ascii_alphanumeric)
        })
    })
}

fn failed_tail(line: &[u8]) -> bool {
    let line = line.trim_ascii_end();
    if line.ends_with(b" ... FAILED")
        || (line.starts_with(b"exitcode '") && line.ends_with(b"': failure"))
    {
        return true;
    }
    let line = line.strip_suffix(b".").unwrap_or(line);
    let line = line.strip_suffix(b"'").unwrap_or(line);
    let digits = line.iter().rev().take_while(|b| b.is_ascii_digit()).count();
    let (head, code) = line.split_at(line.len() - digits);
    let head = head.strip_suffix(b"'").unwrap_or(head);
    code.iter().any(|b| *b != b'0') && EXIT_PHRASES.iter().any(|phrase| head.ends_with(phrase))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_lines_from_common_tools_are_errors() {
        let lines = [
            "ERROR: Job failed: exit code 1",
            "ERROR",
            "ERROR 1045 (28000): Access denied for user 'root'",
            "ERROR in ./src/index.ts",
            "FATAL: password authentication failed",
            "CRITICAL: out of memory",
            "PANIC: unexpected state",
            "FAIL src/login.test.ts",
            "FAIL\tgithub.com/acme/app\t0.123s",
            "FAILED tests/test_cart.py::test_total - assert 1 == 2",
            "FAILED: CMakeFiles/app.dir/main.cc.o",
            "FAILURE: Build failed with an exception.",
            "ERRO[0000] connection refused",
            "[ERROR] Failed to execute goal org.apache.maven.plugins:maven-compiler-plugin",
            "[ERROR]",
            "[2026-10-28 10:00:00] [ERROR] connection refused",
            "2026-10-28T10:00:00.123Z ERROR worker crashed",
            "2026-10-28 10:00:00,123 ERROR [main] boom",
            "Error: Cannot find module 'left-pad'",
            "error: could not compile `app` (lib) due to 1 previous error",
            "error[E0425]: cannot find value `x` in this scope",
            "fatal: not a git repository (or any of the parent directories): .git",
            "panic: runtime error: index out of range [3] with length 3",
            "npm ERR! code ELIFECYCLE",
            "npm error code E404",
            "Traceback (most recent call last):",
            "ValueError: invalid literal for int() with base 10: 'x'",
            "ModuleNotFoundError: No module named 'requests'",
            "java.lang.IllegalStateException: boom",
            "Exception in thread \"main\" java.lang.NullPointerException",
            "Segmentation fault (core dumped)",
            "make: *** [Makefile:12: build] Error 2",
            "--- FAIL: TestLogin (0.00s)",
            "test result: FAILED. 2 passed; 1 failed",
            "test login::rejects_bad_password ... FAILED",
            "BUILD FAILED in 12s",
            "BUILD FAILURE",
            "❌  Failure - Main npm test",
            "exitcode '1': failure",
            "Error: Process completed with exit code 1.",
            "script returned exit code 2",
            "Bash exited with code '1'.",
            "exit status 1",
            "The command '/bin/sh -c make' returned a non-zero code: 2",
            "container exited with code 137",
            "{\"level\":\"error\",\"msg\":\"tests failed\"}",
            "{\"ts\":1,\"level\": \"fatal\",\"msg\":\"x\"}",
            "time=\"2026-10-28T10:01:31Z\" level=fatal msg=\"exit status 1\"",
            "ts=2026-10-28 level=ERROR msg=boom",
        ];
        for line in lines {
            assert!(is_error(line), "{line:?}");
        }
    }

    #[test]
    fn ordinary_lines_are_not_errors() {
        let lines = [
            "",
            "0 errors",
            "0 FAILED",
            "1 error(s) expected in this fixture",
            "error_handler.rs",
            "src/errors.rs",
            "cargo test --fail-fast",
            "--fail-fast",
            "errors: none",
            "Errors: 0",
            "ERRORS: 0",
            "ERROR_COUNT=0",
            "FAILURES",
            "No errors found",
            "no error: all good",
            "test errors::parse_error ... ok",
            "PASS src/error.test.ts",
            "✓ handles error responses",
            "Compiling error-chain v0.12.0",
            "Error handling guide",
            "Finished in 0.5s (0 failures)",
            "Process exited with code 0",
            "exit status 0",
            "exit code 00",
            "exit code 2 is documented below",
            "log_level=error",
            "Setting log level: error",
            "level=info msg=\"retrying after error\"",
            "{\"level\":\"info\",\"msg\":\"error budget ok\"}",
            "{\"level\":\"errors\"}",
            "retrying after ERROR in 5s",
            "[INFO] BUILD SUCCESS",
            "Fatal is a word",
            "panicking is fun",
        ];
        for line in lines {
            assert!(!is_error(line), "{line:?}");
        }
    }

    #[test]
    fn only_the_head_and_tail_of_a_very_long_line_are_inspected() {
        let long = format!("{}ERROR: late", "x".repeat(10_000));
        assert!(!is_error(&long));
        let long = format!("ERROR: early{}", "x".repeat(10_000));
        assert!(is_error(&long));
        let long = format!("{} exit code 3", "x".repeat(10_000));
        assert!(is_error(&long));
    }
}
