pub(crate) use octocrab::models::workflows::Run;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct PendingDeployment {
    pub environment: PendingEnvironment,
    pub current_user_can_approve: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PendingEnvironment {
    pub id: u64,
    pub name: String,
}
