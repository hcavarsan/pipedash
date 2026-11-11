#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::sync::atomic::{
    AtomicBool,
    AtomicU64,
};
use std::sync::Arc;

use bytes::Bytes;
use pipedash_core::infrastructure::run_logs::{
    BlockLimits,
    DocKey,
    Document,
    Ingester,
};
use pipedash_plugin_api::{
    LogDialect,
    PlainDialect,
};
use tokio::sync::mpsc;

static PLAIN: PlainDialect = PlainDialect;

fn dialect(provider_type: &str) -> &'static dyn LogDialect {
    pipedash_plugins::registry()
        .create(provider_type)
        .unwrap()
        .log_dialect()
}

fn doc_key() -> DocKey {
    DocKey {
        provider_id: 1,
        pipeline_id: "stub__1__owner/repo/ci.yml".into(),
        run_number: 7,
        node_id: "100".into(),
    }
}

fn ingester(dir: &Path, dialect: &'static dyn LogDialect) -> Ingester {
    Ingester {
        key: doc_key(),
        revision: 1,
        complete: true,
        dir: dir.to_path_buf(),
        dialect,
        steps: Vec::new(),
        received: Arc::new(AtomicU64::new(0)),
        limits: BlockLimits::default(),
        base: None,
        restart: Arc::new(AtomicBool::new(false)),
    }
}

const BUILDKITE_LOG: &str = concat!(
    "\x1b_bk;t=1745322209921\x07~~~ Running global environment hook\r\n",
    "\x1b_bk;t=1745322209921\x07\x1b[90m$\x1b[0m /buildkite/agent/hooks/environment\r\n",
    "\x1b_bk;t=1745322210692\x07~~~ Running script\r\n",
    "\x1b_bk;t=1745322210692\x07\x1b[90m$\x1b[0m ./script.sh\r\n",
    "\x1b_bk;t=1745322210694\x07--- :package: Build\r\r\n",
    "\x1b_bk;t=1745322210698\x07remote: Counting objects:  50%\x1b[K",
    "\x1b_bk;t=1745322210699\x07\rremote: Counting objects: 100%, done.\x1b[K",
    "\x1b_bk;t=1745322210700\x07\r\r\n",
    "\x1b_bk;t=1745322210800\x07+++ Running tests\r\n",
    "\x1b_bk;t=1745322210900\x07ok\r\n",
    "\x1b_bk;t=1745322211000\x07--- Lint\r\n",
    "\x1b_bk;t=1745322212000\x07\x1b[31m🚨 Error: The command exited with status 1\x1b[0m\r\n",
    "\x1b_bk;t=1745322212000\x07^^^ +++\r\n",
    "\x1b_bk;t=1745322212100\x07--- Upload\r\n",
    "\x1b_bk;t=1745322212200\x07^^^ +++ Upload failed\r\n",
);

#[test]
fn buildkite_groups_become_flat_sections() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ingester = ingester(dir.path(), dialect("buildkite"));
    let (tx, mut rx) = mpsc::channel(1);
    tx.try_send(Bytes::from_static(BUILDKITE_LOG.as_bytes()))
        .expect("channel has room");
    drop(tx);

    let meta = ingester
        .run(&mut rx)
        .expect("ingest")
        .finish(None)
        .expect("finish")
        .meta;

    let sections: Vec<_> = meta
        .sections
        .iter()
        .map(|s| {
            (
                s.title.as_str(),
                s.parent,
                s.header_line,
                s.end_line,
                s.open,
            )
        })
        .collect();
    assert_eq!(
        sections,
        vec![
            ("Running global environment hook", None, Some(0), 2, false),
            ("Running script", None, Some(2), 4, false),
            (":package: Build", None, Some(4), 6, false),
            ("Running tests", None, Some(6), 8, true),
            ("Lint", None, Some(8), 10, true),
            ("Upload failed", None, Some(10), 11, true),
        ]
    );
    assert_eq!(meta.sections[3].duration_ms, Some(100));
    assert_eq!(meta.line_count, 11);
    assert_eq!(meta.error_lines, vec![9]);
    assert_eq!(meta.focus_line, Some(9));
    assert_eq!(meta.first_timestamp_ms, Some(1_745_322_209_921));

    let text = Document::open(dir.path())
        .expect("open")
        .text_block(0)
        .expect("block");
    let text = std::str::from_utf8(&text).expect("utf8");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 11);
    assert_eq!(
        lines[5],
        "2025-04-22T11:43:30.698Z remote: Counting objects: 100%, done."
    );
    assert!(lines[9].ends_with(" 🚨 Error: The command exited with status 1"));
    assert!(!text.contains("bk;t=") && !text.contains("^^^"));
}

