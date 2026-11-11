use std::collections::{
    HashMap,
    HashSet,
};

use super::rows::to_json;
use super::{
    PipelineViewRepository,
    ViewExport,
};
use crate::domain::{
    DomainResult,
    PinKind,
    ViewKind,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::DatabasePool;

const ALL_DISPLAY: &str =
    r#"{"sort":"name","direction":"asc","sections":"repository","layout":"table"}"#;
const PINNED_DISPLAY: &str =
    r#"{"sort":"manual","direction":"asc","sections":"none","layout":"cards"}"#;

fn unique_name(name: &str, taken: &mut HashSet<String>) -> String {
    let mut candidate = name.to_string();
    let mut n = 2;
    while !taken.insert(candidate.to_lowercase()) {
        candidate = format!("{name} ({n})");
        n += 1;
    }
    candidate
}

impl PipelineViewRepository {
    pub async fn reset(&self) -> DomainResult<()> {
        let display = format!(
            "UPDATE pipeline_views SET display_json = {}, updated_at = {} WHERE kind = {}",
            self.pool.placeholder(1),
            self.pool.datetime_now(),
            self.pool.placeholder(2)
        );
        on_pool!(&self.pool, p => async {
            let mut tx = p.begin().await?;
            for sql in [
                "DELETE FROM pipeline_view_members",
                "DELETE FROM pipeline_views WHERE kind = 'custom'",
                "DELETE FROM pipeline_view_pins",
            ] {
                sqlx::query(sql).execute(&mut *tx).await?;
            }
            for (kind, json) in [("all", ALL_DISPLAY), ("pinned", PINNED_DISPLAY)] {
                sqlx::query(sqlx::AssertSqlSafe(display.clone()))
                    .bind(json)
                    .bind(kind)
                    .execute(&mut *tx)
                    .await?;
            }
            tx.commit().await
        }
        .await)
        .map_err(db_err)
    }

    pub async fn export(&self) -> DomainResult<ViewExport> {
        Ok(ViewExport {
            views: self.list_views().await?,
            members: self.list_members().await?,
            pins: self.list_pins().await?,
        })
    }

    pub async fn import(&self, export: &ViewExport, clean: bool) -> DomainResult<()> {
        let existing = self.list_views().await?;
        let mut view_ids: HashMap<i64, i64> = HashMap::new();
        for view in export.views.iter().filter(|v| v.kind != ViewKind::Custom) {
            if let Some(target) = existing.iter().find(|g| g.kind == view.kind) {
                view_ids.insert(view.id, target.id);
            }
        }
        let position_offset = if clean {
            0
        } else {
            existing
                .iter()
                .filter(|v| v.kind == ViewKind::Custom)
                .map(|v| v.position + 1)
                .max()
                .unwrap_or(0)
        };

        let clear: &[&str] = if clean {
            &[
                "DELETE FROM pipeline_view_members",
                "DELETE FROM pipeline_views WHERE kind = 'custom'",
                "DELETE FROM pipeline_view_pins",
            ]
        } else {
            &[]
        };
        let system = format!(
            "UPDATE pipeline_views SET display_json = {}, position = {}, updated_at = {} WHERE id = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.datetime_now(),
            self.pool.placeholder(3)
        );
        let custom = format!(
            "INSERT INTO pipeline_views (kind, name, position, rules_json, display_json)
             VALUES ('custom', {}, {}, {}, {}) RETURNING id",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.placeholder(3),
            self.pool.placeholder(4),
        );
        let view_parent = format!(
            "UPDATE pipeline_views SET parent_id = {} WHERE id = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
        );
        let member = format!(
            "INSERT INTO pipeline_view_members (view_id, provider_id, pipeline_id, position, excluded)
             VALUES ({}, {}, {}, {}, {})
             ON CONFLICT (view_id, pipeline_id) DO NOTHING",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.placeholder(3),
            self.pool.placeholder(4),
            self.pool.placeholder(5),
        );
        let pin_insert = self.pin_insert_sql();

        let mut taken: HashSet<String> = existing
            .iter()
            .filter(|v| v.kind == ViewKind::Custom)
            .map(|v| v.name.to_lowercase())
            .collect();
        let mut prepared = Vec::with_capacity(export.views.len());
        for view in &export.views {
            let name = if view.kind == ViewKind::Custom && !clean {
                unique_name(&view.name, &mut taken)
            } else {
                view.name.clone()
            };
            prepared.push((view, name, to_json(&view.rules)?, to_json(&view.display)?));
        }

        on_pool!(&self.pool, p => async {
            let mut view_ids = view_ids.clone();
            let mut tx = p.begin().await?;
            for sql in clear {
                sqlx::query(*sql).execute(&mut *tx).await?;
            }
            for (view, name, rules, display) in &prepared {
                if view.kind == ViewKind::Custom {
                    let id = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(custom.clone()))
                        .bind(name)
                        .bind(view.position + position_offset)
                        .bind(rules)
                        .bind(display)
                        .fetch_one(&mut *tx)
                        .await?;
                    view_ids.insert(view.id, id);
                } else if let Some(id) = view_ids.get(&view.id) {
                    sqlx::query(sqlx::AssertSqlSafe(system.clone()))
                        .bind(display)
                        .bind(view.position)
                        .bind(*id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            for view in &export.views {
                let parents = view
                    .parent_id
                    .and_then(|old| Some((view_ids.get(&view.id)?, view_ids.get(&old)?)));
                if let Some((id, parent)) = parents {
                    sqlx::query(sqlx::AssertSqlSafe(view_parent.clone()))
                        .bind(*parent)
                        .bind(*id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            for m in &export.members {
                let Some(view_id) = view_ids.get(&m.view_id) else {
                    continue;
                };
                sqlx::query(sqlx::AssertSqlSafe(member.clone()))
                    .bind(*view_id)
                    .bind(m.provider_id)
                    .bind(&m.pipeline_id)
                    .bind(m.position)
                    .bind(m.excluded)
                    .execute(&mut *tx)
                    .await?;
            }
            for pin in &export.pins {
                let parsed = pin.reference.parse::<i64>().ok();
                let reference = match pin.kind {
                    PinKind::Repository => Some(pin.reference.clone()),
                    PinKind::View => parsed
                        .and_then(|old| view_ids.get(&old))
                        .map(i64::to_string),
                };
                let Some(reference) = reference else {
                    continue;
                };
                sqlx::query(sqlx::AssertSqlSafe(pin_insert.clone()))
                    .bind(pin.kind.as_str())
                    .bind(reference)
                    .execute(&mut *tx)
                    .await?;
            }
            tx.commit().await
        }
        .await)
        .map_err(db_err)
    }
}
