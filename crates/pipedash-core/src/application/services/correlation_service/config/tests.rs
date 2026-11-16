use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use pipedash_plugin_api::correlation::ids::{
    ClusterId,
    UnitId,
};
use pipedash_plugin_api::correlation::{
    Correlation,
    TargetLifetime,
    TargetObservation,
};

use super::super::tests::settle;
use super::super::CorrelationService;
use crate::application::services::pipeline_service::observer::FetchObserver;
use crate::application::test_support::{
    pipeline,
    Gate,
    Harness,
};
use crate::application::ConfigWatcher;
use crate::domain::application::dto::{
    ApplicationListQuery,
    ApplicationSummary,
};
use crate::domain::application::overrides::{
    ApplicationEdit,
    CreateApplicationInput,
    CreateApplicationMember,
    OverrideKind,
};
use crate::domain::application::IdentityKind;
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineStatus,
};
use crate::infrastructure::config::sync::SyncResult;
use crate::infrastructure::database::{
    ApplicationMemberRow,
    ApplicationOverrideRow,
    ApplicationRow,
    ApplicationTargetRow,
    TargetDirectionRow,
};
use crate::infrastructure::TokenStore;

const PROVIDERS: &str =
    "[providers.ci]\nname = \"CI\"\ntype = \"stub\"\ntoken = \"t\"\nrefresh_interval = 30\n\n";
const SHOP: &str = "[applications.shop]\nname = \"Shop\"\n\n[[applications.shop.members]]\nprovider = \"ci\"\npipeline = \"shop-*\"\n\n[applications.shop.targets.qa]\nname = \"QA\"\nnamespaces = [\"shop-api\"]\n\n[applications.shop.targets.prod]\nname = \"Prod\"\nnamespaces = [\"shop-prod\"]\nafter = [\"qa\"]\n";
const CD: &str =
    "[providers.cd]\nname = \"CD\"\ntype = \"stub\"\ntoken = \"t\"\nrefresh_interval = 30\n\n";
const RELEASE: &str =
    "\n[views.release]\nname = \"Release\"\nstatuses = [\"failed\"]\nsort = \"status\"\n";
const OTHER: &str = "\n[applications.other]\nname = \"Other\"\n\n[[applications.other.members]]\nprovider = \"ci\"\npipeline = \"other-*\"\n";

async fn list_named(h: &Harness, name: &str) -> ApplicationSummary {
    settle(service(h)).await;
    service(h)
        .list(ApplicationListQuery {
            include_hidden: true,
            ..ApplicationListQuery::default()
        })
        .await
        .unwrap()
        .applications
        .into_iter()
        .find(|a| a.name == name)
        .unwrap()
}

fn service(h: &Harness) -> &Arc<CorrelationService> {
    &h.correlation_service
}

fn write(h: &Harness, rest: &str) {
    std::fs::write(h.config_state.config_path(), format!("{PROVIDERS}{rest}")).unwrap();
}

async fn apply(h: &Harness) -> DomainResult<SyncResult> {
    h.provider_service.apply_config_file(&h.config_state).await
}

async fn setup() -> (Harness, i64) {
    let h = Harness::new().await;
    write(&h, "");
    apply(&h).await.unwrap();
    let id = h
        .repository
        .config_backend()
        .list_providers()
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.name == "ci")
        .and_then(|p| p.id)
        .unwrap();
    (h, id)
}

fn unit_pipeline(provider_id: i64, name: &str) -> Pipeline {
    let mut p = pipeline(provider_id, name, PipelineStatus::Success);
    p.correlation = Correlation {
        unit: UnitId::new(name),
        targets: vec![TargetObservation {
            cluster: ClusterId::from_host("cluster.test").unwrap(),
            namespace: Some(name.to_string()),
            name: name.to_string(),
            lifetime: TargetLifetime::Standing,
        }],
        ..Correlation::default()
    };
    p
}

async fn feed_pipelines(h: &Harness, provider_id: i64, pipelines: &[Pipeline]) {
    h.repository
        .update_pipelines_cache(provider_id, pipelines)
        .await
        .unwrap();
    service(h)
        .pipelines_applied(provider_id, &[], pipelines)
        .await;
    service(h).flush_events().await;
}

async fn feed(h: &Harness, provider_id: i64, names: &[&str]) {
    let pipelines: Vec<Pipeline> = names
        .iter()
        .map(|name| unit_pipeline(provider_id, name))
        .collect();
    feed_pipelines(h, provider_id, &pipelines).await;
}

fn id(provider_id: i64, name: &str) -> String {
    format!("stub__{provider_id}__{name}")
}

#[derive(Debug, PartialEq)]
struct Snapshot {
    applications: Vec<ApplicationRow>,
    members: Vec<ApplicationMemberRow>,
    targets: Vec<ApplicationTargetRow>,
    directions: Vec<TargetDirectionRow>,
    overrides: Vec<ApplicationOverrideRow>,
}

async fn snapshot(h: &Harness) -> Snapshot {
    let mut conn = service(h).store.conn();
    Snapshot {
        applications: conn.list_applications().await.unwrap(),
        members: conn.list_application_members().await.unwrap(),
        targets: conn.list_application_targets().await.unwrap(),
        directions: conn.list_target_directions().await.unwrap(),
        overrides: conn.list_overrides().await.unwrap(),
    }
}

fn config_apps(snapshot: &Snapshot) -> Vec<&ApplicationRow> {
    snapshot
        .applications
        .iter()
        .filter(|a| a.identity_kind == "config")
        .collect()
}

