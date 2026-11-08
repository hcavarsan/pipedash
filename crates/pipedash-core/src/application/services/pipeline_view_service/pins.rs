use std::collections::HashSet;

use super::snapshot::resolve_pins;
use super::PipelineViewService;
use crate::domain::{
    DomainError,
    DomainResult,
    LegacyImportResult,
    LegacyMarks,
    PinKind,
    PinnedItem,
    ViewKind,
};
use crate::infrastructure::database::StoredPin;

impl PipelineViewService {
    pub async fn list_pins(&self) -> DomainResult<Vec<PinnedItem>> {
        Ok(resolve_pins(
            self.views.list_pins().await?,
            &self.views.list_views().await?,
        ))
    }

    pub async fn set_pin(&self, kind: PinKind, reference: &str, pinned: bool) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        if pinned {
            let known = match kind {
                PinKind::View => !resolve_pins(
                    vec![StoredPin {
                        kind,
                        reference: reference.to_string(),
                    }],
                    &self.views.list_views().await?,
                )
                .is_empty(),
                PinKind::Repository => {
                    self.cached()
                        .await?
                        .values()
                        .any(|p| p.repository == reference)
                        || self
                            .provider_service
                            .list_providers()
                            .await?
                            .iter()
                            .any(|p| p.configured_repositories.iter().any(|r| r == reference))
                }
            };
            if !known {
                return Err(DomainError::NotFound(format!(
                    "Nothing to pin for {} {reference}",
                    kind.as_str()
                )));
            }
            self.views.insert_pin(kind, reference).await?;
        } else {
            self.views.delete_pin(kind, reference).await?;
        }
        self.changed().await;
        Ok(())
    }

    pub async fn import_legacy_marks(
        &self, marks: LegacyMarks,
    ) -> DomainResult<LegacyImportResult> {
        let _guard = self.write_lock.lock().await;
        let configs = self.repository.config_backend().list_providers().await?;
        let resolve = |key: &str| -> Option<(i64, String)> {
            let config = configs
                .iter()
                .filter(|c| c.id.is_some() && key.starts_with(&format!("{}:", c.display_name())))
                .max_by_key(|c| c.display_name().len())?;
            let remainder = &key[config.display_name().len() + 1..];
            let (kind, rest) = remainder.split_once("__")?;
            if kind != config.provider_type {
                return None;
            }
            let id = config.id?;
            Some((id, format!("{kind}__{id}__{rest}")))
        };

        let stored = self.views.list_views().await?;
        let system = |kind: ViewKind| {
            stored
                .iter()
                .find(|g| g.kind == kind)
                .map(|g| g.id)
                .ok_or_else(|| DomainError::NotFound(format!("{} view not found", kind.as_str())))
        };
        let pinned = system(ViewKind::Pinned)?;
        let hidden = system(ViewKind::Hidden)?;
        let mut existing: HashSet<String> = self
            .views
            .list_members()
            .await?
            .into_iter()
            .filter(|m| m.view_id == pinned && !m.excluded)
            .map(|m| m.pipeline_id)
            .collect();

        let mut result = LegacyImportResult {
            imported: 0,
            skipped: 0,
        };
        for key in &marks.pinned {
            match resolve(key) {
                Some((provider_id, pipeline_id)) => {
                    if existing.insert(pipeline_id.clone()) {
                        self.views
                            .upsert_member(pinned, provider_id, &pipeline_id, false)
                            .await?;
                        result.imported += 1;
                    }
                }
                None => result.skipped += 1,
            }
        }
        for key in &marks.hidden {
            match resolve(key) {
                Some((provider_id, pipeline_id)) => {
                    self.views
                        .upsert_member(hidden, provider_id, &pipeline_id, false)
                        .await?;
                    result.imported += 1;
                }
                None => result.skipped += 1,
            }
        }
        if result.imported > 0 {
            self.changed().await;
        }
        Ok(result)
    }
}
