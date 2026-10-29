use std::collections::{
    HashMap,
    HashSet,
};

use super::DatabasePool;
use crate::domain::{
    DomainError,
    DomainResult,
    PinKind,
    ViewDisplay,
    ViewKind,
    ViewRules,
};

macro_rules! on_pool {
    ($pool:expr, |$p:ident| $body:expr) => {
        match $pool {
            DatabasePool::Sqlite($p) => $body,
            DatabasePool::Postgres($p) => $body,
        }
    };
}

type ViewRow = (i64, String, String, i64, String, String, Option<i64>);
type MemberRow = (i64, i64, String, i64, bool);
type PinRow = (String, String);

const ALL_DISPLAY: &str =
    r#"{"sort":"name","direction":"asc","sections":"repository","layout":"table"}"#;
const PINNED_DISPLAY: &str =
    r#"{"sort":"manual","direction":"asc","sections":"none","layout":"cards"}"#;

const PRUNE_ORPHAN_PINS: &str = "DELETE FROM pipeline_view_pins WHERE kind = 'view'
    AND ref NOT IN (SELECT CAST(id AS TEXT) FROM pipeline_views WHERE kind = 'custom')";

#[derive(Debug, Clone, PartialEq)]
pub struct StoredView {
    pub id: i64,
    pub kind: ViewKind,
    pub name: String,
    pub parent_id: Option<i64>,
    pub position: i64,
    pub rules: ViewRules,
    pub display: ViewDisplay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMember {
    pub view_id: i64,
    pub provider_id: i64,
    pub pipeline_id: String,
    pub position: i64,
    pub excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPin {
    pub kind: PinKind,
    pub reference: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewExport {
    pub views: Vec<StoredView>,
    pub members: Vec<StoredMember>,
    pub pins: Vec<StoredPin>,
}

fn db_error(error: sqlx::Error) -> DomainError {
    DomainError::DatabaseError(error.to_string())
}

fn parse_kind(kind: &str) -> Option<ViewKind> {
    match kind {
        "all" => Some(ViewKind::All),
        "pinned" => Some(ViewKind::Pinned),
        "hidden" => Some(ViewKind::Hidden),
        "custom" => Some(ViewKind::Custom),
        _ => None,
    }
}

fn parse_pin_kind(kind: &str) -> Option<PinKind> {
    match kind {
        "view" => Some(PinKind::View),
        "repository" => Some(PinKind::Repository),
        _ => None,
    }
}

fn stored_view(row: ViewRow) -> Option<StoredView> {
    let (id, kind, name, position, rules_json, display_json, parent_id) = row;
    let Some(kind) = parse_kind(&kind) else {
        tracing::warn!("Skipping pipeline view {id} with unknown kind {kind}");
        return None;
    };
    let rules = serde_json::from_str(&rules_json).unwrap_or_else(|e| {
        tracing::warn!("Invalid rules for pipeline view {id}: {e}");
        ViewRules::default()
    });
    let display = serde_json::from_str(&display_json).unwrap_or_else(|e| {
        tracing::warn!("Invalid display settings for pipeline view {id}: {e}");
        if kind == ViewKind::Pinned {
            ViewDisplay::pinned_default()
        } else {
            ViewDisplay::default()
        }
    });
    Some(StoredView {
        id,
        kind,
        name,
        parent_id,
        position,
        rules,
        display,
    })
}

fn to_json<T: serde::Serialize>(value: &T) -> DomainResult<String> {
    serde_json::to_string(value).map_err(|e| DomainError::InternalError(e.to_string()))
}

fn rules_without_provider(rules: &ViewRules, provider_id: i64) -> Option<ViewRules> {
    if !rules.provider_ids.contains(&provider_id) {
        return None;
    }
    let provider_ids: Vec<i64> = rules
        .provider_ids
        .iter()
        .copied()
        .filter(|id| *id != provider_id)
        .collect();
    if provider_ids.is_empty() {
        return Some(ViewRules::default());
    }
    Some(ViewRules {
        provider_ids,
        ..rules.clone()
    })
}

fn unique_name(name: &str, taken: &mut HashSet<String>) -> String {
    let mut candidate = name.to_string();
    let mut n = 2;
    while !taken.insert(candidate.to_lowercase()) {
        candidate = format!("{name} ({n})");
        n += 1;
    }
    candidate
}

pub struct PipelineViewRepository {
    pool: DatabasePool,
}

impl PipelineViewRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    fn placeholder(&self, index: usize) -> String {
        match &self.pool {
            DatabasePool::Sqlite(_) => "?".to_string(),
            DatabasePool::Postgres(_) => format!("${index}"),
        }
    }

    fn now(&self) -> &'static str {
        match &self.pool {
            DatabasePool::Sqlite(_) => "datetime('now')",
            DatabasePool::Postgres(_) => "NOW()",
        }
    }

    pub async fn list_views(&self) -> DomainResult<Vec<StoredView>> {
        let sql = "SELECT id, kind, name, position, rules_json, display_json, parent_id FROM pipeline_views ORDER BY position, id";
        let rows = on_pool!(&self.pool, |p| sqlx::query_as::<_, ViewRow>(sql)
            .fetch_all(p)
            .await)
        .map_err(db_error)?;
        Ok(rows.into_iter().filter_map(stored_view).collect())
    }

    pub async fn list_members(&self) -> DomainResult<Vec<StoredMember>> {
        let sql = "SELECT view_id, provider_id, pipeline_id, position, excluded FROM pipeline_view_members ORDER BY view_id, position, pipeline_id";
        let rows = on_pool!(&self.pool, |p| sqlx::query_as::<_, MemberRow>(sql)
            .fetch_all(p)
            .await)
        .map_err(db_error)?;
        Ok(rows
            .into_iter()
            .map(
                |(view_id, provider_id, pipeline_id, position, excluded)| StoredMember {
                    view_id,
                    provider_id,
                    pipeline_id,
                    position,
                    excluded,
                },
            )
            .collect())
    }

    pub async fn list_pins(&self) -> DomainResult<Vec<StoredPin>> {
        let sql = "SELECT kind, ref FROM pipeline_view_pins ORDER BY position, id";
        let rows = on_pool!(&self.pool, |p| sqlx::query_as::<_, PinRow>(sql)
            .fetch_all(p)
            .await)
        .map_err(db_error)?;
        Ok(rows
            .into_iter()
            .filter_map(|(kind, reference)| {
                Some(StoredPin {
                    kind: parse_pin_kind(&kind)?,
                    reference,
                })
            })
            .collect())
    }

    pub async fn insert_pin(&self, kind: PinKind, reference: &str) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO pipeline_view_pins (kind, ref, position)
             VALUES ({}, {}, (SELECT COALESCE(MAX(position), -1) + 1 FROM pipeline_view_pins))
             ON CONFLICT (kind, ref) DO NOTHING",
            self.placeholder(1),
            self.placeholder(2),
        );
        on_pool!(&self.pool, |p| sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(kind.as_str())
            .bind(reference)
            .execute(p)
            .await
            .map(|_| ()))
        .map_err(db_error)
    }

    pub async fn delete_pin(&self, kind: PinKind, reference: &str) -> DomainResult<()> {
        let sql = format!(
            "DELETE FROM pipeline_view_pins WHERE kind = {} AND ref = {}",
            self.placeholder(1),
            self.placeholder(2),
        );
        on_pool!(&self.pool, |p| sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(kind.as_str())
            .bind(reference)
            .execute(p)
            .await
            .map(|_| ()))
        .map_err(db_error)
    }

    pub async fn insert_custom(
        &self, name: &str, parent: Option<i64>, rules: &ViewRules, display: &ViewDisplay,
    ) -> DomainResult<i64> {
        let sql = format!(
            "INSERT INTO pipeline_views (kind, name, parent_id, position, rules_json, display_json)
             VALUES ('custom', {}, {}, (SELECT COALESCE(MAX(position), -1) + 1 FROM pipeline_views WHERE kind IN ('pinned', 'custom')), {}, {})
             RETURNING id",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
        );
        let rules = to_json(rules)?;
        let display = to_json(display)?;
        on_pool!(&self.pool, |p| sqlx::query_scalar::<_, i64>(
            sqlx::AssertSqlSafe(sql)
        )
        .bind(name)
        .bind(parent)
        .bind(&rules)
        .bind(&display)
        .fetch_one(p)
        .await)
        .map_err(db_error)
    }

    pub async fn update_view(
        &self, id: i64, name: &str, parent: Option<i64>, rules: &ViewRules, display: &ViewDisplay,
    ) -> DomainResult<bool> {
        let sql = format!(
            "UPDATE pipeline_views SET name = {}, parent_id = {}, rules_json = {}, display_json = {}, updated_at = {} WHERE id = {}",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.now(),
            self.placeholder(5),
        );
        let rules = to_json(rules)?;
        let display = to_json(display)?;
        let affected = on_pool!(&self.pool, |p| sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(name)
            .bind(parent)
            .bind(&rules)
            .bind(&display)
            .bind(id)
            .execute(p)
            .await
            .map(|r| r.rows_affected()))
        .map_err(db_error)?;
        Ok(affected > 0)
    }

    pub async fn delete_custom(&self, id: i64) -> DomainResult<bool> {
        let parent_of = format!(
            "SELECT parent_id FROM pipeline_views WHERE id = {} AND kind = 'custom'",
            self.placeholder(1)
        );
        let reparent = format!(
            "UPDATE pipeline_views SET parent_id = {} WHERE parent_id = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let delete_view = format!(
            "DELETE FROM pipeline_views WHERE id = {}",
            self.placeholder(1)
        );
        let delete_members = format!(
            "DELETE FROM pipeline_view_members WHERE view_id = {}",
            self.placeholder(1)
        );
        on_pool!(&self.pool, |p| async {
            let mut tx = p.begin().await?;
            let Some(parent) =
                sqlx::query_scalar::<_, Option<i64>>(sqlx::AssertSqlSafe(parent_of.clone()))
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?
            else {
                return Ok::<_, sqlx::Error>(false);
            };
            sqlx::query(sqlx::AssertSqlSafe(reparent.clone()))
                .bind(parent)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(sqlx::AssertSqlSafe(delete_members.clone()))
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(sqlx::AssertSqlSafe(delete_view.clone()))
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(PRUNE_ORPHAN_PINS).execute(&mut *tx).await?;
            tx.commit().await?;
            Ok(true)
        }
        .await)
        .map_err(db_error)
    }

    pub async fn set_positions(&self, positions: &[(i64, i64)]) -> DomainResult<()> {
        let sql = format!(
            "UPDATE pipeline_views SET position = {} WHERE id = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        on_pool!(&self.pool, |p| async {
            let mut tx = p.begin().await?;
            for (id, position) in positions {
                sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
                    .bind(*position)
                    .bind(*id)
                    .execute(&mut *tx)
                    .await?;
            }
            tx.commit().await
        }
        .await)
        .map_err(db_error)
    }

    pub async fn upsert_member(
        &self, view_id: i64, provider_id: i64, pipeline_id: &str, excluded: bool,
    ) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO pipeline_view_members (view_id, provider_id, pipeline_id, position, excluded)
             VALUES ({}, {}, {}, (SELECT COALESCE(MAX(position), -1) + 1 FROM pipeline_view_members WHERE view_id = {}), {})
             ON CONFLICT (view_id, pipeline_id) DO UPDATE SET excluded = excluded.excluded",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
        );
        on_pool!(&self.pool, |p| sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(view_id)
            .bind(provider_id)
            .bind(pipeline_id)
            .bind(view_id)
            .bind(excluded)
            .execute(p)
            .await
            .map(|_| ()))
        .map_err(db_error)
    }

    pub async fn delete_member(&self, view_id: i64, pipeline_id: &str) -> DomainResult<bool> {
        let sql = format!(
            "DELETE FROM pipeline_view_members WHERE view_id = {} AND pipeline_id = {}",
            self.placeholder(1),
            self.placeholder(2)
        );
        let affected = on_pool!(&self.pool, |p| sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(view_id)
            .bind(pipeline_id)
            .execute(p)
            .await
            .map(|r| r.rows_affected()))
        .map_err(db_error)?;
        Ok(affected > 0)
    }

    pub async fn replace_manual_members(
        &self, view_id: i64, members: &[(i64, String)], excluded: &[(i64, String)],
    ) -> DomainResult<()> {
        let clear = format!(
            "DELETE FROM pipeline_view_members WHERE view_id = {} AND excluded = FALSE",
            self.placeholder(1)
        );
        let insert = |flag: &str| {
            format!(
                "INSERT INTO pipeline_view_members (view_id, provider_id, pipeline_id, position, excluded)
                 VALUES ({}, {}, {}, {}, {flag})
                 ON CONFLICT (view_id, pipeline_id) DO UPDATE SET provider_id = excluded.provider_id, position = excluded.position, excluded = {flag}",
                self.placeholder(1),
                self.placeholder(2),
                self.placeholder(3),
                self.placeholder(4),
            )
        };
        let rows: Vec<(String, &(i64, String))> = members
            .iter()
            .map(|m| (insert("FALSE"), m))
            .chain(excluded.iter().map(|m| (insert("TRUE"), m)))
            .collect();
        on_pool!(&self.pool, |p| async {
            let mut tx = p.begin().await?;
            sqlx::query(sqlx::AssertSqlSafe(clear.clone()))
                .bind(view_id)
                .execute(&mut *tx)
                .await?;
            for (index, (sql, (provider_id, pipeline_id))) in rows.iter().enumerate() {
                sqlx::query(sqlx::AssertSqlSafe(sql.clone()))
                    .bind(view_id)
                    .bind(*provider_id)
                    .bind(pipeline_id)
                    .bind(index as i64)
                    .execute(&mut *tx)
                    .await?;
            }
            tx.commit().await
        }
        .await)
        .map_err(db_error)
    }

    pub async fn remove_provider(&self, provider_id: i64) -> DomainResult<()> {
        let delete_members = format!(
            "DELETE FROM pipeline_view_members WHERE provider_id = {}",
            self.placeholder(1)
        );
        let update_rules = format!(
            "UPDATE pipeline_views SET rules_json = {}, updated_at = {} WHERE id = {}",
            self.placeholder(1),
            self.now(),
            self.placeholder(2)
        );
        let select_rules = match &self.pool {
            DatabasePool::Sqlite(_) => "SELECT id, rules_json FROM pipeline_views",
            DatabasePool::Postgres(_) => "SELECT id, rules_json FROM pipeline_views FOR UPDATE",
        };
        on_pool!(&self.pool, |p| async {
            let mut tx = p.begin().await.map_err(db_error)?;
            sqlx::query(sqlx::AssertSqlSafe(delete_members.clone()))
                .bind(provider_id)
                .execute(&mut *tx)
                .await
                .map_err(db_error)?;
            let views = sqlx::query_as::<_, (i64, String)>(select_rules)
                .fetch_all(&mut *tx)
                .await
                .map_err(db_error)?;
            for (id, rules_json) in views {
                let Ok(rules) = serde_json::from_str::<ViewRules>(&rules_json) else {
                    continue;
                };
                let Some(rules) = rules_without_provider(&rules, provider_id) else {
                    continue;
                };
                sqlx::query(sqlx::AssertSqlSafe(update_rules.clone()))
                    .bind(to_json(&rules)?)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_error)?;
            }
            tx.commit().await.map_err(db_error)
        }
        .await)
    }

    pub async fn reset(&self) -> DomainResult<()> {
        let display = format!(
            "UPDATE pipeline_views SET display_json = {}, updated_at = {} WHERE kind = {}",
            self.placeholder(1),
            self.now(),
            self.placeholder(2)
        );
        on_pool!(&self.pool, |p| async {
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
        .map_err(db_error)
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
            self.placeholder(1),
            self.placeholder(2),
            self.now(),
            self.placeholder(3)
        );
        let custom = format!(
            "INSERT INTO pipeline_views (kind, name, position, rules_json, display_json)
             VALUES ('custom', {}, {}, {}, {}) RETURNING id",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
        );
        let view_parent = format!(
            "UPDATE pipeline_views SET parent_id = {} WHERE id = {}",
            self.placeholder(1),
            self.placeholder(2),
        );
        let member = format!(
            "INSERT INTO pipeline_view_members (view_id, provider_id, pipeline_id, position, excluded)
             VALUES ({}, {}, {}, {}, {})
             ON CONFLICT (view_id, pipeline_id) DO NOTHING",
            self.placeholder(1),
            self.placeholder(2),
            self.placeholder(3),
            self.placeholder(4),
            self.placeholder(5),
        );
        let pin_insert = format!(
            "INSERT INTO pipeline_view_pins (kind, ref, position)
             VALUES ({}, {}, (SELECT COALESCE(MAX(position), -1) + 1 FROM pipeline_view_pins))
             ON CONFLICT (kind, ref) DO NOTHING",
            self.placeholder(1),
            self.placeholder(2),
        );

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

        on_pool!(&self.pool, |p| async {
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
        .map_err(db_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        SectionMode,
        SortKey,
        ViewLayout,
    };
    use crate::infrastructure::database::init_database;

    fn kinds(views: &[StoredView]) -> Vec<ViewKind> {
        views.iter().map(|g| g.kind).collect()
    }

    async fn view_named(repo: &PipelineViewRepository, name: &str) -> StoredView {
        let views = repo.list_views().await.unwrap();
        views.into_iter().find(|v| v.name == name).unwrap()
    }

    async fn insert_plain(repo: &PipelineViewRepository, name: &str, parent: Option<i64>) -> i64 {
        repo.insert_custom(name, parent, &ViewRules::default(), &ViewDisplay::default())
            .await
            .unwrap()
    }

    async fn exercise(repo: &PipelineViewRepository) {
        repo.reset().await.unwrap();
        let views = repo.list_views().await.unwrap();
        let mut seeded = kinds(&views);
        seeded.sort_by_key(|k| k.as_str());
        assert_eq!(
            seeded,
            vec![ViewKind::All, ViewKind::Hidden, ViewKind::Pinned]
        );
        let pinned = views.iter().find(|g| g.kind == ViewKind::Pinned).unwrap();
        assert_eq!(pinned.display, ViewDisplay::pinned_default());

        let rules = ViewRules {
            repositories: vec!["acme/*".into()],
            ..Default::default()
        };
        let release = repo
            .insert_custom("Release", None, &rules, &ViewDisplay::default())
            .await
            .unwrap();
        let other = repo
            .insert_custom(
                "Other",
                None,
                &ViewRules::default(),
                &ViewDisplay::default(),
            )
            .await
            .unwrap();
        let views = repo.list_views().await.unwrap();
        let stored = views.iter().find(|g| g.id == release).unwrap();
        assert_eq!(stored.rules, rules);
        assert!(stored.position > pinned.position);

        let display = ViewDisplay {
            sort: SortKey::Status,
            sections: SectionMode::Provider,
            ..Default::default()
        };
        assert!(repo
            .update_view(release, "Releases", None, &rules, &display)
            .await
            .unwrap());
        repo.set_positions(&[(other, 0), (pinned.id, 1), (release, 2)])
            .await
            .unwrap();
        let views = repo.list_views().await.unwrap();
        let movable: Vec<i64> = views
            .iter()
            .filter(|g| matches!(g.kind, ViewKind::Pinned | ViewKind::Custom))
            .map(|g| g.id)
            .collect();
        assert_eq!(movable, vec![other, pinned.id, release]);
        let updated = views.iter().find(|g| g.id == release).unwrap();
        assert_eq!(updated.name, "Releases");
        assert_eq!(updated.display, display);

        repo.upsert_member(release, 1, "github__1__a", false)
            .await
            .unwrap();
        repo.upsert_member(release, 1, "github__1__b", true)
            .await
            .unwrap();
        repo.upsert_member(release, 1, "github__1__b", false)
            .await
            .unwrap();
        let members: Vec<(String, bool)> = repo
            .list_members()
            .await
            .unwrap()
            .into_iter()
            .filter(|m| m.view_id == release)
            .map(|m| (m.pipeline_id, m.excluded))
            .collect();
        assert_eq!(
            members,
            vec![
                ("github__1__a".to_string(), false),
                ("github__1__b".to_string(), false)
            ]
        );
        repo.upsert_member(release, 1, "github__1__c", true)
            .await
            .unwrap();

        repo.replace_manual_members(
            release,
            &[
                (1, "github__1__c".to_string()),
                (2, "gitlab__2__d".to_string()),
            ],
            &[],
        )
        .await
        .unwrap();
        let members: Vec<(String, bool)> = repo
            .list_members()
            .await
            .unwrap()
            .into_iter()
            .filter(|m| m.view_id == release)
            .map(|m| (m.pipeline_id, m.excluded))
            .collect();
        assert_eq!(
            members,
            vec![
                ("github__1__c".to_string(), false),
                ("gitlab__2__d".to_string(), false)
            ]
        );
        assert!(repo.delete_member(release, "gitlab__2__d").await.unwrap());
        assert!(!repo.delete_member(release, "gitlab__2__d").await.unwrap());

        repo.upsert_member(release, 2, "argocd__2__a", false)
            .await
            .unwrap();
        repo.remove_provider(2).await.unwrap();
        assert!(repo
            .list_members()
            .await
            .unwrap()
            .iter()
            .all(|m| m.provider_id != 2));

        let exported = repo.export().await.unwrap();
        repo.import(&exported, true).await.unwrap();
        let reimported = repo.export().await.unwrap();
        assert_eq!(reimported.views.len(), exported.views.len());
        assert_eq!(reimported.members.len(), exported.members.len());

        let child = repo
            .insert_custom(
                "Child",
                Some(view_named(repo, "Releases").await.id),
                &ViewRules::default(),
                &ViewDisplay::default(),
            )
            .await
            .unwrap();
        repo.insert_custom(
            "Grand",
            Some(child),
            &ViewRules::default(),
            &ViewDisplay::default(),
        )
        .await
        .unwrap();

        let nested = repo.export().await.unwrap();
        repo.import(&nested, true).await.unwrap();
        let release = view_named(repo, "Releases").await.id;
        let child = view_named(repo, "Child").await;
        assert_eq!(child.parent_id, Some(release));
        assert_eq!(view_named(repo, "Grand").await.parent_id, Some(child.id));
        assert_eq!(view_named(repo, "Releases").await.parent_id, None);

        assert!(repo.delete_custom(child.id).await.unwrap());
        assert_eq!(view_named(repo, "Grand").await.parent_id, Some(release));

        let current = repo
            .list_views()
            .await
            .unwrap()
            .into_iter()
            .find(|g| g.name == "Other")
            .unwrap();
        assert!(repo.delete_custom(current.id).await.unwrap());
        assert!(!repo.delete_custom(pinned.id).await.unwrap());

        repo.reset().await.unwrap();
        let mut remaining = kinds(&repo.list_views().await.unwrap());
        remaining.sort_by_key(|k| k.as_str());
        assert_eq!(
            remaining,
            vec![ViewKind::All, ViewKind::Hidden, ViewKind::Pinned]
        );
        assert!(repo.list_members().await.unwrap().is_empty());
        let pinned = repo
            .list_views()
            .await
            .unwrap()
            .into_iter()
            .find(|g| g.kind == ViewKind::Pinned)
            .unwrap();
        assert_eq!(pinned.display, ViewDisplay::pinned_default());
    }

    #[tokio::test]
    async fn sqlite_views_seed_system_rows_and_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let pool = init_database(dir.path().join("views.db")).await.unwrap();

        exercise(&PipelineViewRepository::new(DatabasePool::Sqlite(pool))).await;
    }

    async fn import_merges_or_replaces(
        source: &PipelineViewRepository, target: &PipelineViewRepository,
    ) {
        source.reset().await.unwrap();
        target.reset().await.unwrap();
        let release = source
            .insert_custom(
                "Release",
                None,
                &ViewRules::default(),
                &ViewDisplay::default(),
            )
            .await
            .unwrap();
        source
            .upsert_member(release, 1, "github__1__a", false)
            .await
            .unwrap();
        source
            .insert_pin(PinKind::View, &release.to_string())
            .await
            .unwrap();
        source
            .insert_pin(PinKind::Repository, "acme/app")
            .await
            .unwrap();

        let mine = target
            .insert_custom("Mine", None, &ViewRules::default(), &ViewDisplay::default())
            .await
            .unwrap();
        target
            .upsert_member(mine, 1, "github__1__b", false)
            .await
            .unwrap();
        let same = target
            .insert_custom(
                "release",
                None,
                &ViewRules::default(),
                &ViewDisplay::default(),
            )
            .await
            .unwrap();
        target
            .upsert_member(same, 1, "github__1__c", false)
            .await
            .unwrap();
        target
            .insert_pin(PinKind::Repository, "acme/web")
            .await
            .unwrap();
        let all = source
            .list_views()
            .await
            .unwrap()
            .into_iter()
            .find(|v| v.kind == ViewKind::All)
            .unwrap();
        let cards = ViewDisplay {
            layout: ViewLayout::Cards,
            ..all.display
        };
        source
            .update_view(all.id, &all.name, None, &all.rules, &cards)
            .await
            .unwrap();

        let export = source.export().await.unwrap();
        let members_of = |members: &[StoredMember], view: i64| {
            let mut ids: Vec<String> = members
                .iter()
                .filter(|m| m.view_id == view)
                .map(|m| m.pipeline_id.clone())
                .collect();
            ids.sort();
            ids
        };
        let pins = |pins: Vec<StoredPin>| -> Vec<(PinKind, String)> {
            pins.into_iter().map(|p| (p.kind, p.reference)).collect()
        };

        target.import(&export, false).await.unwrap();
        let views = target.list_views().await.unwrap();
        let customs: Vec<&str> = views
            .iter()
            .filter(|v| v.kind == ViewKind::Custom)
            .map(|v| v.name.as_str())
            .collect();
        assert_eq!(customs, vec!["Mine", "release", "Release (2)"]);
        let imported = view_named(target, "Release (2)").await.id;
        let members = target.list_members().await.unwrap();
        assert_eq!(members_of(&members, mine), vec!["github__1__b"]);
        assert_eq!(members_of(&members, same), vec!["github__1__c"]);
        assert_eq!(members_of(&members, imported), vec!["github__1__a"]);
        assert_eq!(
            pins(target.list_pins().await.unwrap()),
            vec![
                (PinKind::Repository, "acme/web".to_string()),
                (PinKind::View, imported.to_string()),
                (PinKind::Repository, "acme/app".to_string()),
            ]
        );
        assert_eq!(view_named(target, "All pipelines").await.display, cards);

        target.import(&export, true).await.unwrap();
        let views = target.list_views().await.unwrap();
        let release = views.iter().find(|v| v.kind == ViewKind::Custom).unwrap();
        assert_eq!(
            views.iter().filter(|v| v.kind == ViewKind::Custom).count(),
            1
        );
        assert_eq!(release.name, "Release");
        assert_eq!(
            members_of(&target.list_members().await.unwrap(), release.id),
            vec!["github__1__a"]
        );
        assert_eq!(
            pins(target.list_pins().await.unwrap()),
            vec![
                (PinKind::View, release.id.to_string()),
                (PinKind::Repository, "acme/app".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn sqlite_import_keeps_target_views_unless_cleaning() {
        let dir = tempfile::tempdir().unwrap();
        let source = init_database(dir.path().join("source.db")).await.unwrap();
        let target = init_database(dir.path().join("target.db")).await.unwrap();

        import_merges_or_replaces(
            &PipelineViewRepository::new(DatabasePool::Sqlite(source)),
            &PipelineViewRepository::new(DatabasePool::Sqlite(target)),
        )
        .await;
    }

    #[tokio::test]
    async fn sqlite_import_keeps_source_tree_depth_beside_same_named_target_view() {
        let dir = tempfile::tempdir().unwrap();
        let source = PipelineViewRepository::new(DatabasePool::Sqlite(
            init_database(dir.path().join("source.db")).await.unwrap(),
        ));
        let target = PipelineViewRepository::new(DatabasePool::Sqlite(
            init_database(dir.path().join("target.db")).await.unwrap(),
        ));
        let a = insert_plain(&target, "A", None).await;
        let b = insert_plain(&target, "B", Some(a)).await;
        let target_x = insert_plain(&target, "X", Some(b)).await;
        let x = insert_plain(&source, "X", None).await;
        let y = insert_plain(&source, "Y", Some(x)).await;
        insert_plain(&source, "Z", Some(y)).await;

        target
            .import(&source.export().await.unwrap(), false)
            .await
            .unwrap();

        let views = target.list_views().await.unwrap();
        let parent_of = |name: &str| views.iter().find(|v| v.name == name).unwrap().parent_id;
        let id_of = |name: &str| views.iter().find(|v| v.name == name).unwrap().id;
        assert_eq!(views.iter().filter(|v| v.name == "X").count(), 1);
        assert_eq!(id_of("X"), target_x);
        assert_eq!(parent_of("X"), Some(b));
        assert_eq!(parent_of("X (2)"), None);
        assert_eq!(parent_of("Y"), Some(id_of("X (2)")));
        assert_eq!(parent_of("Z"), Some(id_of("Y")));
    }

    #[cfg(feature = "postgres")]
    #[tokio::test]
    #[ignore = "requires TEST_DATABASE_URL"]
    async fn postgres_views_seed_system_rows_and_round_trip() {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("TEST_DATABASE_URL must be set to run the PostgreSQL tests");
        let pool = crate::infrastructure::database::init_postgres_database(&url)
            .await
            .unwrap();

        let target = PipelineViewRepository::new(DatabasePool::Postgres(pool));
        exercise(&target).await;

        let dir = tempfile::tempdir().unwrap();
        let source = init_database(dir.path().join("source.db")).await.unwrap();
        import_merges_or_replaces(
            &PipelineViewRepository::new(DatabasePool::Sqlite(source)),
            &target,
        )
        .await;
    }
}