fn unit_apps(snapshot: &Snapshot) -> Vec<&ApplicationRow> {
    snapshot
        .applications
        .iter()
        .filter(|a| a.identity_kind == "unit" && a.archived_at.is_none())
        .collect()
}

fn config_id(snapshot: &Snapshot, key: &str) -> Option<i64> {
    snapshot
        .applications
        .iter()
        .find(|a| a.identity_kind == "config" && a.identity_key == key)
        .map(|a| a.id)
}

fn app_targets<'a>(snapshot: &'a Snapshot, key: &str) -> Vec<&'a ApplicationTargetRow> {
    let id = config_id(snapshot, key);
    snapshot
        .targets
        .iter()
        .filter(|t| Some(t.application_id) == id)
        .collect()
}

fn app_directions<'a>(snapshot: &'a Snapshot, key: &str) -> Vec<&'a TargetDirectionRow> {
    let id = config_id(snapshot, key);
    snapshot
        .directions
        .iter()
        .filter(|d| Some(d.application_id) == id)
        .collect()
}

fn members_of<'a>(snapshot: &'a Snapshot, key: &str) -> Vec<&'a ApplicationMemberRow> {
    let Some(app) = snapshot
        .applications
        .iter()
        .find(|a| a.identity_kind == "config" && a.identity_key == key)
    else {
        return Vec::new();
    };
    snapshot
        .members
        .iter()
        .filter(|m| m.application_id == app.id)
        .collect()
}

async fn banner(h: &Harness) -> Option<String> {
    service(h).config_error().await.map(|e| e.message)
}

#[tokio::test]
async fn config_table_creates_a_read_only_application_without_an_automatic_one() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    assert_eq!(unit_apps(&snapshot(&h).await).len(), 1);

    write(&h, SHOP);
    apply(&h).await.unwrap();

    let snap = snapshot(&h).await;
    let members = members_of(&snap, "shop");
    assert_eq!(config_apps(&snap).len(), 1);
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].pipeline_id, id(ci, "shop-api"));
    assert_eq!(members[0].source, "config");
    assert!(unit_apps(&snap).is_empty());
    assert!(
        !crate::domain::application::ApplicationCapabilities::for_identity(
            crate::domain::application::IdentityKind::Config
        )
        .edit
    );
}

#[tokio::test]
async fn glob_member_picks_up_a_pipeline_that_appears_after_the_file_loaded() {
    let (h, ci) = setup().await;
    write(&h, SHOP);
    apply(&h).await.unwrap();
    assert!(members_of(&snapshot(&h).await, "shop").is_empty());

    feed(&h, ci, &["shop-api"]).await;

    let snap = snapshot(&h).await;
    let members = members_of(&snap, "shop");
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].pipeline_id, id(ci, "shop-api"));
    assert!(unit_apps(&snap).is_empty());
}

#[tokio::test]
async fn exclude_removes_a_member_after_combination() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api", "shop-legacy"]).await;

    write(
        &h,
        &format!(
            "{SHOP}\n[[applications.shop.exclude]]\nprovider = \"ci\"\npipeline = \"shop-legacy\"\n"
        ),
    );
    apply(&h).await.unwrap();

    let snap = snapshot(&h).await;
    let members = members_of(&snap, "shop");
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].pipeline_id, id(ci, "shop-api"));
    assert_eq!(unit_apps(&snap).len(), 1);
}

#[tokio::test]
async fn cycle_in_after_rejects_the_file() {
    let (h, _) = setup().await;
    write(
        &h,
        "[applications.shop]\nname = \"Shop\"\n\n[[applications.shop.members]]\nprovider = \"ci\"\npipeline = \"shop-*\"\n\n[applications.shop.targets.a]\nname = \"A\"\nafter = [\"b\"]\n\n[applications.shop.targets.b]\nname = \"B\"\nafter = [\"a\"]\n",
    );

    let result = apply(&h).await;

    assert!(matches!(result, Err(DomainError::InvalidConfig(m)) if m.contains("cycle")));
    assert!(config_apps(&snapshot(&h).await).is_empty());
    assert!(h.config_state.get().await.applications.is_empty());
}

#[tokio::test]
async fn one_unit_claimed_by_two_applications_rejects_the_file() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    write(
        &h,
        "[applications.a]\nname = \"A\"\n\n[[applications.a.members]]\nprovider = \"ci\"\npipeline = \"shop-*\"\n\n[applications.b]\nname = \"B\"\n\n[[applications.b.members]]\nprovider = \"ci\"\npipeline = \"shop-api\"\n",
    );

    let result = apply(&h).await;

    assert!(matches!(
        result,
        Err(DomainError::InvalidConfig(m)) if m == "applications.a and applications.b both claim shop-api"
    ));
    assert!(config_apps(&snapshot(&h).await).is_empty());
    assert!(banner(&h).await.is_some());
}

#[tokio::test]
async fn valid_file_change_reaches_the_list_within_one_poll_with_one_event() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    let before = h.bus.count("applications-changed");
    let watcher = ConfigWatcher::new(Arc::clone(&h.provider_service), Arc::clone(&h.config_state))
        .with_poll(Duration::from_millis(50));
    watcher.start().await;

    write(&h, SHOP);
    h.bus
        .wait_for("applications-changed", |events| {
            events
                .iter()
                .filter(|e| e.event_name() == "applications-changed")
                .count()
                > before
        })
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    watcher.stop().await;

    assert_eq!(h.bus.count("applications-changed"), before + 1);
    assert_eq!(members_of(&snapshot(&h).await, "shop").len(), 1);
}

