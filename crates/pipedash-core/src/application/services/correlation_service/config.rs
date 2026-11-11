#[cfg(test)]
mod tests;

use std::collections::{
    BTreeMap,
    HashMap,
    HashSet,
};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use super::resolve::{
    finish,
    Resolved,
};
use super::{
    copy,
    CorrelationService,
};
use crate::application::services::provider_service::ApplicationsConfig;
use crate::domain::application::dto::ConfigLoadError;
use crate::domain::application::IdentityKind;
use crate::domain::pipeline_id::SEPARATOR;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::database::{
    ApplicationRow,
    CorrelationConn,
};
use crate::infrastructure::PipedashConfig;

#[async_trait]
impl ApplicationsConfig for CorrelationService {
    async fn check(&self, loaded: &PipedashConfig) -> DomainResult<()> {
        if loaded.applications.is_empty() {
            return Ok(());
        }
        let providers: HashMap<i64, String> = self
            .repository
            .config_backend()
            .list_providers()
            .await?
            .into_iter()
            .filter_map(|p| p.id.map(|id| (id, p.name)))
            .collect();
        let units: HashSet<String> = self
            .store
            .conn()
            .list_subject_identities()
            .await?
            .into_iter()
            .filter(|row| row.unit_id.is_some())
            .map(|row| row.subject_id)
            .collect();
        let pipelines = self.repository.get_cached_pipelines(None).await?;

        let mut claimed: BTreeMap<&str, &str> = BTreeMap::new();
        for (key, application) in &loaded.applications {
            let key = key.as_str();
            for pipeline in pipelines.iter().filter(|p| units.contains(&p.id)) {
                let Some(provider) = providers.get(&pipeline.provider_id) else {
                    continue;
                };
                let local = pipeline
                    .id
                    .splitn(3, SEPARATOR)
                    .nth(2)
                    .unwrap_or(&pipeline.id);
                let member = application
                    .members
                    .iter()
                    .any(|m| m.matches(provider, local));
                let excluded = application
                    .exclude
                    .iter()
                    .any(|s| s.matches(provider, local));
                if !member || excluded {
                    continue;
                }
                match claimed.insert(&pipeline.id, key) {
                    Some(other) if other != key => {
                        return Err(DomainError::InvalidConfig(copy::config_claims_both(
                            other, key, local,
                        )));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    async fn commit(&self, loaded: &PipedashConfig) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        let remembered = self.config_state.get().await;
        if remembered.applications == loaded.applications
            && remembered.correlation == loaded.correlation
            && self.config_rows_match(loaded).await?
        {
            if self.config_error.write().await.take().is_some() {
                self.changed();
                self.flush_events().await;
            }
            return Ok(());
        }
        let mut tx = self.store.begin().await?;
        let result = self.commit_in(&mut tx, loaded).await;
        let resolved = finish(tx, result).await?;

        *self.config_error.write().await = None;
        self.settle(resolved);
        self.changed();
        self.flush_events().await;
        Ok(())
    }

    async fn failed(&self, message: String) {
        *self.config_error.write().await = Some(ConfigLoadError {
            message,
            at: Utc::now(),
        });
        self.changed();
        self.flush_events().await;
    }
}

impl CorrelationService {
    async fn config_rows_match(&self, loaded: &PipedashConfig) -> DomainResult<bool> {
        let kind = IdentityKind::Config.as_str();
        let rows = self.store.conn().list_applications().await?;
        let stored: Vec<(&str, &str, bool)> = rows
            .iter()
            .filter(|r| r.identity_kind == kind)
            .map(|r| (r.identity_key.as_str(), r.name.as_str(), r.hidden))
            .collect();
        Ok(stored.len() == loaded.applications.len()
            && loaded
                .applications
                .iter()
                .all(|(key, app)| stored.contains(&(key.as_str(), app.name.as_str(), app.hidden))))
    }

    async fn commit_in(
        &self, conn: &mut CorrelationConn, loaded: &PipedashConfig,
    ) -> DomainResult<Resolved> {
        let wanted = &loaded.applications;
        let kind = IdentityKind::Config.as_str();
        let now = Utc::now();
        let existing = conn.list_applications().await?;
        for row in existing
            .iter()
            .filter(|r| r.identity_kind == kind && !wanted.contains_key(&r.identity_key))
        {
            conn.delete_application(row.id).await?;
        }
        for (key, application) in wanted {
            let created_at = existing
                .iter()
                .find(|r| r.identity_kind == kind && &r.identity_key == key)
                .map_or(now, |r| r.created_at);
            conn.upsert_application(&ApplicationRow {
                id: 0,
                identity_kind: kind.to_string(),
                identity_key: key.clone(),
                name: application.name.clone(),
                hidden: application.hidden,
                archived_at: None,
                created_at,
            })
            .await?;
        }
        self.resolve_all_with(conn, Arc::new(loaded.clone())).await
    }
}