const GITLAB_LOG: &str = concat!(
    "Running with gitlab-runner 18.4.0 (139a0ac0)\n",
    "  on code-europa-eu-runner fkXhbgRUT, system ID: s_5247efa18f3c\n",
    "section_start:1790320399:prepare_executor\r\x1b[0K\x1b[0K\x1b[36;1mPreparing the \"docker+machine\" executor\x1b[0;m\x1b[0;m\n",
    "\x1b[0KUsing Docker executor with image python:3.11 ...\x1b[0;m\n",
    "section_end:1790320401:prepare_executor\r\x1b[0Ksection_start:1790320401:prepare_script\r\x1b[0K\x1b[0K\x1b[36;1mPreparing environment\x1b[0;m\x1b[0;m\n",
    "Running on runner-fkxhbgrut-project-1344-concurrent-0\n",
    "section_end:1790320402:prepare_script\r\x1b[0Ksection_start:1790320402:get_sources[collapsed=true]\r\x1b[0K\x1b[0K\x1b[36;1mGetting source from Git repository\x1b[0;m\x1b[0;m\n",
    "\x1b[32;1mFetching changes with git depth set to 20...\x1b[0;m\n",
    "section_end:1790320403:get_sources\r\x1b[0Ksection_start:1790320403:step_script\r\x1b[0K\x1b[0K\x1b[36;1mExecuting \"step_script\" stage of the job script\x1b[0;m\x1b[0;m\n",
    "\x1b[32;1m$ make test\x1b[0;m\n",
    "test result: ok. 3 passed\n",
    "section_end:1790320422:step_script\r\x1b[0Ksection_start:1790320422:cleanup_file_variables\r\x1b[0K\x1b[0K\x1b[36;1mCleaning up project directory and file based variables\x1b[0;m\x1b[0;m\n",
    "section_end:1790320423:cleanup_file_variables\r\x1b[0K\x1b[32;1mJob succeeded\x1b[0;m\n",
);

const GITLAB_TIMESTAMPED_LOG: &str = concat!(
    "2026-10-27T10:00:00.100000Z 00O Running with gitlab-runner 17.0.0\n",
    "2026-10-27T10:00:00.100000Z 00O section_start:1793095200:prepare_executor\r\x1b[0K\n",
    "2026-10-27T10:00:00.100100Z 00O+\x1b[0K\x1b[36;1mPreparing the \"shell\" executor\x1b[0;m\x1b[0;m\n",
    "2026-10-27T10:00:01.200000Z 00O \x1b[0KUsing Shell (bash) executor...\x1b[0;m\n",
    "2026-10-27T10:00:02.300000Z 00O section_end:1793095202:prepare_executor\r\x1b[0Ksection_start:1793095202:step_script\r\x1b[0K\n",
    "2026-10-27T10:00:02.300100Z 00O+\x1b[0K\x1b[36;1mExecuting \"step_script\" stage of the job script\x1b[0;m\n",
    "2026-10-27T10:00:03.400000Z 00O progress 10%\rprogress 100%\n",
    "2026-10-27T10:00:04.500000Z 00O section_end:1793095204:step_script\r\x1b[0K\n",
    "2026-10-27T10:00:04.500000Z 00O Job succeeded\n",
);