#[tokio::test]
async fn parse_failure_keeps_every_membership_target_direction_and_override() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api", "shop-prod", "other-x"]).await;
    write(&h, SHOP);
    apply(&h).await.unwrap();
    let other = list_named(&h, "other-x").await;
    service(&h)
        .apply_edit(
            other.id,
            ApplicationEdit::Rename {
                name: "Renamed".to_string(),
            },
            None,
        )
        .await
        .unwrap();
    let before = snapshot(&h).await;
    assert!(!before.targets.is_empty());
    assert!(!before.directions.is_empty());
    assert!(!before.overrides.is_empty());

    write(&h, "[applications.shop\nname = \"Shop\"\n");
    let result = apply(&h).await;

    assert!(result.is_err());
    assert_eq!(snapshot(&h).await, before);
    assert!(banner(&h).await.is_some_and(|m| !m.is_empty()));
}

#[tokio::test]
async fn invalid_reload_keeps_everything() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    write(&h, SHOP);
    apply(&h).await.unwrap();
    let before = snapshot(&h).await;
    assert!(banner(&h).await.is_none());

    write(&h, "[applications.x");
    assert!(apply(&h).await.is_err());
    assert_eq!(snapshot(&h).await, before);
    assert!(banner(&h).await.is_some_and(|m| !m.is_empty()));

    write(&h, SHOP);
    apply(&h).await.unwrap();
    assert_eq!(snapshot(&h).await, before);
    assert!(banner(&h).await.is_none());
}

#[tokio::test]
async fn file_with_one_invalid_application_applies_none_of_its_other_tables() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    write(
        &h,
        &format!(
            "{SHOP}\n[applications.broken]\nname = \"Broken\"\n\n[[applications.broken.members]]\nprovider = \"ci\"\npipeline = \"[unclosed\"\n"
        ),
    );

    let result = apply(&h).await;

    assert!(matches!(result, Err(DomainError::InvalidConfig(_))));
    let snap = snapshot(&h).await;
    assert!(config_apps(&snap).is_empty());
    assert_eq!(unit_apps(&snap).len(), 1);
    assert!(h.config_state.get().await.applications.is_empty());
}

#[tokio::test]
async fn valid_file_after_a_rejected_one_applies_in_full_and_clears_the_banner() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api", "shop-prod"]).await;
    write(&h, "[applications.shop\n");
    assert!(apply(&h).await.is_err());
    assert!(banner(&h).await.is_some());

    write(&h, SHOP);
    apply(&h).await.unwrap();

    let snap = snapshot(&h).await;
    assert!(banner(&h).await.is_none());
    assert_eq!(members_of(&snap, "shop").len(), 2);
    let own = app_targets(&snap, "shop");
    assert_eq!(own.len(), 2);
    assert!(own.iter().any(|t| t.name == "QA"));
    assert!(own.iter().any(|t| t.name == "Prod"));
    let directions = app_directions(&snap, "shop");
    assert_eq!(directions.len(), 1);
    assert_eq!(directions[0].source, "config");
}

#[tokio::test]
async fn pipeline_matching_two_applications_is_claimed_by_neither() {
    let (h, ci) = setup().await;
    write(
        &h,
        "[applications.a]\nname = \"A\"\n\n[[applications.a.members]]\nprovider = \"ci\"\npipeline = \"shop-*\"\n\n[applications.b]\nname = \"B\"\n\n[[applications.b.members]]\nprovider = \"ci\"\npipeline = \"*-api\"\n",
    );
    apply(&h).await.unwrap();

    feed(&h, ci, &["shop-api"]).await;

    let snap = snapshot(&h).await;
    assert!(members_of(&snap, "a").is_empty());
    assert!(members_of(&snap, "b").is_empty());
    assert_eq!(unit_apps(&snap).len(), 1);
}

fn rotated(block: &str, interval: u32) -> String {
    block
        .replace("= 30", &format!("= {interval}"))
        .replace("token = \"t\"", "token = \"rotated\"")
}

#[derive(Debug, PartialEq)]
struct Reload {
    rows: serde_json::Value,
    tokens: Vec<Option<String>>,
    generations: Vec<String>,
    remembered: String,
    tables: Snapshot,
    providers_changed: usize,
    views_changed: usize,
}

async fn reload_state(h: &Harness) -> Reload {
    let rows = h
        .repository
        .config_backend()
        .list_providers()
        .await
        .unwrap();
    let mut tokens = Vec::new();
    let mut generations = Vec::new();
    for id in rows.iter().filter_map(|p| p.id) {
        tokens.push(h.repository.stored_token(id).await.unwrap());
        let generation = h.provider_service.generation_of(id).await;
        generations.push(format!("{id}:{generation:?}"));
    }
    let config = h.config_state.get().await;
    Reload {
        rows: serde_json::to_value(&rows).unwrap(),
        tokens,
        generations,
        remembered: format!(
            "{:?}",
            (
                &config.providers,
                &config.views,
                &config.applications,
                &config.correlation
            )
        ),
        tables: snapshot(h).await,
        providers_changed: h.bus.count("providers-changed"),
        views_changed: h.bus.count("pipeline-views-changed"),
    }
}

async fn rejected_banner_event(h: &Harness, announced: usize) {
    settle(service(h)).await;
    assert!(banner(h).await.is_some());
    assert_eq!(h.bus.count("applications-changed"), announced + 1);
}

