mod open;

use std::collections::{
    BTreeMap,
    BTreeSet,
};

use super::applications::{
    self,
    Plan,
};
use super::copy;
use super::overrides::entries::{
    override_entries,
    ConfigClaims,
};
use super::stored::Identity;
use super::targets::group_labels;
use super::vocab::{
    parse_run_node,
    ARTIFACT,
    LIFETIME_SHORT_LIVED,
    UNIT,
};
use super::world::World;
use super::CorrelationService;
use crate::domain::application::dto::Suggestion;
use crate::domain::application::overrides::OverrideEntry;
use crate::domain::application::{
    ApplicationCapabilities,
    IdentityKind,
    MemberSource,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::database::{
    ApplicationMemberRow,
    ApplicationRow,
    ApplicationTargetRow,
    CorrelationConn,
    CorrelationEdgeRow,
    TargetDirectionRow,
};

pub(super) struct OpenSuggestion {
    pub dto: Suggestion,
    pub pipelines: Vec<String>,
    pub applications: Vec<i64>,
}

#[derive(Debug, Clone)]
pub struct MemberState {
    pub row: ApplicationMemberRow,
    pub name: String,
    pub why: String,
}

#[derive(Debug, Clone)]
pub struct GroupState {
    pub key: String,
    pub label: String,
    pub targets: Vec<String>,
    pub short_lived: bool,
}

#[derive(Debug, Clone)]
pub struct ApplicationState {
    pub row: ApplicationRow,
    pub kind: IdentityKind,
    pub capabilities: ApplicationCapabilities,
    pub members: Vec<MemberState>,
    pub targets: Vec<ApplicationTargetRow>,
    pub groups: Vec<GroupState>,
    pub directions: Vec<TargetDirectionRow>,
    pub edges: Vec<CorrelationEdgeRow>,
    pub overrides: Vec<OverrideEntry>,
    pub warnings: Vec<String>,
    pub config_key: Option<String>,
}

struct Loaded<'a> {
    world: &'a World,
    plan: &'a Plan,
    claims: &'a ConfigClaims,
}

async fn edges_of(
    conn: &mut CorrelationConn, world: &World, members: &[ApplicationMemberRow],
) -> DomainResult<Vec<CorrelationEdgeRow>> {
    let mut found: BTreeMap<i64, CorrelationEdgeRow> = BTreeMap::new();
    for member in members {
        let Some(subject) = world.subjects.get(&member.pipeline_id) else {
            continue;
        };
        let Some(unit) = &subject.unit else {
            continue;
        };
        for edge in conn.list_edges_from(UNIT, unit).await? {
            found.insert(edge.id, edge);
        }
        for digest in subject.consumed_digests() {
            for edge in conn.list_edges_to(ARTIFACT, digest).await? {
                found.insert(edge.id, edge);
            }
            for edge in conn.list_edges_from(ARTIFACT, digest).await? {
                found.insert(edge.id, edge);
            }
        }
    }
    Ok(found.into_values().collect())
}

async fn why_member(
    conn: &mut CorrelationConn, world: &World, row: &ApplicationMemberRow,
    targets: &[ApplicationTargetRow], key: &str, units: &BTreeSet<&str>,
) -> DomainResult<String> {
    match MemberSource::parse(&row.source) {
        Some(MemberSource::Override) => return Ok(copy::why_linked_by_user()),
        Some(MemberSource::Config) => return Ok(copy::why_linked_by_config(key)),
        _ => {}
    }
    let label = targets.first().map_or("", |t| t.name.as_str());
    let subject = world.subjects.get(&row.pipeline_id);
    if let Some(unit) = subject.filter(|s| !s.is_build()) {
        return Ok(match (label.is_empty(), unit.consumed.first()) {
            (true, Some(image)) => copy::why_unit_runs(&image.repository),
            _ => copy::why_unit_deploys(label),
        });
    }
    if let Some(edge) = match row.edge_id {
        Some(id) => conn.get_edge(id).await?,
        None => None,
    } {
        let run = parse_run_node(&edge.from_id).map_or(0, |(_, run)| run);
        return Ok(copy::why_run_produced(run, label));
    }
    let shared = subject.and_then(|builder| {
        units
            .iter()
            .filter_map(|u| world.units.get(*u).and_then(|id| world.subjects.get(id)))
            .flat_map(|unit| unit.sources.iter())
            .find(|us| builder.sources.iter().any(|bs| bs.overlaps(us)))
    });
    Ok(match shared {
        Some(source) => copy::why_same_source(&source.repo, source.path()),
        None => copy::why_linked_by_user(),
    })
}

