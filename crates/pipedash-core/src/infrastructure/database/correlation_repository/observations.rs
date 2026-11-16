use std::collections::HashMap;

use super::{
    ArtifactObservationRow,
    CorrelationConn,
    SourceObservationRow,
    SubjectIdentityRow,
    TargetObservationRow,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::bind::BindValue;

impl CorrelationConn {
    pub async fn replace_source_observations(
        &mut self, subject_id: &str, run_number: i64, rows: &[SourceObservationRow],
    ) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/insert_source_observation_sqlite.sql"),
            include_str!("../../../../sql/correlation/insert_source_observation_postgres.sql"),
        );
        let mut scope = self.scope().await?;
        scope
            .delete_where(
                "source_observations",
                &["subject_id", "run_number"],
                &[BindValue::Text(subject_id), BindValue::Int(run_number)],
            )
            .await?;
        for (ordinal, row) in (0..).zip(rows) {
            let values = [
                BindValue::Int(row.provider_id),
                BindValue::Text(subject_id),
                BindValue::Int(run_number),
                BindValue::Int(ordinal),
                BindValue::Text(&row.repo),
                BindValue::Text(&row.path_scopes_json),
                BindValue::OptText(row.commit_sha.as_deref()),
            ];
            scope.execute(sql, &values).await?;
        }
        scope.finish().await
    }

    pub async fn delete_source_observations_by_subject(
        &mut self, subject_id: &str,
    ) -> DomainResult<u64> {
        let values = [BindValue::Text(subject_id)];
        self.delete_where("source_observations", &["subject_id"], &values)
            .await
    }

    pub async fn list_source_observations(&mut self) -> DomainResult<Vec<SourceObservationRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_source_observations_by_subject(
        &mut self, subject_id: &str,
    ) -> DomainResult<Vec<SourceObservationRow>> {
        self.list(&["subject_id"], &[BindValue::Text(subject_id)])
            .await
    }

    pub async fn list_source_observations_by_provider(
        &mut self, provider_id: i64,
    ) -> DomainResult<Vec<SourceObservationRow>> {
        self.list(&["provider_id"], &[BindValue::Int(provider_id)])
            .await
    }

    pub async fn list_source_observations_by_repo(
        &mut self, repo: &str,
    ) -> DomainResult<Vec<SourceObservationRow>> {
        self.list(&["repo"], &[BindValue::Text(repo)]).await
    }

    pub async fn list_source_observations_by_repo_commit(
        &mut self, repo: &str, commit_sha: &str,
    ) -> DomainResult<Vec<SourceObservationRow>> {
        let values = [BindValue::Text(repo), BindValue::Text(commit_sha)];
        self.list(&["repo", "commit_sha"], &values).await
    }

    pub async fn replace_target_observations(
        &mut self, subject_id: &str, run_number: i64, rows: &[TargetObservationRow],
    ) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/insert_target_observation_sqlite.sql"),
            include_str!("../../../../sql/correlation/insert_target_observation_postgres.sql"),
        );
        let mut scope = self.scope().await?;
        scope
            .delete_where(
                "target_observations",
                &["subject_id", "run_number"],
                &[BindValue::Text(subject_id), BindValue::Int(run_number)],
            )
            .await?;
        for (ordinal, row) in (0..).zip(rows) {
            let values = [
                BindValue::Int(row.provider_id),
                BindValue::Text(subject_id),
                BindValue::Int(run_number),
                BindValue::Int(ordinal),
                BindValue::Text(&row.cluster),
                BindValue::OptText(row.namespace.as_deref()),
                BindValue::Text(&row.name),
                BindValue::Text(&row.lifetime),
            ];
            scope.execute(sql, &values).await?;
        }
        scope.finish().await
    }

    pub async fn delete_target_observations_by_subject(
        &mut self, subject_id: &str,
    ) -> DomainResult<u64> {
        let values = [BindValue::Text(subject_id)];
        self.delete_where("target_observations", &["subject_id"], &values)
            .await
    }

    pub async fn list_target_observations(&mut self) -> DomainResult<Vec<TargetObservationRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_target_observations_by_subject(
        &mut self, subject_id: &str,
    ) -> DomainResult<Vec<TargetObservationRow>> {
        self.list(&["subject_id"], &[BindValue::Text(subject_id)])
            .await
    }

    pub async fn list_target_observations_by_provider(
        &mut self, provider_id: i64,
    ) -> DomainResult<Vec<TargetObservationRow>> {
        self.list(&["provider_id"], &[BindValue::Int(provider_id)])
            .await
    }

    pub async fn list_target_observations_by_cluster(
        &mut self, cluster: &str,
    ) -> DomainResult<Vec<TargetObservationRow>> {
        self.list(&["cluster"], &[BindValue::Text(cluster)]).await
    }

    pub async fn replace_artifact_observations(
        &mut self, subject_id: &str, run_number: i64, origin: &str, rows: &[ArtifactObservationRow],
    ) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/insert_artifact_observation_sqlite.sql"),
            include_str!("../../../../sql/correlation/insert_artifact_observation_postgres.sql"),
        );
        let mut scope = self.scope().await?;
        scope
            .delete_where(
                "artifact_observations",
                &["subject_id", "run_number", "origin"],
                &[
                    BindValue::Text(subject_id),
                    BindValue::Int(run_number),
                    BindValue::Text(origin),
                ],
            )
            .await?;
        let next_sql = format!(
            "SELECT relation, MAX(ordinal) FROM artifact_observations \
             WHERE subject_id = {} AND run_number = {} GROUP BY relation",
            scope.placeholder(1),
            scope.placeholder(2)
        );
        let kept: Vec<(String, i64)> = scope
            .fetch(
                &next_sql,
                &[BindValue::Text(subject_id), BindValue::Int(run_number)],
            )
            .await?;
        let mut next: HashMap<String, i64> = kept
            .into_iter()
            .map(|(relation, max)| (relation, max + 1))
            .collect();
        for row in rows {
            let slot = next.entry(row.relation.clone()).or_insert(0);
            let ordinal = *slot;
            *slot += 1;
            let values = [
                BindValue::Int(row.provider_id),
                BindValue::Text(subject_id),
                BindValue::Int(run_number),
                BindValue::Text(&row.relation),
                BindValue::Int(ordinal),
                BindValue::Text(&row.repository),
                BindValue::OptText(row.digest.as_deref()),
                BindValue::OptText(row.tag.as_deref()),
                BindValue::OptInt(row.target_ordinal),
                BindValue::Text(origin),
            ];
            scope.execute(sql, &values).await?;
        }
        scope.finish().await
    }

    pub async fn delete_artifact_observations_by_subject(
        &mut self, subject_id: &str,
    ) -> DomainResult<u64> {
        let values = [BindValue::Text(subject_id)];
        self.delete_where("artifact_observations", &["subject_id"], &values)
            .await
    }

    pub async fn list_artifact_observations(
        &mut self,
    ) -> DomainResult<Vec<ArtifactObservationRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_artifact_observations_by_subject(
        &mut self, subject_id: &str,
    ) -> DomainResult<Vec<ArtifactObservationRow>> {
        self.list(&["subject_id"], &[BindValue::Text(subject_id)])
            .await
    }

    pub async fn list_artifact_observations_by_provider(
        &mut self, provider_id: i64,
    ) -> DomainResult<Vec<ArtifactObservationRow>> {
        self.list(&["provider_id"], &[BindValue::Int(provider_id)])
            .await
    }

    pub async fn list_artifact_observations_by_digest(
        &mut self, digest: &str,
    ) -> DomainResult<Vec<ArtifactObservationRow>> {
        self.list(&["digest"], &[BindValue::Text(digest)]).await
    }

    pub async fn list_artifact_observations_by_repository(
        &mut self, repository: &str,
    ) -> DomainResult<Vec<ArtifactObservationRow>> {
        self.list(&["repository"], &[BindValue::Text(repository)])
            .await
    }

    pub async fn upsert_subject_identity(&mut self, row: &SubjectIdentityRow) -> DomainResult<()> {
        let sql = self.pick(
            include_str!("../../../../sql/correlation/upsert_subject_identity_sqlite.sql"),
            include_str!("../../../../sql/correlation/upsert_subject_identity_postgres.sql"),
        );
        let values = [
            BindValue::Text(&row.subject_id),
            BindValue::Int(row.provider_id),
            BindValue::OptText(row.unit_id.as_deref()),
            BindValue::OptText(row.unit_parent.as_deref()),
            BindValue::Text(&row.unit_depends_on_json),
            BindValue::OptText(row.app_ref_authority.as_deref()),
            BindValue::OptText(row.app_ref_scope.as_deref()),
            BindValue::OptText(row.app_ref_id.as_deref()),
            BindValue::OptText(row.app_label.as_deref()),
        ];
        self.execute(sql, &values).await?;
        Ok(())
    }

    pub async fn delete_subject_identity(&mut self, subject_id: &str) -> DomainResult<bool> {
        let values = [BindValue::Text(subject_id)];
        let removed = self
            .delete_where("subject_identities", &["subject_id"], &values)
            .await?;
        Ok(removed > 0)
    }

    pub async fn get_subject_identity(
        &mut self, subject_id: &str,
    ) -> DomainResult<Option<SubjectIdentityRow>> {
        self.get(&["subject_id"], &[BindValue::Text(subject_id)])
            .await
    }

    pub async fn list_subject_identities(&mut self) -> DomainResult<Vec<SubjectIdentityRow>> {
        self.list(&[], &[]).await
    }

    pub async fn list_subject_identities_by_provider(
        &mut self, provider_id: i64,
    ) -> DomainResult<Vec<SubjectIdentityRow>> {
        self.list(&["provider_id"], &[BindValue::Int(provider_id)])
            .await
    }
}
