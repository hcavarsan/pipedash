use std::collections::{
    BTreeMap,
    BTreeSet,
    HashMap,
};

use futures::FutureExt;

use crate::application::services::correlation_service::vocab::{
    commit_node_id,
    parse_commit_node,
    target_node_id,
    ARTIFACT,
    BUILT_FROM,
    COMMIT,
    CONFIG_LINEAGE,
    INTRODUCED,
    REL_CONSUMED,
    REL_PRODUCED,
};
use crate::application::services::correlation_service::CorrelationService;
use crate::domain::application::{
    Confidence,
    Stage,
};
use crate::domain::DomainResult;
use crate::infrastructure::database::{
    ApplicationMemberRow,
    ChangeRow,
    ChangeStageRow,
    CorrelationConn,
    RunEvidenceRow,
    PIPELINE_LEVEL,
};

type ChangeKey = (String, String);
type StageKey = (String, String, String, i64);

struct Run {
    pipeline_id: String,
    evidence: RunEvidenceRow,
    sources: Vec<ChangeKey>,
    produced: bool,
    consumed: Vec<(String, Option<usize>)>,
    targets: Vec<String>,
}

struct Member {
    stage: Stage,
    runs: Vec<Run>,
}

#[derive(Default)]
struct Graph {
    built_from: HashMap<String, Vec<ChangeKey>>,
    introduced: HashMap<String, Vec<String>>,
}

impl Run {
    fn groups(&self, groups: &HashMap<String, String>, ordinal: Option<usize>) -> BTreeSet<String> {
        let keys: Vec<&String> = match ordinal {
            Some(index) => self.targets.get(index).into_iter().collect(),
            None => self.targets.iter().collect(),
        };
        keys.into_iter()
            .filter_map(|key| groups.get(key).cloned())
            .collect()
    }

    fn row(&self, stage: Stage, group_key: &str) -> ChangeStageRow {
        ChangeStageRow {
            change_id: 0,
            stage: stage.as_str().to_string(),
            group_key: group_key.to_string(),
            pipeline_id: self.pipeline_id.clone(),
            run_number: self.evidence.run_number,
            started_at: self.evidence.started_at,
            concluded_at: self.evidence.concluded_at,
            status: self.evidence.status.clone(),
            outcome_recorded: self.evidence.outcome_recorded,
            timing_recorded: self.evidence.timing_recorded,
            attempt: self.evidence.attempt,
        }
    }
}

async fn load_member(
    conn: &mut CorrelationConn, row: &ApplicationMemberRow,
) -> DomainResult<Option<Member>> {
    let Some(stage) = Stage::parse(&row.stage) else {
        return Ok(None);
    };
    let id = &row.pipeline_id;
    let evidence = conn.list_run_evidence_by_pipeline(id).await?;
    let sources = conn.list_source_observations_by_subject(id).await?;
    let artifacts = conn.list_artifact_observations_by_subject(id).await?;
    let targets = conn.list_target_observations_by_subject(id).await?;
    let target_keys = |run_number: i64| -> Vec<String> {
        targets
            .iter()
            .filter(|target| target.run_number == run_number)
            .map(|target| target_node_id(&target.cluster, target.namespace.as_deref()))
            .collect()
    };
    let runs = evidence
        .into_iter()
        .map(|evidence| {
            let number = evidence.run_number;
            let own = target_keys(number);
            let artifacts = artifacts.iter().filter(|row| row.run_number == number);
            Run {
                pipeline_id: id.clone(),
                sources: sources
                    .iter()
                    .filter(|row| row.run_number == number)
                    .filter_map(|row| Some((row.repo.clone(), row.commit_sha.clone()?)))
                    .collect(),
                produced: artifacts.clone().any(|row| row.relation == REL_PRODUCED),
                consumed: artifacts
                    .filter(|row| row.relation == REL_CONSUMED)
                    .filter_map(|row| {
                        let ordinal = row.target_ordinal.and_then(|o| usize::try_from(o).ok());
                        Some((row.digest.clone()?, ordinal))
                    })
                    .collect(),
                targets: if own.is_empty() {
                    target_keys(PIPELINE_LEVEL)
                } else {
                    own
                },
                evidence,
            }
        })
        .collect();
    Ok(Some(Member { stage, runs }))
}

async fn load_graph(conn: &mut CorrelationConn, members: &[Member]) -> DomainResult<Graph> {
    let exact = Confidence::Exact.as_str();
    let mut graph = Graph::default();
    let mut digests: BTreeSet<String> = BTreeSet::new();
    for member in members
        .iter()
        .filter(|member| member.stage == Stage::Deploy)
    {
        for run in &member.runs {
            digests.extend(run.consumed.iter().map(|(digest, _)| digest.clone()));
            for (repo, commit) in &run.sources {
                let node = commit_node_id(repo, commit);
                let found: Vec<String> = conn
                    .list_edges_from(COMMIT, &node)
                    .await?
                    .into_iter()
                    .filter(|edge| {
                        edge.relation == INTRODUCED
                            && edge.rule == CONFIG_LINEAGE
                            && edge.confidence == exact
                            && edge.to_kind == ARTIFACT
                    })
                    .map(|edge| edge.to_id)
                    .collect();
                if !found.is_empty() {
                    digests.extend(found.iter().cloned());
                    graph.introduced.insert(node, found);
                }
            }
        }
    }
    for digest in digests {
        let commits: Vec<ChangeKey> = conn
            .list_edges_from(ARTIFACT, &digest)
            .await?
            .into_iter()
            .filter(|edge| {
                edge.relation == BUILT_FROM && edge.confidence == exact && edge.to_kind == COMMIT
            })
            .filter_map(|edge| {
                let (repo, sha) = parse_commit_node(&edge.to_id)?;
                Some((repo.to_string(), sha.to_string()))
            })
            .collect();
        graph.built_from.insert(digest, commits);
    }
    Ok(graph)
}

