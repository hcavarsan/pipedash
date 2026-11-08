use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use pipedash_core::domain::ServerUrl;
use pipedash_web::pairing::create_pairing;
use serde_json::{
    json,
    Value,
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::connect::{
    open,
    require_private,
    revoke,
};
use super::driver::{
    backoff,
    FailOnDrop,
};
use super::protocol::{
    parse_server_frame,
    request_frame,
    revokes_access,
    AuthOk,
    ClientHello,
    ServerFrame,
};
use super::Session;
use crate::remote::model::RemoteError;
use crate::remote::store::HostKind;
use crate::remote::test_host::{
    claim,
    start_host,
};

#[test]
fn a_response_carries_its_text_or_base64_body() {
    assert_eq!(
        parse_server_frame(r#"{"type":"response","id":7,"status":200,"body":"{\"ok\":true}"}"#),
        Some(ServerFrame::Response {
            id: 7,
            status: 200,
            body: br#"{"ok":true}"#.to_vec(),
        })
    );
    assert_eq!(
        parse_server_frame(r#"{"type":"response","id":8,"status":200,"bodyBase64":"AP8="}"#),
        Some(ServerFrame::Response {
            id: 8,
            status: 200,
            body: vec![0, 255],
        })
    );
    assert_eq!(
        parse_server_frame(r#"{"type":"response","id":9,"status":204,"body":""}"#),
        Some(ServerFrame::Response {
            id: 9,
            status: 204,
            body: Vec::new(),
        })
    );
    assert_eq!(
        parse_server_frame(r#"{"type":"response","id":9,"status":200,"bodyBase64":"%"}"#),
        None
    );
}

#[test]
fn auth_frames_keep_the_server_code_and_message() {
    assert_eq!(
        parse_server_frame(
            r#"{"type":"auth-error","payload":{"code":"HOST_LOCKED","message":"Locked"}}"#
        ),
        Some(ServerFrame::AuthError {
            code: "HOST_LOCKED".to_string(),
            message: "Locked".to_string(),
        })
    );
    assert_eq!(
        parse_server_frame(
            r#"{"type":"auth-ok","payload":{"host":"desktop","name":"Studio","deviceId":"d1","token":"t"}}"#
        ),
        Some(ServerFrame::AuthOk(AuthOk {
            host: HostKind::Desktop,
            name: Some("Studio".to_string()),
            device_id: Some("d1".to_string()),
            token: Some("t".to_string()),
        }))
    );
    assert_eq!(
        parse_server_frame(
            r#"{"type":"auth-ok","payload":{"host":"server","name":null,"deviceId":null}}"#
        ),
        Some(ServerFrame::AuthOk(AuthOk {
            host: HostKind::Server,
            name: None,
            device_id: None,
            token: None,
        }))
    );
}

#[test]
fn other_frames_are_events_and_garbage_is_ignored() {
    assert_eq!(
        parse_server_frame(r#"{"type":"pipelines-updated","payload":{"provider_id":3}}"#),
        Some(ServerFrame::Event {
            name: "pipelines-updated".to_string(),
            payload: json!({ "provider_id": 3 }),
        })
    );
    for garbage in [
        "",
        "nope",
        "[]",
        r#"{"payload":{}}"#,
        r#"{"type":"response"}"#,
    ] {
        assert_eq!(parse_server_frame(garbage), None, "{garbage}");
    }
}

#[test]
fn only_a_rejected_device_loses_access() {
    assert!(revokes_access("AUTH_FAILED"));
    assert!(revokes_access("DEVICE_REMOVED"));
    for code in ["HOST_LOCKED", "SERVER_NOT_READY", "AUTH_TIMEOUT", ""] {
        assert!(!revokes_access(code), "{code}");
    }
}

#[test]
fn reconnects_back_off_by_half_again_up_to_thirty_seconds() {
    let millis: Vec<u128> = (0..=9).map(|n| backoff(n).as_millis()).collect();
    assert_eq!(
        millis,
        [1000, 1000, 1500, 2250, 3375, 5062, 7593, 11390, 17085, 25628]
    );
    assert_eq!(backoff(10).as_millis(), 30_000);
    assert_eq!(backoff(u32::MAX).as_millis(), 30_000);
}

#[tokio::test]
async fn a_request_waits_for_its_own_response() {
    let (outgoing, mut queue) = mpsc::channel(4);
    let session = Arc::new(Session::new("host", outgoing));

    let asking = Arc::clone(&session);
    let first = tokio::spawn(async move {
        asking
            .request("GET", "/a", None, Duration::from_secs(5))
            .await
    });
    let asking = Arc::clone(&session);
    let second = tokio::spawn(async move {
        asking
            .request("POST", "/b", Some("{}"), Duration::from_secs(5))
            .await
    });

    for _ in 0..2 {
        let Some(Message::Text(text)) = queue.recv().await else {
            panic!("no request frame");
        };
        let frame: Value = serde_json::from_str(&text).unwrap();
        let id = frame["id"].as_u64().unwrap();
        let path = frame["path"].as_str().unwrap().to_string();
        session.resolve(id, 200, path.into_bytes());
    }

    assert_eq!(first.await.unwrap().unwrap(), (200, b"/a".to_vec()));
    assert_eq!(second.await.unwrap().unwrap(), (200, b"/b".to_vec()));
}

#[tokio::test]
async fn an_unanswered_request_times_out_and_a_lost_socket_fails_it() {
    let (outgoing, queue) = mpsc::channel(4);
    let session = Arc::new(Session::new("host", outgoing));

    let result = session
        .request("GET", "/slow", None, Duration::from_millis(20))
        .await;
    assert!(matches!(result, Err(RemoteError::Timeout)));
    assert!(session.pending().is_empty());

    let waiting = Arc::clone(&session);
    let pending = tokio::spawn(async move {
        waiting
            .request("GET", "/later", None, Duration::from_secs(5))
            .await
    });
    while session.pending().is_empty() {
        tokio::task::yield_now().await;
    }
    session.fail_all();
    assert!(matches!(
        pending.await.unwrap(),
        Err(RemoteError::Unreachable(host)) if host == "host"
    ));

    drop(queue);
    let result = session
        .request("GET", "/gone", None, Duration::from_secs(5))
        .await;
    assert!(matches!(result, Err(RemoteError::Unreachable(_))));
    assert!(session.pending().is_empty());
}

#[tokio::test]
async fn a_full_queue_counts_against_the_request_deadline() {
    let (outgoing, _queue) = mpsc::channel(1);
    let session = Session::new("host", outgoing);
    session
        .outgoing
        .send(request_frame(0, "GET", "/fill", None))
        .await
        .unwrap();

    let started = tokio::time::Instant::now();
    let result = session
        .request("GET", "/blocked", None, Duration::from_millis(50))
        .await;

    assert!(matches!(result, Err(RemoteError::Timeout)));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(session.pending().is_empty());
}

#[tokio::test]
async fn dropping_the_serving_task_fails_the_requests_waiting_on_it() {
    let (outgoing, mut queue) = mpsc::channel(4);
    let session = Arc::new(Session::new("host", outgoing));
    let guard = FailOnDrop(Arc::clone(&session));

    let waiting = Arc::clone(&session);
    let pending = tokio::spawn(async move {
        waiting
            .request("GET", "/hang", None, Duration::from_secs(60))
            .await
    });
    queue.recv().await.unwrap();
    drop(guard);

    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), pending)
            .await
            .unwrap()
            .unwrap(),
        Err(RemoteError::Unreachable(host)) if host == "host"
    ));
}

fn addrs(list: &[&str]) -> Vec<SocketAddr> {
    list.iter().map(|addr| addr.parse().unwrap()).collect()
}

#[test]
fn plain_http_needs_every_resolved_address_to_be_private() {
    assert!(require_private("nas", &addrs(&["192.168.1.20:8080"])).is_ok());
    assert!(require_private("nas", &addrs(&["10.0.0.5:80", "[fd00::1]:80"])).is_ok());
    assert!(require_private("nas", &addrs(&["127.0.0.1:80"])).is_ok());

    for public in [
        &["8.8.8.8:80"][..],
        &["192.168.1.20:80", "8.8.8.8:80"][..],
        &["[2001:4860:4860::8888]:80"][..],
        &["[::ffff:8.8.8.8]:80"][..],
    ] {
        assert!(
            matches!(
                require_private("nas", &addrs(public)),
                Err(RemoteError::PublicHttp)
            ),
            "{public:?}"
        );
    }
    assert!(matches!(
        require_private("nas", &[]),
        Err(RemoteError::Unreachable(host)) if host == "nas"
    ));
}

#[tokio::test]
async fn a_public_address_over_plain_http_is_refused_before_connecting() {
    let url = ServerUrl::parse("http://8.8.8.8:8080").unwrap();

    let Err(error) = open(&url, ClientHello::Auth { token: "t" }).await else {
        panic!("a public address must not get the token");
    };
    assert!(matches!(error, RemoteError::PublicHttp));
}

#[tokio::test]
async fn a_code_pairs_one_device_and_a_revoked_device_is_turned_away() {
    let host = start_host().await;
    let code = create_pairing(&host.state, &host.url.url).unwrap().code;

    let (stream, ok) = open(&host.url, claim(&code)).await.unwrap();
    assert_eq!(ok.host, HostKind::Desktop);
    assert_eq!(ok.name.as_deref(), Some("Studio"));
    let token = ok.token.expect("a claim returns the device token");
    let device_id = ok.device_id.expect("a claim returns the device id");

    let Err(second) = open(&host.url, claim(&code)).await else {
        panic!("a pairing code works once");
    };
    assert!(matches!(
        second,
        RemoteError::Rejected { ref code, .. } if code == "PAIRING_CODE_INVALID"
    ));

    let (_, again) = open(&host.url, ClientHello::Auth { token: &token })
        .await
        .unwrap();
    assert_eq!(again.device_id.as_deref(), Some(device_id.as_str()));
    assert_eq!(again.token, None);

    revoke(stream, &device_id).await;
    let Err(rejected) = open(&host.url, ClientHello::Auth { token: &token }).await else {
        panic!("a revoked device can't sign in");
    };
    assert!(matches!(
        rejected,
        RemoteError::Rejected { ref code, .. } if code == "AUTH_FAILED"
    ));
}

#[tokio::test]
async fn an_unknown_address_is_unreachable() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = ServerUrl::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    drop(listener);

    let Err(error) = open(&url, ClientHello::Auth { token: "t" }).await else {
        panic!("nothing listens there");
    };
    assert!(matches!(error, RemoteError::Unreachable(host) if host == "127.0.0.1"));
}

#[tokio::test]
async fn a_cancelled_request_leaves_nothing_pending() {
    let (outgoing, _queue) = mpsc::channel(4);
    let session = Session::new("host", outgoing);

    let waiting = session.request("GET", "/cancel", None, Duration::from_secs(60));
    let _ = tokio::time::timeout(Duration::from_millis(20), waiting).await;

    assert!(session.pending().is_empty());
}

#[test]
fn the_cursor_queue_keeps_the_highest_position_until_a_reset_drops_it() {
    let (outgoing, _queue) = mpsc::channel(1);
    let session = Session::new("host", outgoing);
    assert!(!session.queue_cursor(5));

    session.mark_alerts_synced();
    assert!(session.queue_cursor(5));
    assert!(!session.queue_cursor(9));
    assert!(!session.queue_cursor(7));
    let cursor = session.take_cursor().unwrap();
    assert_eq!(cursor.seen, 9);

    session.reset_cursor();
    assert_ne!(session.cursor_epoch(), cursor.epoch);
    assert!(session.take_cursor().is_none());
    assert!(session.queue_cursor(1));
}
