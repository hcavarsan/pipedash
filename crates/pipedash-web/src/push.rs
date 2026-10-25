use std::sync::Arc;
use std::time::Duration;

use futures_util::future::join_all;
use pipedash_core::application::{
    PipelineService,
    ProviderService,
};
use pipedash_core::domain::{
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
use pipedash_plugin_api::ClientOptions;
use serde::Serialize;
use tokio::sync::broadcast::error::RecvError;

use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

const RELAY_TIMEOUT: Duration = Duration::from_secs(10);
const RUN_LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);
const RUN_LOOKUP_LIMIT: usize = 3;
const FAILED_SUFFIX: &str = " failed";
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
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");

    Alert {
        title: format!(
            "{}{FAILED_SUFFIX}",
            truncate(&pipeline.name, MAX_TITLE_CHARS - FAILED_SUFFIX.len())
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
        let url = self.config.get_string(ConfigKey::PushRelayUrl).await;
        let url = url.trim().trim_end_matches('/');

        (!url.is_empty()).then(|| url.to_owned())
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
        let client = match self.http.client_for(&ClientOptions::default()) {
            Ok(client) => client,
            Err(error) => {
                tracing::warn!(%error, "Couldn't build the push relay client");
                return RelayOutcome::Failed;
            }
        };

        let response = client
            .post(format!("{relay_url}/v1/notifications"))
            .timeout(RELAY_TIMEOUT)
            .json(&RelayNotification {
                subscription_id: &subscription.relay_subscription_id,
                title: &alert.title,
                body: &alert.body,
                thread_id: alert.thread_id.as_deref(),
                pipeline_id: alert.pipeline_id.as_deref(),
                provider_id: alert.provider_id,
                run_number: alert.run_number,
            })
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
        let run = self.failed_run(pipeline).await;

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

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

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
    fn relay_statuses_map_to_outcomes() {
        assert_eq!(relay_outcome(202), RelayOutcome::Delivered);
        assert_eq!(relay_outcome(404), RelayOutcome::Gone);
        assert_eq!(relay_outcome(410), RelayOutcome::Gone);
        assert_eq!(relay_outcome(429), RelayOutcome::RateLimited);
        assert_eq!(relay_outcome(400), RelayOutcome::Failed);
        assert_eq!(relay_outcome(502), RelayOutcome::Failed);
    }
}
