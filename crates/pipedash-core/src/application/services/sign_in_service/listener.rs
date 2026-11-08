use std::sync::Weak;

use tokio::time::Instant;

use super::{
    failure_message,
    SignInService,
};
use crate::domain::SignInStatus;
use crate::infrastructure::oauth::loopback::{
    self,
    Loopback,
};
use crate::infrastructure::oauth::Callback;

const TIMED_OUT: &str = "This sign-in timed out. Start it again from Pipedash.";

pub(super) async fn listen(
    service: Weak<SignInService>, id: String, loopback: Loopback, deadline: Instant,
) {
    loop {
        let (stream, url) = match tokio::time::timeout_at(deadline, loopback.accept()).await {
            Err(_) => {
                if let Some(service) = service.upgrade() {
                    service.expire(&id);
                }
                return;
            }
            Ok(Err(error)) => {
                tracing::warn!(%error, "Stopped listening for the sign-in redirect");
                return;
            }
            Ok(Ok(accepted)) => accepted,
        };
        let Some(service) = service.upgrade() else {
            return;
        };
        let outcome = service.finish(&id, Callback::from_url(&url)).await;
        let page = match &outcome {
            Ok(SignInStatus::Completed { .. }) => Ok(()),
            Ok(SignInStatus::Failed { message }) => Err(message.clone()),
            Ok(SignInStatus::Pending | SignInStatus::Expired) => Err(TIMED_OUT.to_string()),
            Err(error) => Err(failure_message(error.clone())),
        };
        loopback::respond(stream, &page).await;
        if outcome.is_ok() {
            return;
        }
    }
}
