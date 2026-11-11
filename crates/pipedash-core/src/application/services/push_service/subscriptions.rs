use super::alerts::PushAlert;
use super::PushService;
use crate::domain::{
    DomainError,
    DomainResult,
    PushSubscription,
};
use crate::infrastructure::push_relay::{
    post_relay,
    RelayNotification,
    RelayOutcome,
};

const MAX_SUBSCRIPTION_ID_LEN: usize = 128;

pub(crate) fn is_relay_subscription_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_SUBSCRIPTION_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn relay_not_configured() -> DomainError {
    DomainError::Unprocessable {
        code: "PUSH_RELAY_NOT_CONFIGURED",
        message: "push relay is not configured on this server".into(),
    }
}

impl PushService {
    pub async fn list_subscriptions(&self) -> DomainResult<Vec<PushSubscription>> {
        self.subscriptions.list().await
    }

    pub async fn delete_subscription(&self, id: &str) -> DomainResult<bool> {
        self.subscriptions.delete(id).await
    }

    pub async fn test_subscription(&self, id: &str) -> DomainResult<RelayOutcome> {
        let relay_url = self.relay_url().await.ok_or_else(relay_not_configured)?;
        let subscription = self
            .subscriptions
            .get(id)
            .await?
            .ok_or_else(|| DomainError::NotFound("push subscription not found".into()))?;
        let alert = PushAlert {
            title: "Pipedash test notification".to_string(),
            body: "Push notifications from this server are working.".to_string(),
            thread_id: None,
            pipeline_id: None,
            provider_id: None,
            run_number: None,
        };

        Ok(self.deliver(&relay_url, &subscription, &alert).await)
    }

    pub(super) async fn deliver(
        &self, relay_url: &str, subscription: &PushSubscription, alert: &PushAlert,
    ) -> RelayOutcome {
        let outcome = post_relay(
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
        .await;
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
}
