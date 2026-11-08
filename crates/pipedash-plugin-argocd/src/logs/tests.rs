use super::*;

fn app(history: &str) -> Application {
    serde_json::from_str(&format!(
        r#"{{"metadata":{{"name":"web"}},"spec":{{"source":{{"repoURL":"https://github.com/acme/web"}},"destination":{{}},"project":"default"}},
            "status":{{"sync":{{"status":"Synced"}},"health":{{"status":"Healthy"}},"history":{history},
            "operationState":{{"phase":"Succeeded","startedAt":"2026-10-27T10:00:00Z","finishedAt":"2026-10-27T10:00:40Z"}}}}}}"#
    ))
    .unwrap()
}

fn ts(stamp: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(stamp)
        .unwrap()
        .timestamp()
}

#[test]
fn history_entry_of_the_latest_operation_shows_the_operation() {
    let app = app(
        r#"[{"revision":"abc","deployedAt":"2026-10-26T09:00:30Z","deployStartedAt":"2026-10-26T09:00:00Z","id":1},
            {"revision":"def","deployedAt":"2026-10-27T10:00:41Z","deployStartedAt":"2026-10-27T10:00:00Z","id":2}]"#,
    );

    let cases = [
        ("2026-10-27T10:00:00Z", Some("operation")),
        ("2026-10-27T10:00:41Z", Some("operation")),
        ("2026-10-26T09:00:30Z", Some("history 1")),
        ("2026-10-26T09:00:00Z", None),
    ];
    for (stamp, expected) in cases {
        let found = find_sync(&app, ts(stamp)).map(|sync| match sync {
            SyncRecord::Operation(_) => "operation".to_string(),
            SyncRecord::History(entry) => format!("history {}", entry.id),
        });
        assert_eq!(found.as_deref(), expected, "{stamp}");
    }
}

#[test]
fn pod_node_ids_accept_only_kubernetes_names() {
    assert_eq!(
        pod_of_node("pod:web:web-7f9c.abc"),
        Some(("web", "web-7f9c.abc"))
    );
    for id in [
        "",
        "sync",
        "pod:",
        "pod:web",
        "pod:web:",
        "pod:Web:a",
        "pod:web:a:b",
        "pod:web:a/b",
        "pod/web/a",
        "pod:web:a?b=1",
        "pod:..:a",
        "pod:web:-a",
    ] {
        assert_eq!(pod_of_node(id), None, "{id}");
    }
}
