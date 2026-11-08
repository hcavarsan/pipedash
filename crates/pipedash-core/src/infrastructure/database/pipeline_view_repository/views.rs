use super::rows::to_json;
use super::PipelineViewRepository;
use crate::domain::{
    DomainResult,
    ViewDisplay,
    ViewRules,
};
use crate::infrastructure::database::pool::db_err;
use crate::infrastructure::database::DatabasePool;

const PRUNE_ORPHAN_PINS: &str = "DELETE FROM pipeline_view_pins WHERE kind = 'view'
    AND ref NOT IN (SELECT CAST(id AS TEXT) FROM pipeline_views WHERE kind = 'custom')";

impl PipelineViewRepository {
    pub async fn insert_custom(
        &self, name: &str, parent: Option<i64>, rules: &ViewRules, display: &ViewDisplay,
    ) -> DomainResult<i64> {
        let sql = format!(
            "INSERT INTO pipeline_views (kind, name, parent_id, position, rules_json, display_json)
             VALUES ('custom', {}, {}, (SELECT COALESCE(MAX(position), -1) + 1 FROM pipeline_views WHERE kind IN ('pinned', 'custom')), {}, {})
             RETURNING id",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.placeholder(3),
            self.pool.placeholder(4),
        );
        let rules = to_json(rules)?;
        let display = to_json(display)?;
        on_pool!(&self.pool, p => sqlx::query_scalar::<_, i64>(
            sqlx::AssertSqlSafe(sql)
        )
        .bind(name)
        .bind(parent)
        .bind(&rules)
        .bind(&display)
        .fetch_one(p)
        .await)
        .map_err(db_err)
    }

    pub async fn update_view(
        &self, id: i64, name: &str, parent: Option<i64>, rules: &ViewRules, display: &ViewDisplay,
    ) -> DomainResult<bool> {
        let sql = format!(
            "UPDATE pipeline_views SET name = {}, parent_id = {}, rules_json = {}, display_json = {}, updated_at = {} WHERE id = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2),
            self.pool.placeholder(3),
            self.pool.placeholder(4),
            self.pool.datetime_now(),
            self.pool.placeholder(5),
        );
        let rules = to_json(rules)?;
        let display = to_json(display)?;
        let affected = on_pool!(&self.pool, p => sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(name)
            .bind(parent)
            .bind(&rules)
            .bind(&display)
            .bind(id)
            .execute(p)
            .await
            .map(|r| r.rows_affected()))
        .map_err(db_err)?;
        Ok(affected > 0)
    }

    pub async fn delete_custom(&self, id: i64) -> DomainResult<bool> {
        let parent_of = format!(
            "SELECT parent_id FROM pipeline_views WHERE id = {} AND kind = 'custom'",
            self.pool.placeholder(1)
        );
        let reparent = format!(
            "UPDATE pipeline_views SET parent_id = {} WHERE parent_id = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2)
        );
        let delete_view = format!(
            "DELETE FROM pipeline_views WHERE id = {}",
            self.pool.placeholder(1)
        );
        let delete_members = format!(
            "DELETE FROM pipeline_view_members WHERE view_id = {}",
            self.pool.placeholder(1)
        );
        on_pool!(&self.pool, p => async {
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
        .map_err(db_err)
    }

    pub async fn set_positions(&self, positions: &[(i64, i64)]) -> DomainResult<()> {
        let sql = format!(
            "UPDATE pipeline_views SET position = {} WHERE id = {}",
            self.pool.placeholder(1),
            self.pool.placeholder(2)
        );
        on_pool!(&self.pool, p => async {
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
        .map_err(db_err)
    }
}