#[tokio::test]
async fn failure_in_the_middle_of_a_commit_rolls_back_to_the_previous_config() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    write(&h, SHOP);
    apply(&h).await.unwrap();
    let before = reload_state(&h).await;
    let announced = h.bus.count("applications-changed");
    sqlx::query(
        "CREATE TRIGGER fail_other BEFORE INSERT ON applications WHEN NEW.identity_key = 'other' \
         BEGIN SELECT RAISE(ABORT, 'write failed'); END",
    )
    .execute(&h.pool)
    .await
    .unwrap();

    std::fs::write(
        h.config_state.config_path(),
        format!("{}{CD}{SHOP}{RELEASE}{OTHER}", rotated(PROVIDERS, 60)),
    )
    .unwrap();
    let result = apply(&h).await;

    assert!(result.is_err());
    assert_eq!(reload_state(&h).await, before);
    assert_eq!(h.config_state.get().await.applications.len(), 1);
    rejected_banner_event(&h, announced).await;

    write(&h, SHOP);
    let again = apply(&h).await.unwrap();
    settle(service(&h)).await;

    assert!(again.added.is_empty() && again.updated.is_empty());
    assert_eq!(reload_state(&h).await, before);
    assert!(banner(&h).await.is_none());
    assert_eq!(h.bus.count("applications-changed"), announced + 2);
}

#[tokio::test]
async fn provider_failure_after_applications_commit_restores_everything() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    std::fs::write(
        h.config_state.config_path(),
        format!("{PROVIDERS}{CD}{SHOP}"),
    )
    .unwrap();
    apply(&h).await.unwrap();
    let before = reload_state(&h).await;
    let announced = h.bus.count("applications-changed");
    let loaded: Vec<i64> = before
        .rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_i64().unwrap())
        .collect();
    let renamed = SHOP.replace("name = \"Shop\"", "name = \"Shop 2\"");
    std::fs::write(
        h.config_state.config_path(),
        format!(
            "{}{}{renamed}{RELEASE}",
            rotated(PROVIDERS, 60),
            rotated(CD, 90)
        ),
    )
    .unwrap();
    h.stubs
        .fail_build_after(3, DomainError::InternalError("build failed".into()));

    let result = apply(&h).await;

    assert!(result.is_err());
    assert_eq!(reload_state(&h).await, before);
    assert_eq!(config_apps(&snapshot(&h).await)[0].name, "Shop");
    rejected_banner_event(&h, announced).await;

    let retried = apply(&h).await.unwrap();
    settle(service(&h)).await;

    let mut updated = retried.updated.clone();
    updated.sort();
    assert_eq!(updated, ["cd", "ci"]);
    assert!(retried.added.is_empty());
    assert_eq!(config_apps(&snapshot(&h).await)[0].name, "Shop 2");
    assert_eq!(
        h.bus.count("providers-changed"),
        before.providers_changed + 1
    );
    assert_eq!(
        h.bus.count("pipeline-views-changed"),
        before.views_changed + 1
    );
    assert!(banner(&h).await.is_none());
    for id in loaded {
        let generation = format!("{id}:{:?}", h.provider_service.generation_of(id).await);
        assert!(!before.generations.contains(&generation));
    }
}

const CE: &str =
    "[providers.ce]\nname = \"CE\"\ntype = \"stub\"\ntoken = \"t\"\nrefresh_interval = 30\n\n";

async fn reload_baseline() -> (Harness, Reload) {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    std::fs::write(
        h.config_state.config_path(),
        format!("{PROVIDERS}{CD}{SHOP}"),
    )
    .unwrap();
    apply(&h).await.unwrap();
    let before = reload_state(&h).await;

    (h, before)
}

fn write_next(h: &Harness) {
    let renamed = SHOP.replace("name = \"Shop\"", "name = \"Shop 2\"");
    std::fs::write(
        h.config_state.config_path(),
        format!(
            "{}{}{CE}{renamed}{RELEASE}",
            rotated(PROVIDERS, 60),
            rotated(CD, 90)
        ),
    )
    .unwrap();
}

fn reload_in_background(h: &Harness) -> tokio::task::JoinHandle<DomainResult<SyncResult>> {
    let service = Arc::clone(&h.provider_service);
    let config_state = Arc::clone(&h.config_state);
    tokio::spawn(async move { service.apply_config_file(&config_state).await })
}

async fn provider_named(h: &Harness, name: &str) -> Option<i64> {
    let rows = h
        .repository
        .config_backend()
        .list_providers()
        .await
        .unwrap();

    rows.iter().find(|p| p.name == name).and_then(|p| p.id)
}

async fn raw_interval(h: &Harness, name: &str) -> Option<i64> {
    sqlx::query_scalar("SELECT refresh_interval FROM providers WHERE name = ?")
        .bind(name)
        .fetch_optional(&h.pool)
        .await
        .unwrap()
}

async fn wait_for_applications_event(h: &Harness, announced: usize) {
    h.bus
        .wait_for("the reload to finish", |events| {
            events
                .iter()
                .filter(|e| e.event_name() == "applications-changed")
                .count()
                > announced
        })
        .await;
}

