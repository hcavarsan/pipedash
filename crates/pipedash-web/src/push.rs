use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::join_all;
use pipedash_core::application::{
    PipelineService,
    ProviderService,
};
use pipedash_core::domain::{
    DevicePush,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    PushSubscription,
};
use pipedash_core::event::CoreEvent;
use pipedash_core::infrastructure::database::PushRepository;
use pipedash_core::infrastructure::{
    ConfigKey,
    ConfigState,
    HttpClientManager,
};
use pipedash_core::CoreContext;
use pipedash_plugin_api::ClientOptions;
use serde::Serialize;
use tokio::sync::broadcast::error::RecvError;

use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;
use crate::ws::closed;

const RELAY_TIMEOUT: Duration = Duration::from_secs(10);
const RUN_LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);
const RUN_LOOKUP_LIMIT: usize = 3;
const FAILED_SUFFIX: &str = " failed";
const DEGRADED_SUFFIX: &str = " is degraded";
const MAX_TITLE_CHARS: usize = 120;
const MAX_BODY_CHARS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayOutcome {
    Delivered,
    Gone,
    RateLimited,
    Failed,
}

fn relay_outcome(status: u16) -> RelayOutcome {
    match status {
        202 => RelayOutcome::Delivered,
        404 | 410 => RelayOutcome::Gone,
        429 => RelayOutcome::RateLimited,
        _ => RelayOutcome::Failed,
    }
}

pub struct Alert {
    pub title: String,
    pub body: String,
    pub thread_id: Option<String>,
    pub pipeline_id: Option<String>,
    pub provider_id: Option<u64>,
    pub run_number: Option<u64>,
}

#[derive(Serialize)]
struct RelayNotification<'a> {
    subscription_id: &'a str,
    title: &'a str,
    body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pipeline_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_number: Option<u64>,
}

#[derive(Serialize)]
struct RelayAlert<'a> {
    subscription_id: &'a str,
    alert_id: &'a str,
}

pub fn normalize_relay_url(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');

    (!url.is_empty()).then(|| url.to_owned())
}

async fn post_relay<T: Serialize>(
    http: &HttpClientManager, relay_url: &str, body: &T,
) -> RelayOutcome {
    let client = match http.client_for(&ClientOptions::default()) {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!(%error, "Couldn't build the push relay client");
            return RelayOutcome::Failed;
        }
    };

    let response = client
        .post(format!("{relay_url}/v1/notifications"))
        .timeout(RELAY_TIMEOUT)
        .json(body)
        .send()
        .await;

    match response {
        Ok(response) => relay_outcome(response.status().as_u16()),
        Err(error) => {
            tracing::warn!(error = %error.without_url(), "Push relay request failed");
            RelayOutcome::Failed
        }
    }
}

pub async fn send_alert(
    http: &HttpClientManager, relay_url: &str, subscription_id: &str, alert_id: &str,
) -> RelayOutcome {
    post_relay(
        http,
        relay_url,
        &RelayAlert {
            subscription_id,
            alert_id,
        },
    )
    .await
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }

    let mut truncated: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');

    truncated
}