impl CorrelationService {
    async fn state_of(
        &self, conn: &mut CorrelationConn, loaded: &Loaded<'_>, row: &ApplicationRow,
    ) -> DomainResult<ApplicationState> {
        let identity = Identity::of_row(row)
            .ok_or_else(|| DomainError::InternalError(copy::application_not_found(row.id)))?;
        let encoded = identity.encode();
        let world = loaded.world;
        let member_rows = world.members.get(&row.id).cloned().unwrap_or_default();
        let targets = conn.list_application_targets_by_application(row.id).await?;
        let directions = conn.list_target_directions_by_application(row.id).await?;
        let planned = loaded.plan.apps.get(&encoded);
        let units: BTreeSet<&str> = planned
            .map(|app| app.units.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let mut members = Vec::new();
        for member in &member_rows {
            let why = why_member(conn, world, member, &targets, &identity.key, &units).await?;
            members.push(MemberState {
                row: member.clone(),
                name: world.name_of(&member.pipeline_id),
                why,
            });
        }
        let labels = planned
            .map(|app| group_labels(world, loaded.plan, app, &targets))
            .unwrap_or_default();
        let mut grouped: BTreeMap<String, GroupState> = BTreeMap::new();
        for target in &targets {
            let group = grouped
                .entry(target.group_key.clone())
                .or_insert_with(|| GroupState {
                    key: target.group_key.clone(),
                    label: labels
                        .get(&target.group_key)
                        .cloned()
                        .unwrap_or_else(|| target.name.clone()),
                    targets: Vec::new(),
                    short_lived: target.lifetime == LIFETIME_SHORT_LIVED,
                });
            group.targets.push(target.target_key.clone());
        }
        Ok(ApplicationState {
            kind: identity.kind,
            capabilities: ApplicationCapabilities::for_identity(identity.kind),
            edges: edges_of(conn, world, &member_rows).await?,
            overrides: override_entries(world, loaded.plan, loaded.claims, row),
            warnings: loaded
                .plan
                .warnings
                .get(&encoded)
                .cloned()
                .unwrap_or_default(),
            config_key: (identity.kind == IdentityKind::Config).then(|| identity.key.clone()),
            groups: grouped.into_values().collect(),
            row: row.clone(),
            members,
            targets,
            directions,
        })
    }

    pub(super) async fn states_in(
        &self, conn: &mut CorrelationConn, world: &World, only: Option<i64>,
    ) -> DomainResult<Vec<ApplicationState>> {
        let plan = applications::plan(world);
        let claims = ConfigClaims::of(world);
        let loaded = Loaded {
            world,
            plan: &plan,
            claims: &claims,
        };
        let rows: Vec<&ApplicationRow> = match only {
            Some(id) => vec![world
                .applications
                .values()
                .find(|row| row.id == id)
                .ok_or_else(|| DomainError::NotFound(copy::application_not_found(id)))?],
            None => world
                .applications
                .values()
                .filter(|row| row.archived_at.is_none())
                .collect(),
        };
        let mut states = Vec::new();
        for row in rows {
            states.push(self.state_of(conn, &loaded, row).await?);
        }
        Ok(states)
    }

    pub async fn application_states(&self) -> DomainResult<Vec<ApplicationState>> {
        let mut conn = self.store.conn();
        let world = self.load_world(&mut conn).await?;
        self.states_in(&mut conn, &world, None).await
    }

    pub async fn application_state(&self, id: i64) -> DomainResult<ApplicationState> {
        let mut conn = self.store.conn();
        let world = self.load_world(&mut conn).await?;
        let mut states = self.states_in(&mut conn, &world, Some(id)).await?;
        states
            .pop()
            .ok_or_else(|| DomainError::NotFound(copy::application_not_found(id)))
    }
}
