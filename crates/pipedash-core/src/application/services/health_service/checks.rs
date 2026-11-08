use std::sync::Arc;

use tokio::time::timeout;

use super::state::CheckOutcome;
use super::INSTANCE_TIMEOUT;
use crate::application::fetch_scheduler::Generation;
use crate::application::services::provider_service::ProviderService;
use crate::domain::{
    DomainError,
    Provider,
};

pub(super) async fn run_check(
    service: Arc<ProviderService>, id: i64, generation: Generation,
) -> CheckOutcome {
    let lease = match service.acquire(id).await {
        Ok(lease) if lease.generation == generation => lease,
        Ok(_) => return CheckOutcome::Failed,
        Err(e) => {
            tracing::debug!(provider_id = id, error = %e, "Controller health check could not load the provider");
            return CheckOutcome::Failed;
        }
    };
    check_instance(&*lease.provider, id).await
}

pub(super) async fn check_instance(provider: &dyn Provider, id: i64) -> CheckOutcome {
    match timeout(INSTANCE_TIMEOUT, provider.check_health()).await {
        Ok(Ok(components)) => CheckOutcome::Components(components),
        Ok(Err(DomainError::PermissionDenied(_))) => CheckOutcome::Denied,
        Ok(Err(DomainError::PipelineNotFound(_) | DomainError::NotFound(_))) => {
            CheckOutcome::NotInstalled
        }
        Ok(Err(e)) => {
            tracing::debug!(provider_id = id, error = %e, "Controller health check failed");
            CheckOutcome::Failed
        }
        Err(_) => {
            tracing::debug!(provider_id = id, "Controller health check timed out");
            CheckOutcome::Failed
        }
    }
}