#[tokio::test]
async fn readers_see_only_the_last_valid_config_until_the_reload_activates() {
    let (h, before) = reload_baseline().await;
    let ci = provider_named(&h, "ci").await.unwrap();
    let gate = Gate::new();
    h.commit_hook.pause(Arc::clone(&gate));
    write_next(&h);
    let reload = reload_in_background(&h);
    gate.arrived().await;

    assert_eq!(raw_interval(&h, "ci").await, Some(60));
    assert_eq!(raw_interval(&h, "ce").await, Some(30));
    assert_eq!(h.token_store.get_token(ci).await.unwrap(), "rotated");
    assert_eq!(reload_state(&h).await, before);
    assert_eq!(provider_named(&h, "ce").await, None);
    let config = h.repository.get_provider(ci).await.unwrap();
    assert_eq!((config.refresh_interval, config.token.as_str()), (30, "t"));
    let summaries = h.provider_service.list_providers().await.unwrap();
    assert_eq!(summaries.len(), 2);
    assert!(summaries.iter().all(|s| s.refresh_interval == 30));
    let exported = h.provider_service.export_providers_toml().await.unwrap();
    assert!(!exported.contains("providers.ce"));
    assert_eq!(h.config_state.get().await.views.len(), 0);
    assert_eq!(config_apps(&snapshot(&h).await)[0].name, "Shop");

    gate.open();
    let applied = reload.await.unwrap().unwrap();
    settle(service(&h)).await;

    assert_eq!(applied.added, ["ce"]);
    let config = h.repository.get_provider(ci).await.unwrap();
    assert_eq!(
        (config.refresh_interval, config.token.as_str()),
        (60, "rotated")
    );
    assert!(provider_named(&h, "ce").await.is_some());
    assert_eq!(h.provider_service.list_providers().await.unwrap().len(), 3);
    assert_eq!(h.config_state.get().await.views.len(), 1);
    assert_eq!(config_apps(&snapshot(&h).await)[0].name, "Shop 2");
}

#[tokio::test]
async fn a_cancelled_reload_still_undoes_everything_when_the_commit_fails() {
    let (h, before) = reload_baseline().await;
    let announced = h.bus.count("applications-changed");
    let gate = Gate::new();
    h.commit_hook.pause(Arc::clone(&gate));
    h.commit_hook
        .fail_after_pause(DomainError::InternalError("commit failed".into()));
    write_next(&h);
    let reload = reload_in_background(&h);
    gate.arrived().await;
    assert_eq!(raw_interval(&h, "ci").await, Some(60));

    reload.abort();
    assert!(reload.await.unwrap_err().is_cancelled());
    gate.open();
    wait_for_applications_event(&h, announced).await;
    settle(service(&h)).await;

    assert_eq!(raw_interval(&h, "ci").await, Some(30));
    assert_eq!(raw_interval(&h, "ce").await, None);
    assert_eq!(reload_state(&h).await, before);
    assert!(banner(&h).await.is_some());
}

#[tokio::test]
async fn a_cancelled_reload_that_commits_activates_once_with_one_event_of_each_kind() {
    let (h, before) = reload_baseline().await;
    let announced = h.bus.count("applications-changed");
    let gate = Gate::new();
    h.commit_hook.pause(Arc::clone(&gate));
    write_next(&h);
    let reload = reload_in_background(&h);
    gate.arrived().await;

    reload.abort();
    assert!(reload.await.unwrap_err().is_cancelled());
    gate.open();
    wait_for_applications_event(&h, announced).await;
    settle(service(&h)).await;

    assert_eq!(raw_interval(&h, "ci").await, Some(60));
    let after = reload_state(&h).await;
    assert_eq!(after.providers_changed, before.providers_changed + 1);
    assert_eq!(after.views_changed, before.views_changed + 1);
    assert_eq!(h.bus.count("applications-changed"), announced + 1);
    assert_eq!(config_apps(&after.tables)[0].name, "Shop 2");
    assert!(banner(&h).await.is_none());

    let again = apply(&h).await.unwrap();
    settle(service(&h)).await;

    assert!(again.added.is_empty() && again.updated.is_empty());
    assert_eq!(reload_state(&h).await, after);
    assert_eq!(h.bus.count("applications-changed"), announced + 1);
}

#[tokio::test]
async fn a_waiting_provider_update_sees_the_reloaded_settings() {
    let (h, _) = reload_baseline().await;
    let ci = provider_named(&h, "ci").await.unwrap();
    let before = h.provider_service.generation_of(ci).await;
    let gate = Gate::new();
    h.commit_hook.pause(Arc::clone(&gate));
    let renamed = SHOP.replace("name = \"Shop\"", "name = \"Shop 2\"");
    std::fs::write(
        h.config_state.config_path(),
        format!(
            "{}{}{CE}{renamed}",
            rotated(PROVIDERS, 60).replace("name = \"CI\"", "name = \"CI Renamed\""),
            rotated(CD, 90)
        ),
    )
    .unwrap();
    let reload = reload_in_background(&h);
    gate.arrived().await;
    let service = Arc::clone(&h.provider_service);
    let update =
        tokio::spawn(async move { service.update_provider_refresh_interval(ci, 120).await });
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    assert!(!update.is_finished());

    gate.open();
    reload.await.unwrap().unwrap();
    update.await.unwrap().unwrap();

    let config = h.repository.get_provider(ci).await.unwrap();
    assert_eq!(config.refresh_interval, 120);
    assert_eq!(config.display_name(), "CI Renamed");
    assert_eq!(config.token, "rotated");
    assert_eq!(h.stubs.token_of(ci).as_deref(), Some("rotated"));
    assert_ne!(h.provider_service.generation_of(ci).await, before);
    assert!(provider_named(&h, "ce").await.is_some());
    let cd = provider_named(&h, "cd").await.unwrap();
    assert_eq!(
        h.repository
            .get_provider(cd)
            .await
            .unwrap()
            .refresh_interval,
        90
    );
}

