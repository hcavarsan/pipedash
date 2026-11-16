use chrono::{
    DateTime,
    Utc,
};
use sqlx::postgres::PgRow;
use sqlx::sqlite::SqliteRow;
use sqlx::FromRow;

pub const PIPELINE_LEVEL: i64 = -1;

pub(super) trait Fetch:
    for<'r> FromRow<'r, SqliteRow> + for<'r> FromRow<'r, PgRow> + Send + Unpin
{
}

impl<R> Fetch for R where
    R: for<'r> FromRow<'r, SqliteRow> + for<'r> FromRow<'r, PgRow> + Send + Unpin
{
}

pub(super) trait TableRow: Fetch {
    const TABLE: &'static str;
    const COLUMNS: &'static str;
    const ORDER: &'static str;
}

macro_rules! table_row {
    ($row:ident, $table:literal, $columns:literal, $order:literal) => {
        impl TableRow for $row {
            const TABLE: &'static str = $table;
            const COLUMNS: &'static str = $columns;
            const ORDER: &'static str = $order;
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct SourceObservationRow {
    pub provider_id: i64,
    pub subject_id: String,
    pub run_number: i64,
    pub ordinal: i64,
    pub repo: String,
    pub path_scopes_json: String,
    pub commit_sha: Option<String>,
}

table_row!(
    SourceObservationRow,
    "source_observations",
    "provider_id, subject_id, run_number, ordinal, repo, path_scopes_json, commit_sha",
    "subject_id, run_number, ordinal"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct TargetObservationRow {
    pub provider_id: i64,
    pub subject_id: String,
    pub run_number: i64,
    pub ordinal: i64,
    pub cluster: String,
    pub namespace: Option<String>,
    pub name: String,
    pub lifetime: String,
}

table_row!(
    TargetObservationRow,
    "target_observations",
    "provider_id, subject_id, run_number, ordinal, cluster, namespace, name, lifetime",
    "subject_id, run_number, ordinal"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ArtifactObservationRow {
    pub provider_id: i64,
    pub subject_id: String,
    pub run_number: i64,
    pub relation: String,
    pub ordinal: i64,
    pub repository: String,
    pub digest: Option<String>,
    pub tag: Option<String>,
    pub target_ordinal: Option<i64>,
    pub origin: String,
}

table_row!(
    ArtifactObservationRow,
    "artifact_observations",
    "provider_id, subject_id, run_number, relation, ordinal, repository, digest, tag, \
     target_ordinal, origin",
    "subject_id, run_number, relation, ordinal"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct SubjectIdentityRow {
    pub subject_id: String,
    pub provider_id: i64,
    pub unit_id: Option<String>,
    pub unit_parent: Option<String>,
    pub unit_depends_on_json: String,
    pub app_ref_authority: Option<String>,
    pub app_ref_scope: Option<String>,
    pub app_ref_id: Option<String>,
    pub app_label: Option<String>,
}

table_row!(
    SubjectIdentityRow,
    "subject_identities",
    "subject_id, provider_id, unit_id, unit_parent, unit_depends_on_json, app_ref_authority, \
     app_ref_scope, app_ref_id, app_label",
    "subject_id"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct RunEvidenceRow {
    pub pipeline_id: String,
    pub run_number: i64,
    pub outcome_recorded: bool,
    pub timing_recorded: bool,
    pub attempt: bool,
    pub started_at: DateTime<Utc>,
    pub concluded_at: Option<DateTime<Utc>>,
    pub status: String,
}

table_row!(
    RunEvidenceRow,
    "run_evidence",
    "pipeline_id, run_number, outcome_recorded, timing_recorded, attempt, started_at, \
     concluded_at, status",
    "pipeline_id, run_number"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct CorrelationEdgeRow {
    pub id: i64,
    pub from_kind: String,
    pub from_id: String,
    pub to_kind: String,
    pub to_id: String,
    pub relation: String,
    pub confidence: String,
    pub rule: String,
    pub evidence_json: String,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

table_row!(
    CorrelationEdgeRow,
    "correlation_edges",
    "id, from_kind, from_id, to_kind, to_id, relation, confidence, rule, evidence_json, \
     first_seen, last_seen",
    "id"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ApplicationRow {
    pub id: i64,
    pub identity_kind: String,
    pub identity_key: String,
    pub name: String,
    pub hidden: bool,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

table_row!(
    ApplicationRow,
    "applications",
    "id, identity_kind, identity_key, name, hidden, archived_at, created_at",
    "id"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ApplicationMemberRow {
    pub application_id: i64,
    pub pipeline_id: String,
    pub stage: String,
    pub source: String,
    pub edge_id: Option<i64>,
}

table_row!(
    ApplicationMemberRow,
    "application_members",
    "application_id, pipeline_id, stage, source, edge_id",
    "application_id, pipeline_id"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ApplicationOverrideRow {
    pub id: i64,
    pub application_id: Option<i64>,
    pub kind: String,
    pub subject: Option<String>,
    pub target: Option<String>,
    pub value_json: String,
    pub author: Option<String>,
    pub created_at: DateTime<Utc>,
}

table_row!(
    ApplicationOverrideRow,
    "application_overrides",
    "id, application_id, kind, subject, target, value_json, author, created_at",
    "id"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ApplicationTargetRow {
    pub application_id: i64,
    pub target_key: String,
    pub name: String,
    pub group_key: String,
    pub lifetime: String,
}

table_row!(
    ApplicationTargetRow,
    "application_targets",
    "application_id, target_key, name, group_key, lifetime",
    "application_id, target_key"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct TargetDirectionRow {
    pub application_id: i64,
    pub from_group: String,
    pub to_group: String,
    pub source: String,
}

table_row!(
    TargetDirectionRow,
    "target_directions",
    "application_id, from_group, to_group, source",
    "application_id, from_group, to_group"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ArtifactRow {
    pub digest: String,
    pub repository: String,
    pub first_seen: DateTime<Utc>,
    pub label_source: Option<String>,
    pub label_revision: Option<String>,
    pub registry_checked_at: Option<DateTime<Utc>>,
    pub log_scanned_at: Option<DateTime<Utc>>,
}

table_row!(
    ArtifactRow,
    "artifacts",
    "digest, repository, first_seen, label_source, label_revision, registry_checked_at, \
     log_scanned_at",
    "digest"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ChangeRow {
    pub id: i64,
    pub application_id: i64,
    pub repo: String,
    pub commit_sha: String,
    pub authored_at: Option<DateTime<Utc>>,
}

table_row!(
    ChangeRow,
    "changes",
    "id, application_id, repo, commit_sha, authored_at",
    "id"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ChangeStageRow {
    pub change_id: i64,
    pub stage: String,
    pub group_key: String,
    pub pipeline_id: String,
    pub run_number: i64,
    pub started_at: DateTime<Utc>,
    pub concluded_at: Option<DateTime<Utc>>,
    pub status: String,
    pub outcome_recorded: bool,
    pub timing_recorded: bool,
    pub attempt: bool,
}

table_row!(
    ChangeStageRow,
    "change_stages",
    "change_id, stage, group_key, pipeline_id, run_number, started_at, concluded_at, status, \
     outcome_recorded, timing_recorded, attempt",
    "change_id, stage, group_key, pipeline_id, run_number"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ObservedTransitionRow {
    pub id: i64,
    pub pipeline_id: String,
    pub provider_id: i64,
    pub at: DateTime<Utc>,
    pub status_before: Option<String>,
    pub status_after: String,
    pub health_before: Option<String>,
    pub health_after: Option<String>,
    pub revision: Option<String>,
    pub digests_json: String,
}

table_row!(
    ObservedTransitionRow,
    "observed_transitions",
    "id, pipeline_id, provider_id, at, status_before, status_after, health_before, \
     health_after, revision, digests_json",
    "at, id"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ObservationWindowRow {
    pub provider_id: i64,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

table_row!(
    ObservationWindowRow,
    "observation_windows",
    "provider_id, started_at, ended_at",
    "provider_id, started_at"
);

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct LogScanRow {
    pub pipeline_id: String,
    pub run_number: i64,
    pub run_hash: String,
    pub scanned_at: DateTime<Utc>,
}

table_row!(
    LogScanRow,
    "log_scans",
    "pipeline_id, run_number, run_hash, scanned_at",
    "pipeline_id, run_number"
);