type SectionRow = (String, Option<u32>, Option<u32>, u32, bool);

fn ingest_gitlab(log: &str) -> (Vec<SectionRow>, Vec<String>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let ingester = ingester(dir.path(), dialect("gitlab"));
    let (tx, mut rx) = mpsc::channel(1);
    tx.try_send(Bytes::copy_from_slice(log.as_bytes()))
        .expect("channel has room");
    drop(tx);

    let meta = ingester
        .run(&mut rx)
        .expect("ingest")
        .finish(None)
        .expect("finish")
        .meta;

    let sections = meta
        .sections
        .iter()
        .map(|s| (s.title.clone(), s.parent, s.header_line, s.end_line, s.open))
        .collect();
    let text = Document::open(dir.path())
        .expect("open")
        .text_block(0)
        .expect("block");
    let lines = std::str::from_utf8(&text)
        .expect("utf8")
        .lines()
        .map(str::to_string)
        .collect();
    (sections, lines)
}

#[test]
fn gitlab_sections_survive_ingest() {
    let (sections, lines) = ingest_gitlab(GITLAB_LOG);

    let row = |title: &str, header: u32, end: u32, open: bool| -> SectionRow {
        (title.to_string(), None, Some(header), end, open)
    };
    assert_eq!(
        sections,
        vec![
            row("Preparing the \"docker+machine\" executor", 2, 4, true),
            row("Preparing environment", 4, 6, true),
            row("Getting source from Git repository", 6, 8, false),
            row(
                "Executing \"step_script\" stage of the job script",
                8,
                11,
                true
            ),
            row(
                "Cleaning up project directory and file based variables",
                11,
                12,
                true
            ),
        ]
    );
    assert_eq!(lines.len(), 13);
    assert!(lines[12].ends_with("Job succeeded"));
    assert!(lines.iter().all(|line| !line.contains("section_")));
}

#[test]
fn gitlab_text_after_a_section_end_follows_the_section() {
    let (sections, lines) = ingest_gitlab(concat!(
        "section_start:1700000000:build[collapsed=true]\r\x1b[0KBuild\n",
        "compiling\n",
        "section_end:1700000010:build\r\x1b[0KDone\n",
        "after the section\n",
    ));

    assert_eq!(
        sections,
        vec![("Build".to_string(), None, Some(0), 2, false)]
    );
    assert_eq!(
        lines,
        vec![
            "2023-11-14T22:13:20.000Z Build",
            "compiling",
            "2023-11-14T22:13:30.000Z Done",
            "after the section",
        ]
    );
}

#[test]
fn gitlab_runner_timestamps_and_continued_headers_survive_ingest() {
    let (sections, lines) = ingest_gitlab(GITLAB_TIMESTAMPED_LOG);

    assert_eq!(
        sections,
        vec![
            ("prepare_executor".to_string(), None, Some(1), 4, true),
            ("step_script".to_string(), None, Some(4), 7, true),
        ]
    );
    assert_eq!(lines.len(), 8);
    assert_eq!(
        lines[0],
        "2026-10-27T10:00:00.100Z Running with gitlab-runner 17.0.0"
    );
    assert_eq!(lines[6], "2026-10-27T10:00:03.400Z progress 100%");
    assert_eq!(lines[7], "2026-10-27T10:00:04.500Z Job succeeded");
    assert!(lines.iter().all(|line| !line.contains("00O")));
}

fn error_texts(dialect: &'static dyn LogDialect, log: &str) -> Vec<String> {
    let dir = tempfile::tempdir().expect("tempdir");
    let ingester = ingester(dir.path(), dialect);
    let (tx, mut rx) = mpsc::channel(1);
    tx.try_send(Bytes::copy_from_slice(log.as_bytes()))
        .expect("channel has room");
    drop(tx);

    let meta = ingester
        .run(&mut rx)
        .expect("ingest")
        .finish(None)
        .expect("finish")
        .meta;

    let text = Document::open(dir.path())
        .expect("open")
        .text_block(0)
        .expect("block");
    let lines: Vec<&str> = std::str::from_utf8(&text).expect("utf8").lines().collect();
    assert_eq!(meta.error_lines.len() as u32, meta.error_total);
    meta.error_lines
        .iter()
        .map(|&line| lines[line as usize].to_string())
        .collect()
}