async fn still_pending(drain: &mut (impl Future<Output = ()> + Unpin)) {
    tokio::select! {
        biased;
        () = drain => panic!("the drain returned while the commit was paused"),
        () = std::future::ready(()) => {}
    }
}

async fn begin_shutdown_while_paused(h: &Harness, gate: &Arc<Gate>) {
    let mut drain = std::pin::pin!(h.provider_service.drain_reloads());
    still_pending(&mut drain).await;
    let refused = apply(h).await;
    assert!(matches!(
        refused,
        Err(DomainError::ServiceUnavailable { .. })
    ));
    gate.open();
    drain.await;
}

#[tokio::test]
async fn shutdown_waits_past_the_warning_bound_for_a_paused_reload() {
    let (h, before) = reload_baseline().await;
    let announced = h.bus.count("applications-changed");
    let gate = Gate::new();
    h.commit_hook.pause(Arc::clone(&gate));
    write_next(&h);
    let reload = reload_in_background(&h);
    gate.arrived().await;
    let mut drain = std::pin::pin!(h.provider_service.drain_reloads());
    still_pending(&mut drain).await;

    tokio::time::pause();
    for _ in 0..5 {
        tokio::time::advance(Duration::from_secs(121)).await;
        still_pending(&mut drain).await;
    }
    tokio::time::resume();

    assert!(!reload.is_finished());
    assert_eq!(raw_interval(&h, "ci").await, Some(60));
    assert_eq!(reload_state(&h).await, before);
    gate.open();
    drain.await;

    assert!(reload.await.unwrap().is_ok());
    assert_eq!(raw_interval(&h, "ci").await, Some(60));
    let after = reload_state(&h).await;
    assert_eq!(after.providers_changed, before.providers_changed + 1);
    assert_eq!(h.bus.count("applications-changed"), announced + 1);
    assert_eq!(config_apps(&after.tables)[0].name, "Shop 2");
}

#[tokio::test]
async fn shutdown_during_a_paused_commit_drains_to_activation() {
    let (h, before) = reload_baseline().await;
    let announced = h.bus.count("applications-changed");
    let gate = Gate::new();
    h.commit_hook.pause(Arc::clone(&gate));
    write_next(&h);
    let reload = reload_in_background(&h);
    gate.arrived().await;

    begin_shutdown_while_paused(&h, &gate).await;

    assert_eq!(raw_interval(&h, "ci").await, Some(60));
    let after = reload_state(&h).await;
    assert_eq!(after.providers_changed, before.providers_changed + 1);
    assert_eq!(after.views_changed, before.views_changed + 1);
    assert_eq!(h.bus.count("applications-changed"), announced + 1);
    assert_eq!(config_apps(&after.tables)[0].name, "Shop 2");
    assert!(reload.await.unwrap().is_ok());
    assert!(banner(&h).await.is_none());
}

#[tokio::test]
async fn shutdown_during_a_paused_commit_that_fails_drains_to_restore() {
    let (h, before) = reload_baseline().await;
    let announced = h.bus.count("applications-changed");
    let gate = Gate::new();
    h.commit_hook.pause(Arc::clone(&gate));
    h.commit_hook
        .fail_after_pause(DomainError::InternalError("commit failed".into()));
    write_next(&h);
    let reload = reload_in_background(&h);
    gate.arrived().await;
    assert_eq!(raw_interval(&h, "ci").await, Some(60));

    begin_shutdown_while_paused(&h, &gate).await;

    assert_eq!(raw_interval(&h, "ci").await, Some(30));
    assert_eq!(raw_interval(&h, "ce").await, None);
    assert_eq!(reload_state(&h).await, before);
    assert!(reload.await.unwrap().is_err());
    assert!(banner(&h).await.is_some());
    assert_eq!(h.bus.count("applications-changed"), announced + 1);
}

#[tokio::test]
async fn correlation_ignore_stops_a_matching_unit_from_creating_an_application() {
    let (h, ci) = setup().await;
    write(
        &h,
        "[[correlation.ignore]]\nprovider = \"ci\"\npipeline = \"sandbox-*\"\n",
    );
    apply(&h).await.unwrap();

    feed(&h, ci, &["shop-api", "sandbox-x"]).await;

    let snap = snapshot(&h).await;
    let names: Vec<&str> = unit_apps(&snap)
        .iter()
        .map(|a| a.identity_key.as_str())
        .collect();
    assert_eq!(names.len(), 1);
    assert!(names[0].ends_with("shop-api"));
    assert!(snap
        .members
        .iter()
        .all(|m| m.pipeline_id != id(ci, "sandbox-x")));
}

#[tokio::test]
async fn removing_a_table_restores_the_automatic_application() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    write(&h, SHOP);
    apply(&h).await.unwrap();
    assert!(unit_apps(&snapshot(&h).await).is_empty());

    write(&h, "");
    apply(&h).await.unwrap();

    let snap = snapshot(&h).await;
    assert!(config_apps(&snap).is_empty());
    assert!(snap.targets.iter().all(|t| t.name != "QA"));
    assert_eq!(unit_apps(&snap).len(), 1);
    assert!(snap
        .members
        .iter()
        .any(|m| m.pipeline_id == id(ci, "shop-api")));
}

#[tokio::test]
async fn commit_emits_exactly_one_applications_changed() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    let before = h.bus.count("applications-changed");

    write(&h, SHOP);
    apply(&h).await.unwrap();
    tokio::time::sleep(Duration::from_millis(700)).await;

    assert_eq!(h.bus.count("applications-changed"), before + 1);
}