fn failure_alert(pipeline: &Pipeline, provider: &str, run: Option<&PipelineRun>) -> Alert {
    let branch = run
        .and_then(|run| run.branch.as_deref())
        .or(pipeline.branch.as_deref());
    let details = [
        Some(provider.to_owned()),
        branch.map(str::to_owned),
        run.map(|run| format!("#{}", run.run_number)),
        pipeline
            .resource_state
            .as_ref()
            .and_then(|state| state.message.clone()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let suffix = match pipeline.status {
        PipelineStatus::Degraded => DEGRADED_SUFFIX,
        _ => FAILED_SUFFIX,
    };

    Alert {
        title: format!(
            "{}{suffix}",
            truncate(&pipeline.name, MAX_TITLE_CHARS - suffix.len())
        ),
        body: truncate(&details, MAX_BODY_CHARS),
        thread_id: Some(pipeline.id.clone()),
        pipeline_id: Some(pipeline.id.clone()),
        provider_id: u64::try_from(pipeline.provider_id).ok(),
        run_number: run.and_then(|run| u64::try_from(run.run_number).ok()),
    }
}

pub struct PushServices {
    pub subscriptions: PushRepository,
    config: Arc<ConfigState>,
    http: Arc<HttpClientManager>,
    providers: Arc<ProviderService>,
    pipelines: Arc<PipelineService>,
}

impl PushServices {
    pub async fn from_state(state: &AppState) -> ApiResult<Self> {
        let inner = state.inner.read().await;
        let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;

        Ok(Self {
            subscriptions: PushRepository::new(
                core.provider_service.repository().cache_pool().clone(),
            ),
            config: Arc::clone(&core.config_state),
            http: Arc::clone(&core.http_client_manager),
            providers: Arc::clone(&core.provider_service),
            pipelines: Arc::clone(&core.pipeline_service),
        })
    }

    pub async fn relay_url(&self) -> Option<String> {
        normalize_relay_url(&self.config.get_string(ConfigKey::PushRelayUrl).await)
    }

    pub async fn deliver(
        &self, relay_url: &str, subscription: &PushSubscription, alert: &Alert,
    ) -> RelayOutcome {
        let outcome = self.send(relay_url, subscription, alert).await;
        let result = match outcome {
            RelayOutcome::Delivered => self.subscriptions.mark_success(&subscription.id).await,
            RelayOutcome::Gone => self
                .subscriptions
                .delete(&subscription.id)
                .await
                .map(|_| ()),
            RelayOutcome::RateLimited | RelayOutcome::Failed => {
                tracing::warn!(?outcome, "The push relay did not accept a notification");
                Ok(())
            }
        };
        if let Err(error) = result {
            tracing::warn!(%error, "Couldn't update a push subscription");
        }

        outcome
    }

    async fn send(
        &self, relay_url: &str, subscription: &PushSubscription, alert: &Alert,
    ) -> RelayOutcome {
        post_relay(
            &self.http,
            relay_url,
            &RelayNotification {
                subscription_id: &subscription.relay_subscription_id,
                title: &alert.title,
                body: &alert.body,
                thread_id: alert.thread_id.as_deref(),
                pipeline_id: alert.pipeline_id.as_deref(),
                provider_id: alert.provider_id,
                run_number: alert.run_number,
            },
        )
        .await
    }

    async fn failed_run(&self, pipeline: &Pipeline) -> Option<PipelineRun> {
        let history = self
            .pipelines
            .fetch_run_history(&pipeline.id, RUN_LOOKUP_LIMIT);
        let runs = match tokio::time::timeout(RUN_LOOKUP_TIMEOUT, history).await {
            Ok(Ok(runs)) => runs,
            Ok(Err(error)) => {
                tracing::debug!(%error, "Couldn't look up the failed run for a push");
                return None;
            }
            Err(_) => return None,
        };

        runs.into_iter()
            .filter(|run| run.status == PipelineStatus::Failed)
            .max_by_key(|run| run.run_number)
    }

    async fn failure_alert(&self, pipeline: &Pipeline) -> Alert {
        let provider = match self
            .providers
            .repository()
            .config_backend()
            .get_provider(pipeline.provider_id)
            .await
        {
            Ok(Some(config)) => config.name,
            _ => pipeline.provider_type.clone(),
        };
        let run = match pipeline.status {
            PipelineStatus::Failed => self.failed_run(pipeline).await,
            _ => None,
        };

        failure_alert(pipeline, &provider, run.as_ref())
    }

    async fn notify_failures(&self, pipelines: &[Pipeline]) {
        let Some(relay_url) = self.relay_url().await else {
            return;
        };
        let subscriptions = match self.subscriptions.list().await {
            Ok(subscriptions) if !subscriptions.is_empty() => subscriptions,
            Ok(_) => return,
            Err(error) => {
                tracing::warn!(%error, "Couldn't load push subscriptions");
                return;
            }
        };

        for pipeline in pipelines {
            let alert = self.failure_alert(pipeline).await;
            join_all(
                subscriptions
                    .iter()
                    .map(|subscription| self.deliver(&relay_url, subscription, &alert)),
            )
            .await;
        }
    }
}

pub fn spawn_failure_notifier(state: AppState) {
    let mut events = state.ws_event_bus.subscribe();

    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(CoreEvent::PipelinesFailed { pipelines }) => {
                    let state = state.clone();
                    tokio::spawn(async move {
                        if let Ok(services) = PushServices::from_state(&state).await {
                            services.notify_failures(&pipelines).await;
                        }
                    });
                }
                Ok(_) => {}
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "The push notifier missed events");
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

pub async fn deliver_alert(
    core: &CoreContext, relay_url: &str, push: &DevicePush, alert_id: &str, seq: Option<i64>,
) -> RelayOutcome {
    let outcome = send_alert(
        &core.http_client_manager,
        relay_url,
        &push.relay_subscription_id,
        alert_id,
    )
    .await;
    let pushes = core.device_pushes();
    let result = match outcome {
        RelayOutcome::Delivered => pushes.mark_success(&push.device_id).await,
        RelayOutcome::Gone => pushes
            .delete_matching(&push.device_id, &push.relay_subscription_id)
            .await
            .map(|_| ()),
        RelayOutcome::RateLimited | RelayOutcome::Failed => {
            tracing::debug!(?outcome, "The push relay did not accept an alert");
            match seq {
                Some(seq) => {
                    pushes
                        .mark_undelivered(&push.device_id, &push.relay_subscription_id, seq)
                        .await
                }
                None => Ok(()),
            }
        }
    };
    if let Err(error) = result {
        tracing::warn!(%error, "Couldn't update a device push registration");
    }

    outcome
}

async fn push_alert(state: &AppState, alert_id: &str, seq: i64) {
    let Some(relay_url) = state.push_relay_url().await else {
        return;
    };
    let Ok(core) = state.core().await else {
        return;
    };
    let pushes = match core.device_pushes().list().await {
        Ok(pushes) => pushes,
        Err(error) => {
            tracing::warn!(%error, "Couldn't load device push registrations");
            return;
        }
    };
    let alert = core.device_alerts().get(alert_id).await.ok().flatten();
    let preferences = match core.devices().notification_preferences().await {
        Ok(preferences) => preferences,
        Err(error) => {
            tracing::warn!(%error, "Couldn't load device notification preferences");
            HashMap::new()
        }
    };
    let wanted = |push: &&DevicePush| {
        let chosen = alert.as_ref().zip(preferences.get(&push.device_id));
        chosen.is_none_or(|(alert, preferences)| {
            preferences.allows(alert.kind, alert.provider_id, alert.pipeline_id.as_deref())
        })
    };

    join_all(
        pushes
            .iter()
            .filter(wanted)
            .map(|push| deliver_alert(&core, &relay_url, push, alert_id, Some(seq))),
    )
    .await;
}

pub fn spawn_alert_push(state: AppState) {
    let mut events = state.ws_event_bus.subscribe();
    let mut closing = state.closing.subscribe();

    tokio::spawn(async move {
        loop {
            let event = tokio::select! {
                event = events.recv() => event,
                () = closed(&mut closing) => break,
            };
            match event {
                Ok(CoreEvent::AlertRecorded { id, seq }) => {
                    let state = state.clone();
                    tokio::spawn(async move { push_alert(&state, &id, seq).await });
                }
                Ok(_) => {}
                Err(RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "The alert push sender missed events");
                }
                Err(RecvError::Closed) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::future::Future;

    use chrono::Utc;
    use pipedash_core::domain::{
        AlertKind,
        NewDeviceAlert,
        NotificationPreferences,
    };
    use pipedash_core::event::EventBus;

    use super::*;
    use crate::test_support::{
        hosted,
        stub_server,
    };

    fn pipeline(name: &str, branch: Option<&str>) -> Pipeline {
        serde_json::from_value(serde_json::json!({
            "id": "github__1__acme/app/ci.yml",
            "provider_id": 1,
            "provider_type": "github",
            "name": name,
            "status": "failed",
            "last_run": null,
            "last_updated": Utc::now(),
            "repository": "acme/app",
            "branch": branch,
            "workflow_file": null,
        }))
        .unwrap()
    }

    fn run(number: i64, branch: Option<&str>) -> PipelineRun {
        serde_json::from_value(serde_json::json!({
            "id": number.to_string(),
            "pipeline_id": "github__1__acme/app/ci.yml",
            "run_number": number,
            "status": "failed",
            "started_at": Utc::now(),
            "logs_url": "",
            "branch": branch,
        }))
        .unwrap()
    }

    #[test]
    fn a_failure_names_the_pipeline_provider_branch_run_and_thread() {
        let alert = failure_alert(
            &pipeline("CI", Some("main")),
            "GitHub",
            Some(&run(42, None)),
        );

        assert_eq!(alert.title, "CI failed");
        assert_eq!(alert.body, "GitHub · main · #42");
        assert_eq!(
            alert.thread_id.as_deref(),
            Some("github__1__acme/app/ci.yml")
        );
        assert_eq!(
            alert.pipeline_id.as_deref(),
            Some("github__1__acme/app/ci.yml")
        );
        assert_eq!(alert.provider_id, Some(1));
        assert_eq!(alert.run_number, Some(42));
    }

    #[test]
    fn the_run_branch_wins_and_missing_parts_are_left_out() {
        let with_run_branch = failure_alert(
            &pipeline("CI", Some("main")),
            "GitHub",
            Some(&run(7, Some("release"))),
        );
        let provider_only = failure_alert(&pipeline("CI", None), "GitHub", None);

        assert_eq!(with_run_branch.body, "GitHub · release · #7");
        assert_eq!(provider_only.body, "GitHub");
    }

    #[test]
    fn long_names_are_cut_so_the_relay_accepts_them() {
        let alert = failure_alert(
            &pipeline(&"é".repeat(500), None),
            "p".repeat(1000).as_str(),
            None,
        );

        assert_eq!(alert.title.chars().count(), MAX_TITLE_CHARS);
        assert!(alert.title.ends_with("… failed"));
        assert_eq!(alert.body.chars().count(), MAX_BODY_CHARS);
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn a_degraded_pipeline_says_so_and_explains_why() {
        let mut degraded = pipeline("web", None);
        degraded.status = PipelineStatus::Degraded;
        degraded.resource_state = Some(pipedash_core::domain::ResourceState::new(
            "Degraded",
            Some("Pod web/web-7f9c: back-off restarting failed container"),
        ));

        let alert = failure_alert(&degraded, "Argo CD", None);

        assert_eq!(alert.title, "web is degraded");
        assert_eq!(
            alert.body,
            "Argo CD · Pod web/web-7f9c: back-off restarting failed container"
        );
        assert_eq!(alert.run_number, None);
    }

    #[test]
    fn relay_statuses_map_to_outcomes() {
        assert_eq!(relay_outcome(202), RelayOutcome::Delivered);
        assert_eq!(relay_outcome(404), RelayOutcome::Gone);
        assert_eq!(relay_outcome(410), RelayOutcome::Gone);
        assert_eq!(relay_outcome(429), RelayOutcome::RateLimited);
        assert_eq!(relay_outcome(400), RelayOutcome::Failed);
        assert_eq!(relay_outcome(502), RelayOutcome::Failed);
    }

    #[test]
    fn relay_urls_are_trimmed_and_blank_means_none() {
        assert_eq!(
            normalize_relay_url(" https://relay.example.com/ ").as_deref(),
            Some("https://relay.example.com")
        );
        assert_eq!(normalize_relay_url("  /  "), None);
        assert_eq!(normalize_relay_url(""), None);
    }

    async fn eventually<F, Fut>(what: &str, mut check: F)
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = bool>,
    {
        let wait = async {
            while !check().await {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };

        if tokio::time::timeout(Duration::from_secs(10), wait)
            .await
            .is_err()
        {
            panic!("timed out waiting for {what}");
        }
    }

    async fn subscribed(state: &AppState, subscriptions: &[&str]) {
        let core = state.core().await.unwrap();
        for subscription in subscriptions {
            let (device, _) = core.pair_device(subscription, "ios").await.unwrap();
            core.device_pushes()
                .set(&device.id, subscription)
                .await
                .unwrap();
        }
    }

    fn recorded(id: &str) -> CoreEvent {
        CoreEvent::AlertRecorded {
            id: id.into(),
            seq: 1,
        }
    }

    #[tokio::test]
    async fn a_recorded_alert_reaches_every_subscribed_device_with_only_its_id() {
        let host = hosted("alert-push").await;
        let relay = stub_server().await;
        let state = host
            .state
            .clone()
            .with_push_relay(&format!("{}/", relay.url));
        subscribed(&state, &["sub-1", "sub-2"]).await;
        spawn_alert_push(state.clone());

        state.ws_event_bus.emit(recorded("a1")).await;
        eventually("both pushes", || async { relay.requests().len() == 2 }).await;

        let mut requests = relay.requests();
        requests.sort_by_key(|request| request["subscription_id"].to_string());
        assert_eq!(
            requests,
            vec![
                serde_json::json!({ "subscription_id": "sub-1", "alert_id": "a1" }),
                serde_json::json!({ "subscription_id": "sub-2", "alert_id": "a1" }),
            ]
        );
        let core = state.core().await.unwrap();
        eventually("both successes to be recorded", || async {
            let pushes = core.device_pushes().list().await.unwrap();
            pushes.len() == 2 && pushes.iter().all(|push| push.last_success_at.is_some())
        })
        .await;
    }

    #[tokio::test]
    async fn a_device_that_turned_off_an_alert_kind_is_not_pushed_it() {
        let host = hosted("alert-push-preferences").await;
        let relay = stub_server().await;
        let state = host.state.clone().with_push_relay(&relay.url);
        subscribed(&state, &["sub-1", "sub-2"]).await;
        let core = state.core().await.unwrap();
        let quiet = core.device_pushes().list().await.unwrap();
        let quiet = quiet
            .iter()
            .find(|push| push.relay_subscription_id == "sub-2")
            .unwrap();
        core.devices()
            .set_notification_preferences(
                &quiet.device_id,
                &NotificationPreferences {
                    long_run: false,
                    ..NotificationPreferences::default()
                },
            )
            .await
            .unwrap();
        let alert = |kind| NewDeviceAlert {
            key: "pipeline:api".into(),
            kind,
            title: "api".into(),
            body: String::new(),
            pipeline_id: Some("api".into()),
            provider_id: Some(1),
        };
        let long_run = core
            .device_alerts()
            .insert(&alert(AlertKind::LongRun))
            .await
            .unwrap();
        let failed = core
            .device_alerts()
            .insert(&alert(AlertKind::Failed))
            .await
            .unwrap();
        spawn_alert_push(state.clone());

        state.ws_event_bus.emit(recorded(&long_run.id)).await;
        eventually("the long run push", || async {
            !relay.requests().is_empty()
        })
        .await;
        state.ws_event_bus.emit(recorded(&failed.id)).await;
        eventually("the failure pushes", || async {
            relay.requests().len() == 3
        })
        .await;

        let mut requests = relay.requests();
        requests.sort_by_key(|request| request.to_string());
        let mut expected = vec![
            serde_json::json!({ "subscription_id": "sub-1", "alert_id": long_run.id }),
            serde_json::json!({ "subscription_id": "sub-1", "alert_id": failed.id }),
            serde_json::json!({ "subscription_id": "sub-2", "alert_id": failed.id }),
        ];
        expected.sort_by_key(|request| request.to_string());
        assert_eq!(requests, expected);
    }

    #[tokio::test]
    async fn a_device_the_relay_no_longer_knows_is_forgotten_after_an_alert_push() {
        let host = hosted("alert-push-gone").await;
        let relay = stub_server().await;
        relay.reply_with(404);
        let state = host.state.clone().with_push_relay(&relay.url);
        subscribed(&state, &["sub-1"]).await;
        spawn_alert_push(state.clone());

        state.ws_event_bus.emit(recorded("a1")).await;

        let core = state.core().await.unwrap();
        eventually("the device to be forgotten", || async {
            core.device_pushes().list().await.unwrap().is_empty()
        })
        .await;
        assert_eq!(relay.requests().len(), 1);
    }

    #[tokio::test]
    async fn nothing_is_sent_without_a_relay_or_after_the_host_stops() {
        let host = hosted("alert-push-idle").await;
        let relay = stub_server().await;
        subscribed(&host.state, &["sub-1"]).await;
        spawn_alert_push(host.state.clone());
        host.state.ws_event_bus.emit(recorded("a1")).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(relay.requests().is_empty());

        let state = host.state.clone().with_push_relay(&relay.url);
        spawn_alert_push(state.clone());
        state.closing.send_replace(true);
        tokio::time::sleep(Duration::from_millis(100)).await;
        state.ws_event_bus.emit(recorded("a2")).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(relay.requests().is_empty());
    }

    #[tokio::test]
    async fn a_host_reads_the_relay_from_its_builder() {
        let host = hosted("alert-relay-builder").await;
        assert_eq!(host.state.push_relay_url().await, None);
        let state = host
            .state
            .clone()
            .with_push_relay("https://relay.example.org/");
        assert_eq!(
            state.push_relay_url().await.as_deref(),
            Some("https://relay.example.org")
        );
    }
}

#[cfg(test)]
mod gone_tests {
    use super::*;
    use crate::test_support::{
        hosted,
        stub_server,
    };

    #[tokio::test]
    async fn a_stale_gone_answer_keeps_the_registration_that_replaced_it() {
        let host = hosted("gone-replaced").await;
        let relay = stub_server().await;
        relay.reply_with(410);
        let core = host.state.core().await.unwrap();
        let (device, _) = core.pair_device("Pixel", "ios").await.unwrap();
        core.device_pushes().set(&device.id, "sub-a").await.unwrap();
        let stale = core.device_pushes().get(&device.id).await.unwrap().unwrap();
        core.device_pushes().set(&device.id, "sub-b").await.unwrap();

        let outcome = deliver_alert(&core, &relay.url, &stale, "a1", None).await;

        assert_eq!(outcome, RelayOutcome::Gone);
        assert_eq!(relay.requests().len(), 1);
        let kept = core.device_pushes().get(&device.id).await.unwrap().unwrap();
        assert_eq!(kept.relay_subscription_id, "sub-b");

        let current = kept;
        deliver_alert(&core, &relay.url, &current, "a2", None).await;
        assert!(core
            .device_pushes()
            .get(&device.id)
            .await
            .unwrap()
            .is_none());
    }
}