fn assert_errors(found: &[String], expected: &[&str]) {
    assert_eq!(found.len(), expected.len(), "{found:#?}");
    for (line, suffix) in found.iter().zip(expected) {
        assert!(
            line.ends_with(suffix),
            "{line:?} does not end with {suffix:?}"
        );
    }
}

#[test]
fn jenkins_console_notes_are_dropped_before_search_and_error_inference() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ingester = ingester(dir.path(), dialect("jenkins"));
    let (tx, mut rx) = mpsc::channel(1);
    tx.try_send(Bytes::from_static(
        concat!(
            "\x1b[8mha:////4IkPeNOtKgYQ==\x1b[0m[Pipeline] Start of Pipeline\n",
            "Started by user \x1b[8mha:////4O3qAAAA=\x1b[0madmin\n",
            "\x1b[8mha:////4IkPeNOtKgYQ==\x1b[0mERROR: Job failed: exit code 1\n",
        )
        .as_bytes(),
    ))
    .expect("channel has room");
    drop(tx);

    let meta = ingester
        .run(&mut rx)
        .expect("ingest")
        .finish(None)
        .expect("finish")
        .meta;

    let text = Document::open(dir.path())
        .expect("open")
        .text_block(0)
        .expect("block");
    assert_eq!(
        std::str::from_utf8(&text).expect("utf8"),
        "[Pipeline] Start of Pipeline\nStarted by user admin\nERROR: Job failed: exit code 1\n"
    );
    assert!(meta.error_lines.is_empty());

    let found = error_texts(
        &PLAIN,
        "\x1b[8mha:////4IkPeNOtKgYQ==\x1b[0mERROR: Job failed: exit code 1\n",
    );
    assert_eq!(found, ["ERROR: Job failed: exit code 1"]);
}

#[test]
fn jenkins_unstable_build_is_an_error_like_its_failed_status() {
    let found = error_texts(
        dialect("jenkins"),
        concat!(
            "Started by user admin\n",
            "[app] $ /bin/sh -xe /tmp/jenkins123.sh\n",
            "+ make test\n",
            "Tests run: 4, Failures: 1\n",
            "Finished: UNSTABLE\n",
        ),
    );

    assert_errors(&found, &["Finished: UNSTABLE"]);
}

#[test]
fn gitlab_failed_job_flags_the_runner_verdict_and_tool_failures() {
    let found = error_texts(
        dialect("gitlab"),
        concat!(
            "Running with gitlab-runner 18.4.0 (139a0ac0)\n",
            "section_start:1790320399:step_script\r\x1b[0K\x1b[0K\x1b[36;1mExecuting \"step_script\" stage of the job script\x1b[0;m\x1b[0;m\n",
            "\x1b[32;1m$ npm test\x1b[0;m\n",
            "FAIL src/login.test.ts\n",
            "npm ERR! Test failed.  See above for more details.\n",
            "section_end:1790320422:step_script\r\x1b[0Ksection_start:1790320422:cleanup_file_variables\r\x1b[0K\x1b[0K\x1b[36;1mCleaning up project directory and file based variables\x1b[0;m\x1b[0;m\n",
            "section_end:1790320423:cleanup_file_variables\r\x1b[0K\x1b[31;1mERROR: Job failed: exit code 1\x1b[0;m\n",
        ),
    );

    assert_errors(
        &found,
        &[
            "FAIL src/login.test.ts",
            "npm ERR! Test failed.  See above for more details.",
            "ERROR: Job failed: exit code 1",
        ],
    );
}

