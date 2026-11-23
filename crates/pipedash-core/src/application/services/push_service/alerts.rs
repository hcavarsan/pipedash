use std::time::Duration;

use pipedash_plugin_api::http::parse_http_url;

use super::{
    fan_out,
    PushService,
};
use crate::domain::pipeline::notice_title;
use crate::domain::{
    DomainResult,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    PushSubscription,
};
use crate::infrastructure::push_relay::{
    post_failure_webhook,
    FailureWebhookMessage,
};
use crate::infrastructure::ConfigKey;

const RUN_LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);
const RUN_LOOKUP_LIMIT: usize = 3;
const MAX_REPOSITORY_CHARS: usize = 48;
pub(super) const MAX_TITLE_CHARS: usize = 120;
pub(super) const MAX_BODY_CHARS: usize = 400;

pub struct PushAlert {
    pub title: String,
    pub body: String,
    pub thread_id: Option<String>,
    pub pipeline_id: Option<String>,
    pub provider_id: Option<u64>,
    pub run_number: Option<u64>,
}

pub(super) fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }

    let mut truncated: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');

    truncated
}

pub(super) fn failure_alert(
    pipeline: &Pipeline, provider: &str, run: Option<&PipelineRun>,
) -> PushAlert {
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
    let event = match pipeline.status {
        PipelineStatus::Degraded => "is degraded",
        _ => "failed",
    };
    let tail = notice_title(
        "",
        event,
        &truncate(&pipeline.repository, MAX_REPOSITORY_CHARS),
    );
    let name = truncate(&pipeline.name, MAX_TITLE_CHARS - tail.chars().count());

    PushAlert {
        title: format!("{name}{tail}"),
        body: truncate(&details, MAX_BODY_CHARS),
        thread_id: Some(pipeline.id.clone()),
        pipeline_id: Some(pipeline.id.clone()),
        provider_id: u64::try_from(pipeline.provider_id).ok(),
        run_number: run.and_then(|run| u64::try_from(run.run_number).ok()),
    }
}

pub(super) fn webhook_message<'a>(
    pipeline: &'a Pipeline, provider: &'a str, run: Option<&'a PipelineRun>, alert: &PushAlert,
) -> FailureWebhookMessage<'a> {
    FailureWebhookMessage {
        text: format!("{}\n{}", alert.title, alert.body),
        pipeline: &pipeline.name,
        provider,
        branch: run
            .and_then(|run| run.branch.as_deref())
            .or(pipeline.branch.as_deref()),
        run: run.map(|run| run.run_number),
    }
}

impl PushService {
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

    async fn failure_context(&self, pipeline: &Pipeline) -> (String, Option<PipelineRun>) {
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

        (provider, run)
    }

    async fn failure_webhook_url(&self) -> Option<reqwest::Url> {
        let url = self.config.get_string(ConfigKey::FailureWebhookUrl).await;
        let url = url.trim();
        if url.is_empty() {
            return None;
        }

        parse_http_url(url, "Failure webhook")
            .inspect_err(|error| tracing::warn!(%error, "Ignoring the failure webhook"))
            .ok()
    }

    async fn relay_subscriptions(&self) -> DomainResult<Option<(String, Vec<PushSubscription>)>> {
        let Some(relay_url) = self.relay_url().await else {
            return Ok(None);
        };
        let subscriptions = self.subscriptions.list().await?;

        Ok((!subscriptions.is_empty()).then_some((relay_url, subscriptions)))
    }

    pub async fn notify_failures(&self, pipelines: &[Pipeline]) -> DomainResult<()> {
        let webhook_url = self.failure_webhook_url().await;
        let relay = match self.relay_subscriptions().await {
            Ok(relay) => relay,
            Err(error) if webhook_url.is_some() => {
                tracing::warn!(%error, "Couldn't read the push subscriptions");
                None
            }
            Err(error) => return Err(error),
        };
        if webhook_url.is_none() && relay.is_none() {
            return Ok(());
        }

        for pipeline in pipelines {
            let (provider, run) = self.failure_context(pipeline).await;
            let alert = failure_alert(pipeline, &provider, run.as_ref());
            if let Some(webhook_url) = &webhook_url {
                let message = webhook_message(pipeline, &provider, run.as_ref(), &alert);
                post_failure_webhook(&self.http, webhook_url, &message).await;
            }
            let Some((relay_url, subscriptions)) = &relay else {
                continue;
            };
            fan_out(
                subscriptions
                    .iter()
                    .map(|subscription| self.deliver(relay_url, subscription, &alert))
                    .collect(),
            )
            .await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
