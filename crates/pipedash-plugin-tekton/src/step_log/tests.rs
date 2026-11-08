use super::*;

#[test]
fn sidecar_cursors_reset_when_a_native_init_container_restarts() {
    let sidecar: StepState = serde_json::from_str(r#"{"name":"cache","running":{}}"#).unwrap();
    let mut pod: Pod = serde_json::from_str(r#"{"status":{"initContainerStatuses":[{"name":"sidecar-cache","ready":true,"restartCount":0,"image":"cache","imageID":"cache","containerID":"id-1"}]}}"#).unwrap();
    let steps = started_steps(std::slice::from_ref(&sidecar), &pod, true);
    let cursor = TektonCursor {
        pod: "pod-1".to_string(),
        containers: vec![ContainerCursor {
            name: steps[0].container.clone(),
            instance: steps[0].instance.clone(),
            done: false,
            capped: false,
            last: None,
        }],
    };
    assert_eq!(steps[0].instance.id, "id-1");
    assert!(cursor.continues("pod-1", &steps));
    let status = &mut pod
        .status
        .as_mut()
        .unwrap()
        .init_container_statuses
        .as_mut()
        .unwrap()[0];
    status.restart_count = 1;
    status.container_id = Some("id-2".to_string());
    let restarted = started_steps(std::slice::from_ref(&sidecar), &pod, true);
    assert!(!cursor.continues("pod-1", &restarted));
}

fn at(stamp: &str) -> DateTime<Utc> {
    stamp.parse().unwrap()
}

fn run(from: Option<Stamp>, hold_tail: bool, chunks: &[&str]) -> Result<(String, StepSummary), ()> {
    let mut filter = StepFilter::new(from, hold_tail);
    let mut out = Vec::new();
    for chunk in chunks {
        filter.push(chunk.as_bytes(), &mut out).map_err(|_| ())?;
    }
    let summary = filter.finish(&mut out).map_err(|_| ())?;
    Ok((String::from_utf8(out).unwrap(), summary))
}

const LOG: &str = "2026-10-28T10:00:01.5Z a\n2026-10-28T10:00:02Z b\n2026-10-28T10:00:02Z b\n2026-10-28T10:00:03Z c\n";

#[test]
fn the_cursor_stamps_the_last_timestamp_with_its_line_count() {
    let (text, summary) = run(None, false, &[LOG]).unwrap();

    assert_eq!(text, LOG);
    assert_eq!(summary.lines, 4);
    assert_eq!(
        summary.last,
        Some(Stamp {
            at: at("2026-10-28T10:00:03Z"),
            seen: 1
        })
    );
    let (_, summary) = run(
        None,
        false,
        &["2026-10-28T10:00:02Z x\n2026-10-28T10:00:02Z x\n"],
    )
    .unwrap();
    assert_eq!(summary.last.unwrap().seen, 2);
}

#[test]
fn resuming_drops_the_lines_already_seen_at_the_boundary_timestamp() {
    let from = Stamp {
        at: at("2026-10-28T10:00:02Z"),
        seen: 2,
    };
    let more = "2026-10-28T10:00:02Z b\n2026-10-28T10:00:04Z d\n";
    let body =
        format!("2026-10-28T10:00:01.5Z a\n2026-10-28T10:00:02Z b\n2026-10-28T10:00:02Z b\n{more}");

    let (text, summary) = run(Some(from), false, &[&body]).unwrap();

    assert_eq!(text, more);
    assert_eq!(
        summary.last,
        Some(Stamp {
            at: at("2026-10-28T10:00:04Z"),
            seen: 1
        })
    );
}

#[test]
fn resuming_with_nothing_new_keeps_the_cursor() {
    let from = Stamp {
        at: at("2026-10-28T10:00:03Z"),
        seen: 1,
    };

    let (text, summary) = run(Some(from), false, &["2026-10-28T10:00:03Z c\n"]).unwrap();

    assert_eq!(text, "");
    assert_eq!(summary.last, Some(from));
}

#[test]
fn a_boundary_line_that_vanished_diverges() {
    let from = Stamp {
        at: at("2026-10-28T10:00:02Z"),
        seen: 2,
    };
    let cases: [&[&str]; 3] = [
        &[],
        &["2026-10-28T10:00:02Z b\n2026-10-28T10:00:05Z later\n"],
        &["2026-10-28T10:00:09Z later\n"],
    ];

    for chunks in cases {
        assert!(run(Some(from), false, chunks).is_err(), "{chunks:?}");
    }
}

#[test]
fn splitting_the_stream_anywhere_yields_the_same_bytes() {
    let whole = run(None, false, &[LOG]).unwrap().0;

    for cut in 0..LOG.len() {
        let (head, tail) = LOG.split_at(cut);
        assert_eq!(run(None, false, &[head, tail]).unwrap().0, whole, "{cut}");
    }
}

#[test]
fn an_unterminated_last_line_is_held_back_while_the_container_runs() {
    let body = "2026-10-28T10:00:01Z a\n2026-10-28T10:00:02Z par";

    let (running, summary) = run(None, true, &[body]).unwrap();
    let (finished, _) = run(None, false, &[body]).unwrap();

    assert_eq!(running, "2026-10-28T10:00:01Z a\n");
    assert_eq!(summary.lines, 1);
    assert_eq!(summary.last.unwrap().at, at("2026-10-28T10:00:01Z"));
    assert_eq!(
        finished,
        "2026-10-28T10:00:01Z a\n2026-10-28T10:00:02Z par\n"
    );
}

#[test]
fn an_oversized_unterminated_line_is_cut_into_lines() {
    let big = "x".repeat(MAX_PENDING_LINE_BYTES + 1);
    let body = format!("2026-10-28T10:00:01Z {big}");

    let (text, summary) = run(None, true, &[&body]).unwrap();

    assert_eq!(text, format!("{body}\n"));
    assert_eq!(summary.lines, 1);
}
