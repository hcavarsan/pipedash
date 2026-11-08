use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::{
    Mutex,
    PoisonError,
};
use std::time::Duration;

use super::platform::Env;
use super::registration::{
    delete_registration,
    registration,
    save_registration,
    Registration,
};
use super::*;
use crate::local_notifications::store::AlertHost;
use crate::push::stub::Stub;
use crate::push::Result;
use crate::remote::test_host::{
    start_host_with_relay,
    Host,
};

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct TestEnv {
    relay: String,
    host: Mutex<Option<AlertHost>>,
    enabled: AtomicBool,
    problem: Mutex<Option<String>>,
}

impl TestEnv {
    fn new(relay: &str, host: AlertHost) -> Self {
        Self {
            relay: relay.to_owned(),
            host: Mutex::new(Some(host)),
            enabled: AtomicBool::new(true),
            problem: Mutex::new(None),
        }
    }

    fn current(&self) -> Option<AlertHost> {
        self.host
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn problem(&self) -> Option<String> {
        self.problem
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Env for TestEnv {
    fn relay_url(&self) -> &str {
        &self.relay
    }

    async fn notifications_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    async fn stored_host(&self) -> Option<AlertHost> {
        self.current()
    }

    async fn available(&self) -> bool {
        true
    }

    async fn permission_granted(&self) -> bool {
        true
    }

    async fn device_token(&self) -> Result<String> {
        Ok("apns-token".to_owned())
    }

    async fn mark_push(&self, host: &AlertHost, push: bool) {
        if let Some(stored) = self
            .host
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            if same_host(stored, host) {
                stored.push = push;
            }
        }
    }

    fn set_problem(&self, problem: Option<String>) {
        *self.problem.lock().unwrap_or_else(PoisonError::into_inner) = problem;
    }
}

async fn paired(relay: Option<&str>) -> (Host, AlertHost) {
    let host = start_host_with_relay(relay).await;
    let code = pipedash_web::pairing::create_pairing(&host.state, &host.url.url)
        .unwrap()
        .code;
    let alert_host = crate::remote::alerts::claim_alert_host(host.url.clone(), &code, true)
        .await
        .unwrap();

    (host, alert_host)
}

async fn relay_stub() -> Stub {
    Stub::start_with_body(
        |request| match (request.method.as_str(), request.path.as_str()) {
            ("POST", "/v1/subscriptions") => (201, r#"{"subscription_id":"sub-new"}"#.to_owned()),
            _ => (204, String::new()),
        },
    )
    .await
}

async fn seed_registration(device_id: &str, subscription_id: &str) {
    save_registration(Registration {
        subscription_id: subscription_id.to_owned(),
        device_id: device_id.to_owned(),
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn a_host_that_dropped_its_relay_is_told_apart_from_one_that_still_pushes() {
    let relay = "https://relay.pipedash.app";
    let (_off, off_host) = paired(None).await;
    assert_eq!(
        host_push(&off_host, relay).await,
        Some(HostPush::RelayStopped)
    );

    let (_other, other_host) = paired(Some("https://relay.example.com")).await;
    assert_eq!(
        host_push(&other_host, relay).await,
        Some(HostPush::RelayStopped)
    );

    let (on, on_host) = paired(Some(&format!("{relay}/"))).await;
    assert_eq!(
        host_push(&on_host, relay).await,
        Some(HostPush::NoRegistration)
    );

    on.core
        .device_pushes()
        .set(&on_host.device_id, "sub-1")
        .await
        .unwrap();
    assert_eq!(
        host_push(&on_host, relay).await,
        Some(HostPush::Active {
            undelivered_after: None
        })
    );

    let unreachable = AlertHost {
        server_url: "http://127.0.0.1:1".to_owned(),
        ..on_host
    };
    assert_eq!(host_push(&unreachable, relay).await, None);
}

#[tokio::test]
async fn a_sync_that_was_waiting_when_notifications_went_off_registers_nothing() {
    let _serial = SERIAL.lock().await;
    let relay = relay_stub().await;
    let (host, alert_host) = paired(Some(&relay.url)).await;
    let alert_host = AlertHost {
        push: true,
        ..alert_host
    };
    host.core
        .device_pushes()
        .set(&alert_host.device_id, "sub-old")
        .await
        .unwrap();
    seed_registration(&alert_host.device_id, "sub-old").await;
    let env = std::sync::Arc::new(TestEnv::new(&relay.url, alert_host.clone()));

    let held = PUSH_OPS.lock().await;
    let pending = tokio::spawn({
        let env = std::sync::Arc::clone(&env);
        async move { sync_with(&*env).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!pending.is_finished());

    env.enabled.store(false, Ordering::SeqCst);
    disable_with(&*env).await;
    *env.host.lock().unwrap_or_else(PoisonError::into_inner) = None;
    drop(held);
    pending.await.unwrap();

    assert_eq!(relay.count("PUT", "/v1/subscriptions/sub-old"), 0);
    assert_eq!(relay.count("POST", "/v1/subscriptions"), 0);
    assert_eq!(relay.count("DELETE", "/v1/subscriptions/sub-old"), 1);
    assert!(registration().await.unwrap().is_none());
    assert!(host
        .core
        .device_pushes()
        .get(&alert_host.device_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn a_sync_for_a_replaced_host_registers_nothing() {
    let relay = relay_stub().await;
    let (host, alert_host) = paired(Some(&relay.url)).await;
    let alert_host = AlertHost {
        push: true,
        ..alert_host
    };
    let env = std::sync::Arc::new(TestEnv::new(&relay.url, alert_host.clone()));

    let held = PUSH_OPS.lock().await;
    let pending = tokio::spawn({
        let env = std::sync::Arc::clone(&env);
        async move { sync_with(&*env).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    *env.host.lock().unwrap_or_else(PoisonError::into_inner) = Some(AlertHost {
        device_id: "other".to_owned(),
        server_url: "https://other.example.com".to_owned(),
        ..alert_host.clone()
    });
    drop(held);
    pending.await.unwrap();

    assert_eq!(relay.count("POST", "/v1/subscriptions"), 0);
    assert!(host
        .core
        .device_pushes()
        .get(&alert_host.device_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn a_host_that_now_offers_push_is_set_up_on_the_next_sync() {
    let _serial = SERIAL.lock().await;
    let relay = relay_stub().await;
    let (host, alert_host) = paired(Some(&relay.url)).await;
    delete_registration().await.unwrap();
    let env = TestEnv::new(&relay.url, alert_host.clone());
    env.set_problem(Some("old problem".to_owned()));

    sync_with(&env).await;

    assert!(env.current().unwrap().push);
    assert_eq!(env.problem(), None);
    assert_eq!(relay.count("POST", "/v1/subscriptions"), 1);
    let row = host
        .core
        .device_pushes()
        .get(&alert_host.device_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.relay_subscription_id, "sub-new");
    assert_eq!(
        registration().await.unwrap().unwrap().subscription_id,
        "sub-new"
    );
}

#[tokio::test]
async fn a_host_that_still_offers_no_push_is_left_alone() {
    let relay = relay_stub().await;
    let (_host, alert_host) = paired(None).await;
    let env = TestEnv::new(&relay.url, alert_host);

    sync_with(&env).await;

    assert!(!env.current().unwrap().push);
    assert_eq!(relay.count("POST", "/v1/subscriptions"), 0);
}

#[tokio::test]
async fn a_host_that_stopped_its_relay_ends_push_and_explains_why() {
    let _serial = SERIAL.lock().await;
    let relay = relay_stub().await;
    let (_host, alert_host) = paired(None).await;
    seed_registration(&alert_host.device_id, "sub-old").await;
    let env = TestEnv::new(
        &relay.url,
        AlertHost {
            push: true,
            ..alert_host
        },
    );

    sync_with(&env).await;

    assert!(!env.current().unwrap().push);
    assert_eq!(
        env.problem().as_deref(),
        Some("Studio stopped sending push alerts. Pipedash checks it in the background instead.")
    );
    assert_eq!(relay.count("DELETE", "/v1/subscriptions/sub-old"), 1);
    assert!(registration().await.unwrap().is_none());
}
