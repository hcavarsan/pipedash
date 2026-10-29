use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::thread;
use std::time::{
    Duration,
    Instant,
};

use bytes::Bytes;
use regex::Regex;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::testing::{
    cache,
    doc_key,
    feed,
    finish,
    ingester,
};
use super::{
    BlockLimits,
    Document,
    LogStore,
};
use crate::domain::LogCursor;

const LINES: u32 = 3_000_000;
const CHUNK_BYTES: usize = 64 * 1024;

fn vm_hwm_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

fn write_line(buf: &mut Vec<u8>, i: u32) {
    let ms = u64::from(i) * 7;
    let (sec, ms) = (ms / 1000, ms % 1000);
    let _ = write!(
        buf,
        "2026-01-01T{:02}:{:02}:{:02}.{:03}Z ",
        sec / 3600 % 24,
        sec / 60 % 60,
        sec % 60,
        ms
    );
    let (module, file, took) = (i % 977, i.wrapping_mul(7) % 313, i.wrapping_mul(13) % 997);
    if i % 1000 == 0 {
        let _ = write!(buf, "##[group]Run build step {}", i / 1000);
    } else if i % 1000 == 999 {
        buf.extend_from_slice(b"##[endgroup]");
    } else if i % 50_000 == 7 {
        let _ = write!(buf, "##[error]boom at line {i}");
    } else if i % 10 == 0 {
        let _ = write!(
            buf,
            "\u{1b}[32mstep {i} build crate_{module} ok /work/src/mod_{module}/f{file}.rs in {took}ms\u{1b}[0m"
        );
    } else {
        let _ = write!(
            buf,
            "step {i} build crate_{module} ok /work/src/mod_{module}/f{file}.rs in {took}ms"
        );
    }
    buf.push(b'\n');
}

fn dir_bytes(dir: &std::path::Path) -> u64 {
    fs::read_dir(dir)
        .expect("read dir")
        .flatten()
        .map(|e| e.metadata().map_or(0, |m| m.len()))
        .sum()
}

#[test]
#[ignore]
fn three_million_lines() {
    let hwm_before = vm_hwm_kib();
    let dir = tempfile::tempdir().expect("tempdir");
    let (tx, mut rx) = mpsc::channel::<Bytes>(16);
    let producer = thread::spawn(move || {
        let mut total = 0u64;
        let mut buf = Vec::with_capacity(CHUNK_BYTES * 2);
        for i in 0..LINES {
            write_line(&mut buf, i);
            if buf.len() >= CHUNK_BYTES {
                total += buf.len() as u64;
                let chunk = std::mem::replace(&mut buf, Vec::with_capacity(CHUNK_BYTES * 2));
                tx.blocking_send(Bytes::from(chunk))
                    .expect("ingest stopped");
            }
        }
        total += buf.len() as u64;
        tx.blocking_send(Bytes::from(buf)).expect("ingest stopped");
        total
    });

    let started = Instant::now();
    let outcome = ingester(dir.path(), doc_key(), 1, Vec::new(), BlockLimits::default())
        .run(&mut rx)
        .expect("ingest")
        .finish(None)
        .expect("finish");
    let ingest_time = started.elapsed();
    let raw_bytes = producer.join().expect("producer");
    let mb_per_s = raw_bytes as f64 / 1e6 / ingest_time.as_secs_f64();
    let disk = dir_bytes(dir.path());
    println!(
        "ingest: {} MB in {:.2}s = {:.0} MB/s, disk {} MB ({:.1}%)",
        raw_bytes / 1_000_000,
        ingest_time.as_secs_f64(),
        mb_per_s,
        disk / 1_000_000,
        disk as f64 / raw_bytes as f64 * 100.0
    );
    assert_eq!(outcome.meta.line_count, LINES - 3000);
    assert_eq!(outcome.meta.error_total, 60);
    assert!(mb_per_s >= 100.0, "ingest too slow: {mb_per_s:.0} MB/s");
    assert!(
        (disk as f64) < raw_bytes as f64 * 0.4,
        "disk usage too high: {disk}"
    );

    let doc = Document::open(dir.path()).expect("open");
    let cache = cache();
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut timings: Vec<Duration> = (0..1000)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let start = (seed % u64::from(outcome.meta.line_count - 500)) as u32;
            let at = Instant::now();
            let page = doc.read_lines(&cache, start, 500, 4096).expect("page");
            let elapsed = at.elapsed();
            assert_eq!(page.lines.len(), 500);
            elapsed
        })
        .collect();
    timings.sort();
    let p99 = timings[989];
    println!(
        "read_lines: p50 {:?} p99 {:?} max {:?}",
        timings[500], p99, timings[999]
    );
    assert!(p99 < Duration::from_millis(10), "p99 too slow: {p99:?}");

    let literal = Regex::new("boom at line 1").expect("regex");
    let at = Instant::now();
    let found = doc
        .search(&literal, &CancellationToken::new())
        .expect("search");
    let literal_time = at.elapsed();
    println!(
        "literal search: {:?}, {} matches",
        literal_time, found.total
    );
    assert!(found.total > 0);
    assert!(
        literal_time < Duration::from_millis(1500),
        "literal search too slow"
    );

    let pattern = Regex::new(r"crate_9[0-9]{2} ok .*f31[0-9]\.rs").expect("regex");
    let at = Instant::now();
    let found = doc
        .search(&pattern, &CancellationToken::new())
        .expect("search");
    let regex_time = at.elapsed();
    println!("regex search: {:?}, {} matches", regex_time, found.total);
    assert!(found.total > 0);
    assert!(
        regex_time < Duration::from_millis(1500),
        "regex search too slow"
    );

    if let (Some(before), Some(after)) = (hwm_before, vm_hwm_kib()) {
        let growth_mib = after.saturating_sub(before) / 1024;
        println!("VmHWM growth: {growth_mib} MiB");
        assert!(growth_mib < 64, "memory growth too high: {growth_mib} MiB");
    }
}

