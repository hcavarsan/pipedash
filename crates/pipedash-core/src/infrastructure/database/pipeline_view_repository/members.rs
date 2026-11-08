use super::rows::to_json;
use super::PipelineViewRepository;
use crate::domain::{
    DomainResult,
    ViewRules,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::DatabasePool;

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

impl PipelineViewRepository {
    pub async fn upsert_member(
        &self, view_id: i64, provider_id: i64, pipeline_id: &str, excluded: bool,
    ) -> DomainResult<()> {
        let sql = format!(
            "INSERT INTO pipeline_view_members (view_id, provider_id, pipeline_id, position, excluded)
             VALUES ({}, {}, {}, (SELECT COALESCE(MAX(position), -1) + 1 FROM pipeline_view_members WHERE view_id = {}), {})
             ON CONFLICT (view_id, pipeline_id) DO UPDATE SET excluded = excluded.excluded",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.placeholder(3),
            self.pool.placeholder(4),
            self.pool.placeholder(5),
        );
        on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(view_id)
            .bind(provider_id)
            .bind(pipeline_id)
            .bind(view_id)
            .bind(excluded)
            .execute(p)
            .await
            .map(|_| ()))
        .map_err(db_err)
    }

    pub async fn delete_member(&self, view_id: i64, pipeline_id: &str) -> DomainResult<bool> {
        let sql = format!(
            "DELETE FROM pipeline_view_members WHERE view_id = {} AND pipeline_id = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2)
        );
        let affected = on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(view_id)
            .bind(pipeline_id)
            .execute(p)
            .await
            .map(|r| r.rows_affected()))
        .map_err(db_err)?;
        Ok(affected > 0)
    }

    pub async fn replace_manual_members(
        &self, view_id: i64, members: &[(i64, String)], excluded: &[(i64, String)],
    ) -> DomainResult<()> {
        let clear = format!(
            "DELETE FROM pipeline_view_members WHERE view_id = {} AND excluded = FALSE",
            self.pool.placeholder(1)
        );
        let insert = |flag: &str| {
            format!(
                "INSERT INTO pipeline_view_members (view_id, provider_id, pipeline_id, position, excluded)
                 VALUES ({}, {}, {}, {}, {flag})
                 ON CONFLICT (view_id, pipeline_id) DO UPDATE SET provider_id = excluded.provider_id, position = excluded.position, excluded = {flag}",
                self.pool.placeholder(1),
                self.pool.placeholder(2),
                self.pool.placeholder(3),
                self.pool.placeholder(4),
            )
        };
        let rows: Vec<(String, &(i64, String))> = members
            .iter()
            .map(|m| (insert("FALSE"), m))
            .chain(excluded.iter().map(|m| (insert("TRUE"), m)))
            .collect();
        on_pool!(&self.pool, p => async {
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
        .map_err(db_err)
    }

    pub async fn remove_provider(&self, provider_id: i64) -> DomainResult<()> {
        let delete_members = format!(
            "DELETE FROM pipeline_view_members WHERE provider_id = {}",
            self.pool.placeholder(1)
        );
        let update_rules = format!(
            "UPDATE pipeline_views SET rules_json = {}, updated_at = {} WHERE id = {}",
            self.pool.placeholder(1),
            self.pool.datetime_now(),
            self.pool.placeholder(2)
        );
        let lock_views = match &self.pool {
            DatabasePool::Sqlite(_) => None,
            DatabasePool::Postgres(_) => {
                Some("LOCK TABLE pipeline_views IN SHARE ROW EXCLUSIVE MODE")
            }
        };
        on_pool!(&self.pool, p => async {
            let mut tx = p.begin().await.map_err(db_err)?;
            if let Some(lock) = lock_views {
                sqlx::query(lock)
                    .execute(&mut *tx)
                    .await
                    .map_err(db_err)?;
            }
            sqlx::query(sqlx::AssertSqlSafe(delete_members.clone()))
                .bind(provider_id)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
            let views =
                sqlx::query_as::<_, (i64, String)>("SELECT id, rules_json FROM pipeline_views")
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(db_err)?;
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
                    .map_err(db_err)?;
            }
            tx.commit().await.map_err(db_err)
        }
        .await)
    }
}