async fn warnings_of(h: &Harness, name: &str) -> Vec<String> {
    settle(service(h)).await;
    service(h)
        .list(ApplicationListQuery {
            include_hidden: true,
            ..ApplicationListQuery::default()
        })
        .await
        .unwrap()
        .applications
        .into_iter()
        .find(|a| a.name == name)
        .map(|a| a.warnings)
        .unwrap_or_default()
}

#[tokio::test]
async fn late_conflict_shows_a_warning_on_both_applications() {
    let (h, ci) = setup().await;
    write(
        &h,
        "[applications.a]\nname = \"A\"\n\n[[applications.a.members]]\nprovider = \"ci\"\npipeline = \"shop-*\"\n\n[applications.b]\nname = \"B\"\n\n[[applications.b.members]]\nprovider = \"ci\"\npipeline = \"*-api\"\n",
    );
    apply(&h).await.unwrap();
    feed(&h, ci, &["shop-api"]).await;

    for name in ["A", "B"] {
        let warnings = warnings_of(&h, name).await;
        assert!(
            warnings.iter().any(|w| w.contains("shop-api")),
            "{name}: {warnings:?}"
        );
    }
}

#[tokio::test]
async fn selector_matching_nothing_shows_a_warning() {
    let (h, _) = setup().await;
    write(&h, SHOP);
    apply(&h).await.unwrap();

    let warnings = warnings_of(&h, "Shop").await;

    assert!(
        warnings.iter().any(|w| w.contains("shop-*")),
        "{warnings:?}"
    );
}

#[tokio::test]
async fn file_wins_over_a_ui_override_which_is_then_shadowed() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api", "other-x"]).await;
    let mine = service(&h)
        .create_application(
            CreateApplicationInput {
                name: "Mine".to_string(),
                members: vec![CreateApplicationMember {
                    pipeline_id: id(ci, "other-x"),
                    stage: None,
                }],
            },
            None,
        )
        .await
        .unwrap();
    service(&h)
        .apply_edit(
            mine.summary.id,
            ApplicationEdit::Link {
                pipeline_id: id(ci, "shop-api"),
                stage: None,
            },
            None,
        )
        .await
        .unwrap();

    write(&h, SHOP);
    apply(&h).await.unwrap();

    let snap = snapshot(&h).await;
    assert_eq!(members_of(&snap, "shop").len(), 1);
    settle(service(&h)).await;
    let detail = service(&h).detail(mine.summary.id).await.unwrap();
    assert!(
        detail.overrides.iter().any(|o| o.shadowed),
        "{:?}",
        detail.overrides
    );
}

#[tokio::test]
async fn copy_as_toml_loaded_as_config_gives_the_same_members_and_targets() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    let mine = service(&h)
        .create_application(
            CreateApplicationInput {
                name: "Mine".to_string(),
                members: vec![CreateApplicationMember {
                    pipeline_id: id(ci, "shop-api"),
                    stage: None,
                }],
            },
            None,
        )
        .await
        .unwrap();
    let toml = mine.toml.clone().unwrap();
    let members: Vec<String> = mine.members.iter().map(|m| m.pipeline.id.clone()).collect();
    let targets: Vec<String> = mine.summary.chips.iter().map(|c| c.label.clone()).collect();

    write(&h, &toml);
    apply(&h).await.unwrap();

    let snap = snapshot(&h).await;
    let config = config_apps(&snap);
    assert_eq!(config.len(), 1);
    settle(service(&h)).await;
    let detail = service(&h).detail(config[0].id).await.unwrap();
    let loaded: Vec<String> = detail
        .members
        .iter()
        .map(|m| m.pipeline.id.clone())
        .collect();
    let loaded_targets: Vec<String> = detail
        .summary
        .chips
        .iter()
        .map(|c| c.label.clone())
        .collect();
    assert_eq!(loaded, members);
    assert_eq!(loaded_targets, targets);
}

#[tokio::test]
async fn config_target_tables_name_group_and_order_targets() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-qa1", "shop-qa2", "shop-prod"]).await;
    write(
        &h,
        "[applications.shop]\nname = \"Shop\"\n\n[[applications.shop.members]]\nprovider = \"ci\"\npipeline = \"shop-*\"\n\n[applications.shop.targets.qa]\nname = \"QA\"\nnamespaces = [\"shop-qa*\"]\n\n[applications.shop.targets.prod]\nname = \"Prod\"\nnamespaces = [\"shop-prod\"]\nafter = [\"qa\"]\n",
    );
    apply(&h).await.unwrap();

    let snap = snapshot(&h).await;
    let own = app_targets(&snap, "shop");
    let qa: Vec<_> = own
        .iter()
        .filter(|t| t.target_key.contains("shop-qa"))
        .collect();
    let prod: Vec<_> = own
        .iter()
        .filter(|t| t.target_key.contains("shop-prod"))
        .collect();
    assert_eq!(qa.len(), 2);
    assert_eq!(qa[0].group_key, qa[1].group_key);
    assert_eq!(prod.len(), 1);
    assert_eq!(prod[0].name, "Prod");
    assert_ne!(qa[0].group_key, prod[0].group_key);
    let directions = app_directions(&snap, "shop");
    assert_eq!(directions.len(), 1);
    assert_eq!(directions[0].from_group, qa[0].group_key);
    assert_eq!(directions[0].to_group, prod[0].group_key);
    assert_eq!(directions[0].source, "config");

    let app = list_named(&h, "Shop").await;
    let labels: Vec<&str> = app.chips.iter().map(|c| c.label.as_str()).collect();
    assert!(
        labels.contains(&"QA") && labels.contains(&"Prod"),
        "{labels:?}"
    );
    assert_eq!(app.arrows.len(), 1);
}