const BASE_LINES: u32 = 1_000_000;
const INCREMENT_LINES: u32 = 1_500;
const INCREMENTS: u32 = 40;

fn raw_lines(from: u32, to: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    for i in from..to {
        write_line(&mut buf, i);
    }
    buf
}

fn percentile(timings: &mut [Duration], percent: usize) -> Duration {
    timings.sort();
    timings[(timings.len() - 1) * percent / 100]
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn repeated_appends() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = LogStore::open(dir.path().join("run-logs"), u64::MAX)
        .await
        .expect("store");
    let key = doc_key();
    let mut loads = Vec::new();
    let mut ingests = Vec::new();
    let mut finishes = Vec::new();
    let mut publishes = Vec::new();
    let mut totals = Vec::new();
    let mut first_total = Duration::ZERO;
    let mut next_line = 0;
    for step in 0..=INCREMENTS {
        let to = next_line
            + if step == 0 {
                BASE_LINES
            } else {
                INCREMENT_LINES
            };
        let data = raw_lines(next_line, to);
        next_line = to;

        let started = Instant::now();
        let base = store.base(&key).await.expect("base");
        let load = started.elapsed();
        let expected = base.as_ref().map(|base| base.revision);
        let revision = store.allocate_revision();
        let epoch = store.epoch(key.provider_id);
        let tmp = store.tmp_dir().await.expect("tmp");
        let (work_dir, work_key) = (tmp.clone(), key.clone());
        let (ingest, finish_time, outcome) = tokio::task::spawn_blocking(move || {
            let mut ingester = ingester(
                &work_dir,
                work_key,
                revision,
                Vec::new(),
                BlockLimits::default(),
            );
            ingester.complete = false;
            ingester.base = base;
            let at = Instant::now();
            let ingested = feed(ingester, &data, CHUNK_BYTES);
            let ingest = at.elapsed();
            let at = Instant::now();
            let cursor = LogCursor::new(&revision).expect("cursor");
            let outcome = finish(ingested, Some(cursor));

            (ingest, at.elapsed(), outcome)
        })
        .await
        .expect("append");
        let at = Instant::now();
        store
            .publish(&key, &tmp, revision, epoch, expected, HashSet::new())
            .await
            .expect("publish");
        let publish = at.elapsed();
        let total = started.elapsed();
        assert!(outcome.resumable);
        assert_eq!(outcome.meta.stable_lines == 0, step == 0);

        if step == 0 {
            first_total = total;
            let (document, _lease) = store
                .lease(&key, revision)
                .await
                .expect("lease")
                .expect("some");
            println!(
                "base: {BASE_LINES} lines in {:.2}s, revision {} MB (checkpoint {} KB)",
                total.as_secs_f64(),
                dir_bytes(document.dir()) / 1_000_000,
                fs::metadata(document.dir().join("checkpoint.json")).map_or(0, |m| m.len()) / 1000,
            );
            continue;
        }
        loads.push(load);
        ingests.push(ingest);
        finishes.push(finish_time);
        publishes.push(publish);
        totals.push(total);
    }

    let line_count = BASE_LINES + INCREMENTS * INCREMENT_LINES;
    let (document, _lease) = {
        let revision = store.current(&key).expect("current");
        store
            .lease(&key, revision)
            .await
            .expect("lease")
            .expect("some")
    };
    assert_eq!(document.meta().line_count, line_count - (line_count / 1000));
    for (name, timings) in [
        ("base+checkpoint load", &mut loads),
        ("copy+parse", &mut ingests),
        ("finish+checkpoint", &mut finishes),
        ("publish", &mut publishes),
        ("total", &mut totals),
    ] {
        println!(
            "append of {INCREMENT_LINES} lines, {name}: p50 {:?} p95 {:?} max {:?}",
            percentile(timings, 50),
            percentile(timings, 95),
            timings[timings.len() - 1]
        );
    }
    let median = percentile(&mut totals, 50);
    println!(
        "median append {:?} vs first full ingest {:?} ({:.1}%)",
        median,
        first_total,
        median.as_secs_f64() / first_total.as_secs_f64() * 100.0
    );
    assert!(
        median < first_total / 4,
        "an append costs too much: {median:?}"
    );
}