type Derived = BTreeMap<ChangeKey, BTreeMap<StageKey, ChangeStageRow>>;

fn put(derived: &mut Derived, key: &ChangeKey, row: ChangeStageRow) {
    let stage_key = (
        row.stage.clone(),
        row.group_key.clone(),
        row.pipeline_id.clone(),
        row.run_number,
    );
    derived
        .entry(key.clone())
        .or_default()
        .insert(stage_key, row);
}

fn derive(members: &[Member], graph: &Graph, groups: &HashMap<String, String>) -> Derived {
    let mut derived = Derived::new();
    for member in members {
        for run in &member.runs {
            if member.stage == Stage::Build || run.produced {
                for key in &run.sources {
                    put(&mut derived, key, run.row(Stage::Build, ""));
                }
            }
        }
    }
    let mut proven: Vec<(ChangeKey, BTreeSet<String>, &Run)> = Vec::new();
    for member in members
        .iter()
        .filter(|member| member.stage == Stage::Deploy)
    {
        for run in &member.runs {
            for (digest, ordinal) in &run.consumed {
                for key in graph.built_from.get(digest).into_iter().flatten() {
                    proven.push((key.clone(), run.groups(groups, *ordinal), run));
                }
            }
            for (repo, commit) in &run.sources {
                let lineage = graph.introduced.get(&commit_node_id(repo, commit));
                for digest in lineage.into_iter().flatten() {
                    for key in graph.built_from.get(digest).into_iter().flatten() {
                        proven.push((key.clone(), run.groups(groups, None), run));
                    }
                }
            }
        }
    }
    for (key, run_groups, run) in proven {
        for group in run_groups {
            put(&mut derived, &key, run.row(Stage::Deploy, &group));
        }
    }
    for member in members
        .iter()
        .filter(|member| member.stage == Stage::Deploy)
    {
        for run in &member.runs {
            let known: Vec<&ChangeKey> = run
                .sources
                .iter()
                .filter(|key| derived.contains_key(*key))
                .collect();
            for key in known {
                for group in run.groups(groups, None) {
                    put(&mut derived, key, run.row(Stage::Deploy, &group));
                }
            }
        }
    }
    derived
}

async fn existing_rows(
    conn: &mut CorrelationConn, application_id: i64,
) -> DomainResult<BTreeMap<ChangeKey, Vec<ChangeStageRow>>> {
    let mut keys: HashMap<i64, ChangeKey> = HashMap::new();
    let mut rows: BTreeMap<ChangeKey, Vec<ChangeStageRow>> = BTreeMap::new();
    for change in conn.list_changes_by_application(application_id).await? {
        let key = (change.repo, change.commit_sha);
        rows.insert(key.clone(), Vec::new());
        keys.insert(change.id, key);
    }
    for stage in conn
        .list_change_stages_by_application(application_id)
        .await?
    {
        if let Some(list) = keys.get(&stage.change_id).and_then(|key| rows.get_mut(key)) {
            list.push(ChangeStageRow {
                change_id: 0,
                ..stage
            });
        }
    }
    Ok(rows)
}

impl CorrelationService {
    pub(in crate::application::services::correlation_service) async fn rebuild_changes_impl(
        &self, application_id: i64,
    ) -> DomainResult<bool> {
        let mut conn = self.store.conn();
        let Some(application) = conn.get_application(application_id).await? else {
            return Ok(false);
        };
        if application.archived_at.is_some() {
            return Ok(false);
        }
        let mut members = Vec::new();
        for row in conn
            .list_application_members_by_application(application_id)
            .await?
        {
            members.extend(load_member(&mut conn, &row).await?);
        }
        let graph = load_graph(&mut conn, &members).await?;
        let groups: HashMap<String, String> = conn
            .list_application_targets_by_application(application_id)
            .await?
            .into_iter()
            .map(|target| (target.target_key, target.group_key))
            .collect();
        let derived = derive(&members, &graph, &groups);
        let wanted: BTreeMap<ChangeKey, Vec<ChangeStageRow>> = derived
            .iter()
            .map(|(key, stages)| (key.clone(), stages.values().cloned().collect()))
            .collect();
        if wanted == existing_rows(&mut conn, application_id).await? {
            return Ok(false);
        }
        self.store
            .transaction(move |tx| {
                async move {
                    let mut keep = BTreeSet::new();
                    for ((repo, commit_sha), stages) in derived {
                        let change = ChangeRow {
                            id: 0,
                            application_id,
                            repo,
                            commit_sha,
                            authored_at: None,
                        };
                        let id = tx.upsert_change(&change).await?;
                        let rows: Vec<ChangeStageRow> = stages.into_values().collect();
                        tx.replace_change_stages(id, &rows).await?;
                        keep.insert(id);
                    }
                    for stale in tx.list_changes_by_application(application_id).await? {
                        if !keep.contains(&stale.id) {
                            tx.delete_change(stale.id).await?;
                        }
                    }
                    Ok(true)
                }
                .boxed()
            })
            .await
    }
}
