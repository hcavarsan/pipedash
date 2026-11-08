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
        "FAIL\texample.test/acme/app\t0.123s",
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