#[tokio::test]
async fn provider_only_change_emits_no_applications_changed_and_a_valid_file_clears_the_banner() {
    let (h, ci) = setup().await;
    feed(&h, ci, &["shop-api"]).await;
    write(&h, SHOP);
    apply(&h).await.unwrap();
    let before = h.bus.count("applications-changed");

    std::fs::write(
        h.config_state.config_path(),
        format!("{}{SHOP}", PROVIDERS.replace("= 30", "= 60")),
    )
    .unwrap();
    apply(&h).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(h.bus.count("applications-changed"), before);

    write(&h, "[applications.shop\n");
    assert!(apply(&h).await.is_err());
    assert!(banner(&h).await.is_some());
    write(&h, SHOP);
    apply(&h).await.unwrap();
    assert!(banner(&h).await.is_none());
}

const CHECKOUT: &str = "[applications.checkout]\nname = \"Checkout\"\n\n[[applications.checkout.members]]\nprovider = \"ci\"\npipeline = \"checkout-*\"\n\n[applications.checkout.targets.qa]\nname = \"QA\"\nnamespaces = [\"checkout-qa\"]\n\n[applications.checkout.targets.staging]\nname = \"Staging\"\nnamespaces = [\"checkout-staging\"]\nafter = [\"qa\"]\n\n[applications.checkout.targets.prod]\nname = \"Prod\"\nnamespaces = [\"checkout-prod\"]\nafter = [\"staging\"]\n";

async fn listed(h: &Harness) -> Vec<ApplicationSummary> {
    settle(service(h)).await;
    service(h)
        .list(ApplicationListQuery {
            include_hidden: true,
            ..ApplicationListQuery::default()
        })
        .await
        .unwrap()
        .applications
}

async fn claimed_checkout(h: &Harness, ci: i64) {
    let mut pipelines: Vec<Pipeline> = ["checkout-qa", "checkout-staging", "checkout-prod"]
        .iter()
        .map(|name| {
            let mut p = unit_pipeline(ci, name);
            p.correlation.app_label = Some("checkout".to_string());
            p
        })
        .collect();
    pipelines.push(pipeline(ci, "checkout-build", PipelineStatus::Success));
    feed_pipelines(h, ci, &pipelines).await;
    let ids: Vec<String> = service(h)
        .suggestions()
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    service(h).accept_suggestions(ids, None).await.unwrap();
    let merged = listed(h).await;
    assert_eq!(merged.len(), 1, "{merged:?}");
    let rename = ApplicationEdit::Rename {
        name: "Checkout".to_string(),
    };
    service(h)
        .apply_edit(merged[0].id, rename, None)
        .await
        .unwrap();
    write(h, CHECKOUT);
    apply(h).await.unwrap();
}

#[tokio::test]
async fn config_claim_archives_the_merged_automatic_application() {
    let (h, ci) = setup().await;
    claimed_checkout(&h, ci).await;

    let apps = listed(&h).await;
    assert_eq!(apps.len(), 1, "{apps:?}");
    assert_eq!(apps[0].identity_kind, IdentityKind::Config);
    assert!(!apps[0].capabilities.edit);
    let labels: Vec<&str> = apps[0].chips.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["QA", "Staging", "Prod"]);
    assert_eq!(apps[0].arrows.len(), 2);
    let detail = service(&h).detail(apps[0].id).await.unwrap();
    let kinds: Vec<OverrideKind> = detail.overrides.iter().map(|o| o.kind).collect();
    assert!(kinds.contains(&OverrideKind::Merge), "{kinds:?}");
    assert!(kinds.contains(&OverrideKind::Rename), "{kinds:?}");
    assert!(detail.overrides.iter().all(|o| o.shadowed));

    write(&h, "");
    apply(&h).await.unwrap();

    let apps = listed(&h).await;
    assert_eq!(apps.len(), 1, "{apps:?}");
    assert_eq!(apps[0].name, "Checkout");
    assert_ne!(apps[0].identity_kind, IdentityKind::Config);
    assert!(apps[0].capabilities.edit);
    assert_eq!(apps[0].chips.len(), 3);
}

#[tokio::test]
async fn shadowed_override_can_be_reverted_from_the_config_application() {
    let (h, ci) = setup().await;
    claimed_checkout(&h, ci).await;
    let config_app = listed(&h).await.remove(0);
    let before = service(&h).detail(config_app.id).await.unwrap().overrides;
    let rename = before
        .iter()
        .find(|o| o.kind == OverrideKind::Rename)
        .unwrap();
    assert!(rename.shadowed);

    let detail = service(&h)
        .revert_override(config_app.id, rename.id)
        .await
        .unwrap();

    assert_eq!(detail.overrides.len(), before.len() - 1);
    assert!(detail.overrides.iter().all(|o| o.id != rename.id));
    assert_eq!(snapshot(&h).await.overrides.len(), before.len() - 1);
    let denied = service(&h)
        .apply_edit(
            config_app.id,
            ApplicationEdit::Rename {
                name: "Other".to_string(),
            },
            None,
        )
        .await;
    assert!(matches!(denied, Err(DomainError::InvalidConfig(_))));
    let missing = service(&h).revert_override(config_app.id, rename.id).await;
    assert!(matches!(missing, Err(DomainError::NotFound(_))));
}