#[test]
fn gitea_failed_job_flags_the_runner_verdict_and_tool_failures() {
    let found = error_texts(
        dialect("gitea"),
        concat!(
            "2026-10-28T10:15:30.0000000Z ⭐ Run Main npm test\n",
            "2026-10-28T10:15:31.0000000Z > app@1.0.0 test\n",
            "2026-10-28T10:15:31.5000000Z FAIL src/login.test.ts\n",
            "2026-10-28T10:15:32.0000000Z npm ERR! Test failed.  See above for more details.\n",
            "2026-10-28T10:15:33.0000000Z ❌  Failure - Main npm test\n",
            "2026-10-28T10:15:33.0000000Z exitcode '1': failure\n",
        ),
    );

    assert_errors(
        &found,
        &[
            "FAIL src/login.test.ts",
            "npm ERR! Test failed.  See above for more details.",
            "❌  Failure - Main npm test",
            "exitcode '1': failure",
        ],
    );
}

#[test]
fn bitbucket_failed_step_flags_test_and_build_failures() {
    let found = error_texts(
        dialect("bitbucket"),
        concat!(
            "+ npm ci\n",
            "added 214 packages\n",
            "+ python manage.py test\n",
            "Traceback (most recent call last):\n",
            "  File \"manage.py\", line 12, in <module>\n",
            "ValueError: invalid literal for int() with base 10: 'x'\n",
            "+ make build\n",
            "make: *** [Makefile:12: build] Error 2\n",
            "Searching for test report files in directories named [test-results]\n",
        ),
    );

    assert_errors(
        &found,
        &[
            "Traceback (most recent call last):",
            "ValueError: invalid literal for int() with base 10: 'x'",
            "make: *** [Makefile:12: build] Error 2",
        ],
    );
}

#[test]
fn tekton_failed_step_flags_container_output_failures() {
    let found = error_texts(
        dialect("tekton"),
        concat!(
            "\x1e2026-10-28T10:01:01Z Step run-tests\n",
            "2026-10-28T10:01:20Z running 3 tests\n",
            "2026-10-28T10:01:30Z test login::rejects_bad_password ... FAILED\n",
            "2026-10-28T10:01:30Z test result: FAILED. 2 passed; 1 failed\n",
            "2026-10-28T10:01:31Z {\"level\":\"error\",\"msg\":\"tests failed\"}\n",
            "2026-10-28T10:01:31Z time=\"2026-10-28T10:01:31Z\" level=fatal msg=\"exit status 1\"\n",
        ),
    );

    assert_errors(
        &found,
        &[
            "test login::rejects_bad_password ... FAILED",
            "test result: FAILED. 2 passed; 1 failed",
            "{\"level\":\"error\",\"msg\":\"tests failed\"}",
            "time=\"2026-10-28T10:01:31Z\" level=fatal msg=\"exit status 1\"",
        ],
    );
}

#[test]
fn providers_with_native_error_markers_do_not_infer_more() {
    let jenkins = error_texts(
        dialect("jenkins"),
        concat!(
            "[Pipeline] sh\n",
            "+ make test\n",
            "test result: FAILED. 0 passed; 1 failed\n",
            "[Pipeline] }\n",
            "[Pipeline] // stage\n",
            "[Pipeline] error\n",
            "ERROR: script returned exit code 2\n",
            "Finished: FAILURE\n",
        ),
    );
    assert_errors(
        &jenkins,
        &["ERROR: script returned exit code 2", "Finished: FAILURE"],
    );

    let azure = error_texts(
        dialect("azure-devops"),
        concat!(
            "2026-10-28T10:00:00.0000000Z ##[section]Starting: Bash\n",
            "2026-10-28T10:00:01.0000000Z npm ERR! Test failed.\n",
            "2026-10-28T10:00:02.0000000Z ##[error]Bash exited with code '1'.\n",
            "2026-10-28T10:00:02.0000000Z ##[section]Finishing: Bash\n",
        ),
    );
    assert_errors(&azure, &["Bash exited with code '1'."]);
}
