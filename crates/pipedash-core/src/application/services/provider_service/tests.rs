use std::collections::HashMap;
use std::io::{
    Read,
    Write,
};
use std::sync::Arc;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::PluginError;
use pipedash_plugin_api::{
    Plugin as PluginTrait,
    RateLimit,
};
use tokio::time::Instant;

use super::plugin::keep_selection_with_ci;
use super::quota::quota_pause;
use crate::application::services::health_service::HealthInput;
use crate::application::test_support::plugins::{
    STUB,
    STUB_HTTP,
    STUB_OIDC,
    STUB_OPTIONS,
};
use crate::application::test_support::{
    pipeline,
    FeedStub,
    Gate,
    Harness,
    StubProvider,
};
use crate::application::FetchResult;
use crate::domain::PipelineStatus;
use crate::domain::{
    DomainError,
    FetchStatus,
    ProviderConfig,
    ProviderImportResult,
    ProviderRecovery,
    SignInCredential,
    SignInStart,
    SignInStatus,
    SIGN_IN_CONFIG_KEY,
};
use crate::event::CoreEvent;
use crate::infrastructure::config::ConfigLoader;
use crate::infrastructure::oauth::test_idp::{
    jwt,
    Idp,
};
use crate::infrastructure::TokenStore;
use crate::test_env::EnvGuard;

const STUB_SECRET: &str = "stubtok_exampleSECRET0123456789";
const STUB_SECOND_SECRET: &str = "stubtok_SECRET-VALUE";
const STUB_DEFAULT_SECRET: &str = "hunter2-default";

fn provider(
    name: &str, provider_type: &str, token: &str, refresh_interval: i64, config: &[(&str, &str)],
) -> ProviderConfig {
    ProviderConfig {
        id: None,
        name: name.to_string(),
        provider_type: provider_type.to_string(),
        token: token.to_string(),
        config: config
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        refresh_interval,
        version: None,
    }
}

struct CiCheck(Option<&'static [&'static str]>);

#[async_trait::async_trait]
impl PluginTrait for CiCheck {
    fn metadata(&self) -> &pipedash_plugin_api::PluginMetadata {
        unimplemented!()
    }

    fn initialize(
        &mut self, _: i64, _: HashMap<String, String>, _: Option<Arc<reqwest::Client>>,
    ) -> pipedash_plugin_api::PluginResult<()> {
        Ok(())
    }

    async fn validate_credentials(&self) -> pipedash_plugin_api::PluginResult<bool> {
        Ok(true)
    }

    async fn fetch_pipelines(
        &self,
    ) -> pipedash_plugin_api::PluginResult<Vec<pipedash_plugin_api::Pipeline>> {
        Ok(Vec::new())
    }

    async fn fetch_run_history(
        &self, _: &str, _: usize,
    ) -> pipedash_plugin_api::PluginResult<Vec<pipedash_plugin_api::PipelineRun>> {
        Ok(Vec::new())
    }

    async fn fetch_run_details(
        &self, _: &str, _: i64,
    ) -> pipedash_plugin_api::PluginResult<pipedash_plugin_api::PipelineRun> {
        Err(PluginError::NotSupported(String::new()))
    }

    async fn trigger_pipeline(
        &self, _: pipedash_plugin_api::TriggerParams,
    ) -> pipedash_plugin_api::PluginResult<pipedash_plugin_api::TriggerResult> {
        Err(PluginError::NotSupported(String::new()))
    }

    async fn ids_with_ci(&self, _: Vec<String>) -> pipedash_plugin_api::PluginResult<Vec<String>> {
        self.0
            .map(|kept| kept.iter().map(|id| id.to_string()).collect())
            .ok_or_else(|| PluginError::NetworkError("reset".to_string()))
    }
}

#[tokio::test]
async fn a_saved_selection_keeps_only_repositories_with_ci() {
    let selected = &[("selected_items", "acme/api, acme/empty,acme/web")];

    let mut config = provider("stub", STUB, "t", 30, selected);
    keep_selection_with_ci(&CiCheck(Some(&["acme/api", "acme/web"])), &mut config)
        .await
        .unwrap();
    assert_eq!(config.config["selected_items"], "acme/api,acme/web");

    let mut config = provider("stub", STUB, "t", 30, selected);
    keep_selection_with_ci(&CiCheck(None), &mut config)
        .await
        .unwrap();
    assert_eq!(
        config.config["selected_items"],
        "acme/api, acme/empty,acme/web"
    );

    let mut config = provider("stub", STUB, "t", 30, selected);
    let error = keep_selection_with_ci(&CiCheck(Some(&[])), &mut config)
        .await
        .unwrap_err();
    assert!(matches!(error, DomainError::InvalidConfig(_)), "{error}");
    assert_eq!(
        config.config["selected_items"],
        "acme/api, acme/empty,acme/web"
    );
}

#[tokio::test]
async fn test_export_providers_toml_round_trips_without_tokens() {
    let source = Harness::with_real_plugins().await;
    let repository = source.repository.clone();
    let service = &source.provider_service;

    let originals = [
        provider(
            "stub-main",
            STUB,
            STUB_SECRET,
            45,
            &[("selected_items", "acme/api,acme/web")],
        ),
        provider(
            "Stub Main",
            STUB,
            &format!("${{STUB_API_TOKEN:-{STUB_DEFAULT_SECRET}}}"),
            120,
            &[
                ("display_name", "Stub Display"),
                ("server_url", "http://127.0.0.1:9"),
                ("username", "ci"),
                ("selected_items", "(root)/deploy"),
            ],
        ),
        provider("stub_second", STUB, STUB_SECOND_SECRET, 30, &[]),
    ];
    for original in &originals {
        repository.add_provider(original).await.unwrap();
    }

    let exported = service.export_providers_toml().await.unwrap();

    for secret in [STUB_SECRET, STUB_SECOND_SECRET, STUB_DEFAULT_SECRET] {
        assert!(!exported.contains(secret), "export leaked {secret}");
    }
    assert!(exported.contains("token = \"${PIPEDASH_TOKEN_STUB_MAIN:-}\""));
    assert!(exported.contains("token = \"${STUB_API_TOKEN:-}\""));
    assert!(exported.contains("token = \"${PIPEDASH_TOKEN_STUB_SECOND:-}\""));

    let _env = EnvGuard::set(&[
        ("PIPEDASH_TOKEN_STUB_MAIN", Some("env-stub-token")),
        ("STUB_API_TOKEN", Some("env-api-token")),
        ("PIPEDASH_TOKEN_STUB_SECOND", None),
    ]);
    let parsed = ConfigLoader::parse(&exported).unwrap();

    let mut keys: Vec<_> = parsed.providers.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, ["Stub Main", "stub-main", "stub_second"]);
    assert_eq!(parsed.providers["stub-main"].token, "env-stub-token");
    assert_eq!(parsed.providers["Stub Main"].token, "env-api-token");
    assert_eq!(parsed.providers["stub_second"].token, "");
    assert_eq!(
        parsed.providers["Stub Main"].name.as_deref(),
        Some("Stub Display")
    );

    let target_harness = Harness::with_real_plugins().await;
    let target = &target_harness.repository;
    crate::infrastructure::config::sync::ProviderSyncService::new()
        .sync_toml_to_db(
            parsed.providers,
            target.config_backend().as_ref(),
            &crate::infrastructure::MemoryTokenStore::new(),
            false,
            &crate::application::test_support::plugins::registry(),
        )
        .await
        .unwrap();

    let imported = target.config_backend().list_providers().await.unwrap();

    assert_eq!(imported.len(), originals.len());
    for original in &originals {
        let imported = imported.iter().find(|p| p.name == original.name).unwrap();
        assert_eq!(imported.name, original.name);
        assert_eq!(imported.provider_type, original.provider_type);
        assert_eq!(imported.refresh_interval, original.refresh_interval);
        assert_eq!(imported.config, original.config);
    }
}

#[tokio::test]
async fn test_import_providers_toml_adds_missing_providers_without_tokens() {
    let source = Harness::with_real_plugins().await;
    let named = provider(
        "Stub Main",
        STUB,
        "${STUB_API_TOKEN}",
        120,
        &[
            ("display_name", "Stub Display"),
            ("server_url", "http://127.0.0.1:9"),
            ("username", "ci"),
            ("selected_items", "(root)/deploy"),
        ],
    );
    for original in [
        provider(
            "stub-main",
            STUB,
            STUB_SECRET,
            45,
            &[("selected_items", "acme/api")],
        ),
        named.clone(),
    ] {
        source.repository.add_provider(&original).await.unwrap();
    }
    let exported = source
        .provider_service
        .export_providers_toml()
        .await
        .unwrap();

    let target = Harness::with_real_plugins().await;
    let kept = provider(
        "stub-main",
        STUB,
        "target-token",
        300,
        &[("selected_items", "acme/web")],
    );
    let kept_id = target.repository.add_provider(&kept).await.unwrap();

    let _env = EnvGuard::set(&[("STUB_API_TOKEN", Some("env-api-token"))]);
    let result = target
        .provider_service
        .import_providers_toml(&exported)
        .await
        .unwrap();

    assert_eq!(
        result,
        ProviderImportResult {
            added: vec!["Stub Display".to_string()],
            skipped: vec!["stub-main".to_string()],
            failed: Vec::new(),
            not_attempted: Vec::new(),
        }
    );
    let unchanged = target.repository.get_provider(kept_id).await.unwrap();
    assert_eq!(unchanged.token, kept.token);
    assert_eq!(unchanged.refresh_interval, kept.refresh_interval);
    assert_eq!(unchanged.config, kept.config);

    let imported = target.repository.list_providers().await.unwrap();
    let imported = imported.iter().find(|p| p.name == named.name).unwrap();
    let row = target
        .repository
        .config_backend()
        .get_provider(imported.id.unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(imported.token, "");
    assert_eq!(row.token, "");
    assert_eq!(imported.provider_type, named.provider_type);
    assert_eq!(imported.refresh_interval, named.refresh_interval);
    assert_eq!(imported.config, named.config);
    assert_eq!(target.bus.count("providers-changed"), 1);
}

#[tokio::test]
async fn test_import_providers_toml_rejects_unknown_type_before_adding() {
    let h = Harness::with_real_plugins().await;
    let content = r#"
[providers.stub-main]
type = "stub"

[providers.mystery]
type = "unknown"
"#;

    let err = h
        .provider_service
        .import_providers_toml(content)
        .await
        .unwrap_err();

    assert!(matches!(err, DomainError::InvalidProviderType(_)), "{err}");
    assert!(h.repository.list_providers().await.unwrap().is_empty());
    assert_eq!(h.bus.count("providers-changed"), 0);
}

#[tokio::test]
async fn test_import_providers_toml_reports_what_happened_when_a_write_fails() {
    let h = Harness::new().await;
    execute(
        &h,
        "CREATE TRIGGER fail_ci_b BEFORE INSERT ON providers WHEN NEW.name = 'ci-b' \
             BEGIN SELECT RAISE(ABORT, 'write failed'); END",
    )
    .await;
    let content = r#"
[providers.ci-a]
type = "stub"

[providers.ci-b]
name = "CI B"
type = "stub"

[providers.ci-c]
name = "CI C"
type = "stub"
"#;

    let result = h
        .provider_service
        .import_providers_toml(content)
        .await
        .unwrap();

    assert_eq!(result.added, ["ci-a"]);
    assert!(result.skipped.is_empty());
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].name, "CI B");
    assert!(result.failed[0].error.contains("write failed"));
    assert_eq!(result.not_attempted, ["CI C"]);
    let stored: Vec<String> = h
        .repository
        .list_providers()
        .await
        .unwrap()
        .into_iter()
        .map(|provider| provider.name)
        .collect();
    assert_eq!(stored, ["ci-a"]);
    assert_eq!(h.bus.count("providers-changed"), 1);
}

#[test]
fn quota_pause_starts_under_ten_percent_until_reset() {
    let now = Utc::now();
    let reset_at = now + chrono::Duration::minutes(20);
    let pause = |remaining, reset_at| {
        quota_pause(
            &RateLimit {
                limit: 5000,
                remaining,
                reset_at,
            },
            now,
        )
    };

    assert_eq!(pause(500, reset_at), None);
    assert_eq!(pause(499, reset_at), Some(reset_at));
    assert_eq!(pause(0, reset_at), Some(reset_at));
    assert_eq!(pause(0, now), None);
    assert_eq!(
        quota_pause(
            &RateLimit {
                limit: 0,
                remaining: 0,
                reset_at,
            },
            now,
        ),
        None
    );
}

fn reset_in(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(Utc::now().timestamp() + seconds, 0).unwrap()
}

fn quota(remaining: u64, reset_at: DateTime<Utc>) -> Option<RateLimit> {
    Some(RateLimit {
        limit: 5000,
        remaining,
        reset_at,
    })
}

#[tokio::test]
async fn refresh_skips_a_provider_with_its_quota_almost_used_up() {
    let h = Harness::new().await;
    h.start().await;
    let reset = reset_in(1200);
    let (id, stub) = h.add_stub(StubProvider::new).await;
    *stub.rate_limit.lock().unwrap() = quota(0, reset);

    let first = h
        .pipeline_service
        .fetch_now(&[id], Duration::from_secs(5))
        .await;
    let again = h
        .pipeline_service
        .fetch_now(&[id], Duration::from_secs(5))
        .await;
    let summaries = h.provider_service.list_providers().await.unwrap();

    assert_eq!(stub.rate_limit_check_count(), 1);
    assert_eq!(stub.fetch_count(), 0);
    assert_eq!(summaries[0].paused_until, Some(reset));
    assert!(matches!(summaries[0].last_fetch_status, FetchStatus::Never));
    assert_eq!(first[0].state, crate::domain::RefreshState::Recent);
    assert_eq!(again[0].state, crate::domain::RefreshState::Recent);
    assert_eq!(again[0].kind, Some(crate::domain::FailureKind::RateLimited));
    assert_eq!(stub.rate_limit_check_count(), 1);
}

#[tokio::test]
async fn refresh_fetches_a_provider_with_quota_left() {
    let h = Harness::new().await;
    h.start().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    *stub.rate_limit.lock().unwrap() = quota(4000, reset_in(1200));

    h.pipeline_service
        .fetch_now(&[id], Duration::from_secs(5))
        .await;
    let summaries = h.provider_service.list_providers().await.unwrap();

    assert_eq!(stub.rate_limit_check_count(), 1);
    assert_eq!(stub.fetch_count(), 1);
    assert_eq!(summaries[0].paused_until, None);
}

#[tokio::test]
async fn quota_answer_for_replaced_credentials_does_not_pause_the_new_ones() {
    let h = Harness::new().await;
    let gate = Gate::new();
    let (id, old) = h
        .add_stub(|id| StubProvider::new(id).rate_limit_gated(&gate))
        .await;
    *old.rate_limit.lock().unwrap() = quota(0, reset_in(1200));
    h.stubs.queue(StubProvider::new(id));
    let service = Arc::clone(&h.provider_service);
    let lease = service.acquire(id).await.unwrap();

    let check = tokio::spawn({
        let service = Arc::clone(&service);
        let lease = lease.clone();
        async move { service.check_quota(&lease).await }
    });
    gate.arrived().await;
    let replacement =
        (service.provider_factory)(&h.repository.get_provider(id).await.unwrap()).unwrap();
    service.replace(id, replacement, 30).await;
    gate.open();

    assert_eq!(check.await.unwrap(), None);
    assert_eq!(h.scheduler.paused_until(id, Utc::now()), None);
}

fn updated(h: &Harness, id: i64) -> ProviderConfig {
    let mut config = h.config("renamed", 45);
    config.id = Some(id);
    config
}

#[tokio::test]
async fn an_update_during_a_blocked_fetch_supersedes_the_old_attempt() {
    let h = Harness::new().await;
    h.start().await;
    let old_gate = Gate::new();
    let (id, old) = h
        .add_stub(|id| {
            StubProvider::new(id)
                .gated(&old_gate)
                .returning(vec![pipeline(id, "old", PipelineStatus::Success)])
        })
        .await;
    let new = h.stubs.queue(StubProvider::new(id).returning(vec![pipeline(
        id,
        "new",
        PipelineStatus::Failed,
    )]));
    let blocked = h.spawn_fetch(id).await;
    old_gate.arrived().await;
    let before = h.provider_service.acquire(id).await.unwrap().generation;

    h.provider_service
        .update_provider(id, updated(&h, id))
        .await
        .unwrap();
    let updated_at = h
        .bus
        .names()
        .iter()
        .position(|name| *name == "provider-updated")
        .unwrap();
    h.bus
        .wait_for("the fetch queued by the update", |events| {
            events[updated_at..]
                .iter()
                .any(|event| matches!(event, CoreEvent::PipelinesUpdated { .. }))
        })
        .await;
    old_gate.open();
    let old_result = blocked.await.unwrap();

    assert!(matches!(old_result, FetchResult::Superseded));
    assert_eq!(h.cached_names(id).await, ["new"]);
    let published: Vec<_> = h.bus.events()[updated_at..]
        .iter()
        .filter_map(|event| match event {
            CoreEvent::PipelinesUpdated { pipelines, .. } => {
                Some(pipelines.iter().map(|p| p.name.clone()).collect::<Vec<_>>())
            }
            _ => None,
        })
        .collect();
    assert_eq!(published, vec![vec!["new".to_string()]]);
    let view = h.scheduler.view(id, Instant::now()).unwrap();
    assert_eq!(
        (view.failures, view.last_failure, view.in_flight),
        (0, None, false)
    );
    assert_ne!(
        h.provider_service.acquire(id).await.unwrap().generation,
        before
    );
    assert_eq!((old.fetch_count(), new.fetch_count()), (1, 1));
}

#[tokio::test]
async fn an_update_resets_the_fetch_status_and_the_schedule() {
    let h = Harness::new().await;
    let (id, _) = h
        .add_stub(|id| StubProvider::new(id).failing(DomainError::NetworkError("down".into())))
        .await;
    h.fetch_directly(id).await;
    h.health_inputs();
    assert_eq!(h.scheduler.view(id, Instant::now()).unwrap().failures, 1);

    h.provider_service
        .update_provider(id, updated(&h, id))
        .await
        .unwrap();
    let inputs = h.health_inputs();

    let event = h
        .bus
        .events()
        .into_iter()
        .find_map(|event| match event {
            CoreEvent::ProviderUpdated { provider, .. } => Some(provider),
            _ => None,
        })
        .unwrap();
    assert!(matches!(event.last_fetch_status, FetchStatus::Never));
    assert_eq!(event.last_fetch_error, None);
    assert_eq!(event.last_fetch_error_kind, None);
    assert_eq!(event.health.next_attempt_at, None);
    assert_eq!(event.name, "renamed");
    assert_eq!(h.scheduler.view(id, Instant::now()).unwrap().failures, 0);
    let lease = h.provider_service.acquire(id).await.unwrap();
    assert!(matches!(
        inputs.as_slice(),
        [HealthInput::ProvidersChanged { changed }]
            if changed.len() == 1 && changed[0].generation == Some(lease.generation)
    ));
}

#[tokio::test]
async fn an_old_failure_finishing_after_an_update_leaves_the_new_schedule_clean() {
    let h = Harness::new().await;
    h.start().await;
    let old_gate = Gate::new();
    let new_gate = Gate::new();
    let (id, _) = h
        .add_stub(|id| {
            StubProvider::new(id)
                .gated(&old_gate)
                .failing(DomainError::NetworkError("down".into()))
        })
        .await;
    h.stubs.queue(StubProvider::new(id).gated(&new_gate));
    let blocked = h.spawn_fetch(id).await;
    old_gate.arrived().await;

    h.provider_service
        .update_provider(id, updated(&h, id))
        .await
        .unwrap();
    new_gate.arrived().await;
    old_gate.open();
    let old_result = blocked.await.unwrap();

    assert!(matches!(old_result, FetchResult::Superseded));
    let view = h.scheduler.view(id, Instant::now()).unwrap();
    assert_eq!(
        (view.failures, view.last_failure, view.in_flight),
        (0, None, true)
    );
    let fetch = h.repository.get_provider_fetch_status(id).await.unwrap();
    assert_eq!((fetch.status.as_str(), fetch.error), ("never", None));
    new_gate.open();
    h.bus
        .wait_for("the new fetch", |events| {
            events
                .iter()
                .any(|event| matches!(event, CoreEvent::PipelinesUpdated { .. }))
        })
        .await;
    assert_eq!(h.scheduler.view(id, Instant::now()).unwrap().failures, 0);
}

#[tokio::test]
async fn a_removal_during_a_blocked_fetch_leaves_nothing_behind() {
    let h = Harness::new().await;
    let gate = Gate::new();
    let (id, _) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
    let blocked = h.spawn_fetch(id).await;
    gate.arrived().await;
    h.health_inputs();

    h.provider_service.remove_provider(id).await.unwrap();
    let removed_at = h.bus.events().len();
    gate.open();
    let result = blocked.await.unwrap();
    let inputs = h.health_inputs();

    assert!(matches!(result, FetchResult::Superseded));
    assert!(h.repository.get_provider(id).await.is_err());
    assert!(h.repository.get_provider_fetch_status(id).await.is_err());
    assert!(h.cached_names(id).await.is_empty());
    assert!(h.scheduler.view(id, Instant::now()).is_none());
    assert!(h.bus.names()[removed_at..].is_empty());
    assert_eq!(h.bus.names()[removed_at - 1], "provider-removed");
    assert!(matches!(
        inputs.as_slice(),
        [HealthInput::ProvidersChanged { changed }]
            if changed[0].id == id && changed[0].generation.is_none()
    ));
}

#[tokio::test]
async fn concurrent_acquires_share_one_provider() {
    let h = Harness::new().await;
    let id = h.add_provider("shared").await;

    let leases = futures::future::join_all((0..8).map(|_| h.provider_service.acquire(id))).await;

    let first = leases[0].as_ref().unwrap();
    for lease in &leases {
        let lease = lease.as_ref().unwrap();
        assert!(Arc::ptr_eq(&lease.provider, &first.provider));
        assert_eq!(lease.generation, first.generation);
    }
}

#[tokio::test]
async fn a_lazy_acquire_racing_a_removal_does_not_bring_the_provider_back() {
    let h = Harness::new().await;
    let id = h.add_provider("racing").await;
    let lock = h.provider_service.commit_lock(id);
    let held = lock.lock().await;
    let removal = tokio::spawn({
        let service = Arc::clone(&h.provider_service);
        async move { service.remove_provider(id).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let acquire = tokio::spawn({
        let service = Arc::clone(&h.provider_service);
        async move { service.acquire(id).await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;

    drop(held);
    removal.await.unwrap().unwrap();
    let acquired = acquire.await.unwrap();

    assert!(matches!(acquired, Err(DomainError::ProviderNotFound(_))));
    assert!(h.provider_service.current(id).await.is_none());
    assert!(h.scheduler.view(id, Instant::now()).is_none());
}

async fn wait_for_startup_snapshot(h: &Harness, id: i64) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while h.stubs.token_of(id).is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn bulk_load_keeps_a_concurrent_interval_edit() {
    let h = Harness::new().await;
    let id = h.add_provider("startup-race").await;
    let service = Arc::clone(&h.provider_service);
    let lock = service.commit_lock(id);
    let held = lock.lock().await;
    let mut update = Box::pin(service.update_provider_refresh_interval(id, 120));
    assert!(futures::poll!(&mut update).is_pending());
    let loading = tokio::spawn({
        let service = Arc::clone(&service);
        async move { service.load_all_providers().await }
    });
    wait_for_startup_snapshot(&h, id).await;
    drop(held);
    update.await.unwrap();
    loading.await.unwrap().unwrap();
    h.fetch_directly(id).await;
    let saved = h.repository.get_provider(id).await.unwrap();
    let scheduled = h.scheduler.view(id, Instant::now()).unwrap();

    assert_eq!(saved.refresh_interval, 120);
    assert!(scheduled.next_attempt_in > Duration::from_secs(115));
}

#[tokio::test]
async fn bulk_load_does_not_reinstall_a_concurrently_removed_provider() {
    let h = Harness::new().await;
    let id = h.add_provider("removed-during-startup").await;
    let service = Arc::clone(&h.provider_service);
    let lock = service.commit_lock(id);
    let held = lock.lock().await;
    let mut removal = Box::pin(service.remove_provider(id));
    assert!(futures::poll!(&mut removal).is_pending());
    let loading = tokio::spawn({
        let service = Arc::clone(&service);
        async move { service.load_all_providers().await }
    });
    wait_for_startup_snapshot(&h, id).await;
    drop(held);
    removal.await.unwrap();
    loading.await.unwrap().unwrap();

    assert!(matches!(
        h.repository.get_provider(id).await,
        Err(DomainError::ProviderNotFound(_))
    ));
    assert!(service.current(id).await.is_none());
    assert!(h.scheduler.view(id, Instant::now()).is_none());
}

#[tokio::test]
async fn changing_the_refresh_interval_keeps_the_generation_and_reschedules() {
    let h = Harness::new().await;
    h.start().await;
    let (id, _) = h.add_stub(StubProvider::new).await;
    h.pipeline_service
        .fetch_now(&[id], Duration::from_secs(5))
        .await;
    let before = h.provider_service.acquire(id).await.unwrap().generation;
    assert!(
        h.scheduler
            .view(id, Instant::now())
            .unwrap()
            .next_attempt_in
            > Duration::from_secs(20)
    );

    h.provider_service
        .update_provider_refresh_interval(id, 5)
        .await
        .unwrap();

    let after = h.provider_service.acquire(id).await.unwrap().generation;
    assert_eq!(before, after);
    assert!(
        h.scheduler
            .view(id, Instant::now())
            .unwrap()
            .next_attempt_in
            <= Duration::from_secs(5)
    );
    assert_eq!(
        h.repository
            .get_provider(id)
            .await
            .unwrap()
            .refresh_interval,
        5
    );
}

#[tokio::test]
async fn health_hears_about_every_provider_change() {
    let h = Harness::new().await;

    let id = h
        .provider_service
        .add_provider(h.config("added", 30))
        .await
        .unwrap();
    let added = h.health_inputs();
    let added_generation = h.provider_service.acquire(id).await.unwrap().generation;
    h.provider_service
        .update_provider(id, updated(&h, id))
        .await
        .unwrap();
    let updated_inputs = h.health_inputs();
    h.provider_service.load_all_providers().await.unwrap();
    let reloaded = h.health_inputs();
    let reloaded_generation = h.provider_service.acquire(id).await.unwrap().generation;
    h.provider_service.remove_provider(id).await.unwrap();
    let removed = h.health_inputs();

    let generations = |inputs: &[HealthInput]| match inputs {
        [HealthInput::ProvidersChanged { changed }] => changed
            .iter()
            .map(|input| (input.id, input.generation))
            .collect::<Vec<_>>(),
        other => panic!("unexpected health inputs: {other:?}"),
    };
    assert_eq!(generations(&added), [(id, Some(added_generation))]);
    let updated_generation = generations(&updated_inputs)[0].1;
    assert!(updated_generation.is_some());
    assert_ne!(updated_generation, Some(added_generation));
    assert_eq!(generations(&reloaded), [(id, Some(reloaded_generation))]);
    assert_ne!(Some(reloaded_generation), updated_generation);
    assert_eq!(generations(&removed), [(id, None)]);
    assert_eq!(h.bus.count("provider-added"), 1);
    assert_eq!(h.bus.count("provider-updated"), 1);
    assert_eq!(h.bus.count("provider-removed"), 1);
}

#[tokio::test]
async fn workflow_parameters_are_cached_for_ten_minutes() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    let workflow = format!("stub__{id}__web");

    let first = h
        .provider_service
        .get_workflow_parameters(&workflow)
        .await
        .unwrap();
    h.provider_service
        .get_workflow_parameters(&workflow)
        .await
        .unwrap();
    let cached_fetches = stub
        .parameter_fetches
        .load(std::sync::atomic::Ordering::SeqCst);
    sqlx::query("UPDATE workflow_parameters_cache SET cached_at = datetime('now', '-11 minutes')")
        .execute(&h.pool)
        .await
        .unwrap();
    h.provider_service
        .get_workflow_parameters(&workflow)
        .await
        .unwrap();

    assert_eq!(first.len(), 1);
    assert_eq!(cached_fetches, 1);
    assert_eq!(
        stub.parameter_fetches
            .load(std::sync::atomic::Ordering::SeqCst),
        2
    );
}

#[tokio::test]
async fn workflow_parameter_failures_keep_their_kind_and_leave_no_lock_behind() {
    let h = Harness::new().await;
    let (_, stub) = h.add_stub(StubProvider::new).await;
    *stub.parameters.lock().unwrap() = Err(DomainError::RateLimited {
        message: "slow".to_string(),
        retry_after: None,
    });

    let failed = h
        .provider_service
        .get_workflow_parameters("stub__1__web")
        .await;
    let missing = h
        .provider_service
        .get_workflow_parameters("stub__4242__web")
        .await;
    let malformed = h.provider_service.get_workflow_parameters("web").await;

    assert!(matches!(failed, Err(DomainError::RateLimited { .. })));
    assert!(matches!(missing, Err(DomainError::ProviderNotFound(_))));
    assert!(matches!(malformed, Err(DomainError::InvalidConfig(_))));
    assert!(h.provider_service.parameter_fetches.is_empty());
}

#[tokio::test]
async fn fetching_workflow_parameters_does_not_block_the_provider_map() {
    let h = Harness::new().await;
    let gate = Gate::new();
    h.add_stub(|id| StubProvider::new(id).parameter_gated(&gate))
        .await;
    let other = h.add_provider("other").await;
    let fetch = tokio::spawn({
        let service = Arc::clone(&h.provider_service);
        async move { service.get_workflow_parameters("stub__1__web").await }
    });
    gate.arrived().await;

    let acquired =
        tokio::time::timeout(Duration::from_secs(2), h.provider_service.acquire(other)).await;
    gate.open();

    assert!(acquired.is_ok());
    fetch.await.unwrap().unwrap();
}

#[tokio::test]
async fn workflow_parameters_fetched_across_a_removal_are_not_cached() {
    let h = Harness::new().await;
    let gate = Gate::new();
    let (id, _) = h
        .add_stub(|id| StubProvider::new(id).parameter_gated(&gate))
        .await;
    let workflow = format!("stub__{id}__web");
    let fetch = tokio::spawn({
        let service = Arc::clone(&h.provider_service);
        let workflow = workflow.clone();
        async move { service.get_workflow_parameters(&workflow).await }
    });
    gate.arrived().await;

    h.provider_service.remove_provider(id).await.unwrap();
    gate.open();

    assert_eq!(fetch.await.unwrap().unwrap().len(), 1);
    assert!(h
        .repository
        .get_cached_workflow_parameters(&workflow)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn field_options_work_when_the_plugin_rejects_the_partial_form() {
    let h = Harness::new().await;
    let config = HashMap::new();
    assert!(h
        .provider_service
        .init_plugin(STUB_OPTIONS, config.clone(), 0)
        .await
        .is_err());

    let options = h
        .provider_service
        .field_options(STUB_OPTIONS, "choice", config)
        .await
        .unwrap();

    assert_eq!(options, ["option-a", "option-b"]);
}

#[tokio::test]
async fn failed_field_options_report_why_the_plugin_could_not_initialize() {
    let h = Harness::new().await;

    let rejected = h
        .provider_service
        .field_options(STUB_OPTIONS, "broken", HashMap::new())
        .await;

    assert!(
        matches!(rejected, Err(DomainError::InvalidConfig(message)) if message.contains("initialize"))
    );
}

fn serve_tls(stream: std::net::TcpStream, config: Arc<rustls::ServerConfig>) {
    let Ok(connection) = rustls::ServerConnection::new(config) else {
        return;
    };
    let mut tls = rustls::StreamOwned::new(connection, stream);
    let mut request = Vec::new();
    let mut buffer = [0; 4096];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match tls.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(count) => request.extend_from_slice(&buffer[..count]),
        }
    }
    let body = r#"{"items":[]}"#;
    let _ = write!(
            tls,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    let _ = tls.flush();
    tls.conn.send_close_notify();
    let _ = tls.flush();
}

fn self_signed_server() -> String {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert = rustls::pki_types::CertificateDer::from(
        include_bytes!("../../../../tests/fixtures/self_signed_cert.der").to_vec(),
    );
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
        include_bytes!("../../../../tests/fixtures/self_signed_key.der")
            .to_vec()
            .into(),
    );
    let config = Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap(),
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let config = Arc::clone(&config);
            std::thread::spawn(move || serve_tls(stream, config));
        }
    });

    format!("https://{addr}")
}

fn http_settings(url: &str, insecure: bool) -> HashMap<String, String> {
    HashMap::from([
        ("server_url".to_string(), url.to_string()),
        ("token".to_string(), "token".to_string()),
        ("insecure".to_string(), insecure.to_string()),
    ])
}

#[tokio::test]
async fn insecure_tls_is_honored_by_add_validate_and_preview() {
    let h = Harness::with_real_plugins().await;
    let service = &h.provider_service;
    let url = self_signed_server();
    let page = Some(pipedash_plugin_api::PaginationParams {
        page: 1,
        page_size: 10,
    });
    let add = |insecure| {
        let mut config = h.config("tls", 30);
        config.provider_type = STUB_HTTP.to_string();
        config.config = http_settings(&url, insecure);
        config.config.remove("token");
        service.add_provider(config)
    };

    let id = add(true).await.unwrap();
    let insecure = service
        .init_plugin(STUB_HTTP, http_settings(&url, true), 0)
        .await
        .unwrap();
    let secure = service
        .init_plugin(STUB_HTTP, http_settings(&url, false), 0)
        .await
        .unwrap();
    let rejected = add(false).await.unwrap_err();

    assert!(id > 0);
    assert!(insecure.validate_credentials().await.unwrap());
    assert!(insecure
        .fetch_available_pipelines_filtered(None, None, page.clone())
        .await
        .unwrap()
        .items
        .is_empty());
    assert!(matches!(
        secure.validate_credentials().await,
        Err(PluginError::Tls(_))
    ));
    assert!(matches!(
        secure
            .fetch_available_pipelines_filtered(None, None, page)
            .await,
        Err(PluginError::Tls(_))
    ));
    assert!(matches!(rejected, DomainError::InvalidConfig(message) if message.contains("TLS")));
}

fn config_file(h: &Harness, providers: &[(&str, &str, i64)]) {
    let mut content = String::new();
    for (key, name, refresh_interval) in providers {
        content.push_str(&format!(
            "[providers.{key}]\nname = \"{name}\"\ntype = \"stub\"\ntoken = \"token-{key}\"\nrefresh_interval = {refresh_interval}\n\n"
        ));
    }
    std::fs::write(h.config_state.config_path(), content).unwrap();
}

async fn provider_id(h: &Harness, key: &str) -> Option<i64> {
    h.repository
        .config_backend()
        .list_providers()
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.name == key)
        .and_then(|p| p.id)
}

#[tokio::test]
async fn removal_preserves_the_provider_and_toml_when_token_deletion_fails() {
    let h = Harness::new().await;
    config_file(&h, &[("ci-a", "CI A", 30)]);
    h.provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();
    let id = provider_id(&h, "ci-a").await.unwrap();
    h.token_store.fail_next_delete();

    assert!(h.provider_service.remove_provider(id).await.is_err());
    assert!(h.repository.get_provider(id).await.is_ok());
    assert!(h
        .config_state
        .load_providers()
        .await
        .unwrap()
        .contains_key("ci-a"));

    h.provider_service.remove_provider(id).await.unwrap();
    h.config_state.reload().await.unwrap();
    h.config_state
        .sync_providers_from_toml(h.repository.config_backend().as_ref(), false)
        .await
        .unwrap();
    assert!(h.repository.list_providers().await.unwrap().is_empty());
    assert!(h.token_store.get_all_tokens().await.unwrap().is_empty());
}

#[tokio::test]
async fn removal_awaits_token_cleanup_before_deleting_the_provider() {
    let h = Harness::new().await;
    let id = h.add_provider("await-delete").await;
    let gate = Gate::new();
    h.token_store.gate_delete(Arc::clone(&gate));
    let remove = tokio::spawn({
        let service = Arc::clone(&h.provider_service);
        async move { service.remove_provider(id).await }
    });
    gate.arrived().await;

    assert!(!remove.is_finished());
    assert!(h.repository.get_provider(id).await.is_ok());

    gate.open();
    remove.await.unwrap().unwrap();
    assert!(h.repository.get_provider(id).await.is_err());
    assert!(h.token_store.get_all_tokens().await.unwrap().is_empty());
}

#[tokio::test]
async fn removing_a_provider_without_a_token_entry_works() {
    let h = Harness::new().await;
    let id = h
        .repository
        .config_backend()
        .create_provider(&h.config("tokenless", 30))
        .await
        .unwrap();

    h.provider_service.remove_provider(id).await.unwrap();

    assert!(h.repository.get_provider(id).await.is_err());
}

#[tokio::test]
async fn a_failed_final_deletion_keeps_the_token_and_a_retry_removes_it() {
    let h = Harness::new().await;
    let id = h.add_provider("final-delete").await;
    execute(
        &h,
        "CREATE TRIGGER block_delete BEFORE DELETE ON providers \
             BEGIN SELECT RAISE(ABORT, 'delete blocked'); END",
    )
    .await;

    assert!(h.provider_service.remove_provider(id).await.is_err());
    assert!(h.repository.get_provider(id).await.is_ok());
    assert_eq!(h.token_store.get_token(id).await.unwrap(), "token");

    execute(&h, "DROP TRIGGER block_delete").await;
    h.provider_service.remove_provider(id).await.unwrap();
    assert!(h.repository.get_provider(id).await.is_err());
    assert!(h.token_store.get_all_tokens().await.unwrap().is_empty());
}

#[tokio::test]
async fn failed_config_deletion_keeps_the_provider_available_for_retry() {
    let h = Harness::new().await;
    config_file(&h, &[("ci-a", "CI A", 30)]);
    h.provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();
    let id = provider_id(&h, "ci-a").await.unwrap();
    let token = h.token_store.get_token(id).await.unwrap();
    assert_eq!(token, "token-ci-a");
    let path = h.config_state.config_path();
    std::fs::remove_file(path).unwrap();
    std::fs::create_dir(path).unwrap();

    assert!(h.provider_service.remove_provider(id).await.is_err());
    assert!(h.repository.get_provider(id).await.is_ok());
    assert_eq!(h.token_store.get_token(id).await.unwrap(), token);

    std::fs::remove_dir(path).unwrap();
    config_file(&h, &[("ci-a", "CI A", 30)]);
    h.provider_service.remove_provider(id).await.unwrap();
    assert!(h.repository.get_provider(id).await.is_err());
    assert!(h.config_state.load_providers().await.unwrap().is_empty());
    assert!(h.token_store.get_all_tokens().await.unwrap().is_empty());
}

#[tokio::test]
async fn renaming_a_toml_provider_keeps_the_file_key_for_removal() {
    let h = Harness::new().await;
    config_file(&h, &[("ci-a", "CI A", 30)]);
    h.provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();
    let id = provider_id(&h, "ci-a").await.unwrap();
    let mut renamed = h.repository.get_provider(id).await.unwrap();
    renamed.name = "Renamed CI".to_string();

    h.provider_service
        .update_provider(id, renamed)
        .await
        .unwrap();
    let saved = h.repository.get_provider(id).await.unwrap();
    assert_eq!(
        (saved.name.as_str(), saved.display_name()),
        ("ci-a", "Renamed CI")
    );

    h.provider_service.remove_provider(id).await.unwrap();
    h.config_state.reload().await.unwrap();
    h.config_state
        .sync_providers_from_toml(h.repository.config_backend().as_ref(), false)
        .await
        .unwrap();
    assert!(h.repository.list_providers().await.unwrap().is_empty());
}

#[tokio::test]
async fn test_apply_config_file_adds_and_updates_changed_blocks_only() {
    let h = Harness::new().await;
    config_file(&h, &[("ci-a", "CI A", 30), ("ci-b", "CI B", 30)]);

    let first = h
        .provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();

    assert_eq!(first.added, ["ci-a", "ci-b"]);
    let a = provider_id(&h, "ci-a").await.unwrap();
    let b = provider_id(&h, "ci-b").await.unwrap();
    let loaded = h.provider_service.generation_of(a).await.unwrap();
    assert!(h.provider_service.generation_of(b).await.is_some());
    assert!(h
        .bus
        .events()
        .iter()
        .any(|e| matches!(e, CoreEvent::ProvidersChanged)));

    h.provider_service
        .update_provider_refresh_interval(b, 120)
        .await
        .unwrap();
    config_file(
        &h,
        &[
            ("ci-a", "CI Renamed", 30),
            ("ci-b", "CI B", 30),
            ("ci-c", "CI C", 60),
        ],
    );

    let second = h
        .provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();

    assert_eq!(second.added, ["ci-c"]);
    assert_eq!(second.updated, ["ci-a"]);
    let c = provider_id(&h, "ci-c").await.unwrap();
    assert_eq!(
        h.repository.get_provider(c).await.unwrap().token,
        "token-ci-c"
    );
    assert!(h.provider_service.generation_of(c).await.is_some());
    assert_eq!(
        h.repository.get_provider(a).await.unwrap().display_name(),
        "CI Renamed"
    );
    assert_ne!(h.provider_service.generation_of(a).await, Some(loaded));
    assert_eq!(
        h.repository.get_provider(b).await.unwrap().refresh_interval,
        120
    );

    let unchanged = h
        .provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();
    assert!(unchanged.added.is_empty() && unchanged.updated.is_empty());
}

#[tokio::test]
async fn test_apply_config_file_rejects_an_invalid_file_and_keeps_providers() {
    let h = Harness::new().await;
    config_file(&h, &[("ci-a", "CI A", 30)]);
    h.provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();
    let a = provider_id(&h, "ci-a").await.unwrap();

    let write = |content: &str| std::fs::write(h.config_state.config_path(), content).unwrap();

    write("[providers.ci-a]\nname = \"CI Broken\"\ntype = \"\"\n");
    let empty = h.provider_service.apply_config_file(&h.config_state).await;
    write("[providers.ci-a]\nname = \"CI Broken\"\ntype = \"unknown\"\n");
    let unknown = h.provider_service.apply_config_file(&h.config_state).await;

    assert!(matches!(empty, Err(DomainError::InvalidConfig(_))));
    assert!(matches!(unknown, Err(DomainError::InvalidProviderType(_))));

    assert_eq!(
        h.repository.get_provider(a).await.unwrap().display_name(),
        "CI A"
    );
    assert_eq!(
        h.config_state.get().await.providers["ci-a"].provider_type,
        "stub"
    );
}

async fn execute(h: &Harness, sql: &'static str) {
    sqlx::query(sql).execute(&h.pool).await.unwrap();
}

#[tokio::test]
async fn test_apply_config_file_retry_replaces_providers_saved_by_a_failed_apply() {
    let h = Harness::new().await;
    config_file(&h, &[("ci-a", "CI A", 30), ("ci-b", "CI B", 30)]);
    h.provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();
    let a = provider_id(&h, "ci-a").await.unwrap();
    let b = provider_id(&h, "ci-b").await.unwrap();
    let loaded_a = h.provider_service.generation_of(a).await.unwrap();
    let loaded_b = h.provider_service.generation_of(b).await.unwrap();
    let announced = h.bus.count("providers-changed");

    execute(
        &h,
        "CREATE TRIGGER fail_ci_b BEFORE UPDATE ON providers WHEN NEW.name = 'ci-b' \
             BEGIN SELECT RAISE(ABORT, 'write failed'); END",
    )
    .await;
    config_file(&h, &[("ci-a", "CI A2", 30), ("ci-b", "CI B2", 30)]);
    assert!(h
        .provider_service
        .apply_config_file(&h.config_state)
        .await
        .is_err());
    assert_eq!(
        h.repository.get_provider(a).await.unwrap().display_name(),
        "CI A2"
    );
    assert_eq!(h.bus.count("providers-changed"), announced + 1);

    execute(&h, "DROP TRIGGER fail_ci_b").await;
    h.provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();

    assert_eq!(
        h.repository.get_provider(b).await.unwrap().display_name(),
        "CI B2"
    );
    assert_eq!(h.bus.count("providers-changed"), announced + 2);
    assert_ne!(h.provider_service.generation_of(a).await, Some(loaded_a));
    assert_ne!(h.provider_service.generation_of(b).await, Some(loaded_b));
    assert_eq!(
        h.config_state.get().await.providers["ci-b"].name.as_deref(),
        Some("CI B2")
    );
}

#[tokio::test]
async fn test_apply_config_file_stores_an_empty_token_for_a_new_provider_without_one() {
    let h = Harness::new().await;
    config_file(&h, &[("ci-a", "CI A", 30)]);
    let path = h.config_state.config_path();
    let unset = std::fs::read_to_string(path)
        .unwrap()
        .replace("token-ci-a", "${PIPEDASH_TOKEN_UNSET_IN_TESTS:-}");
    std::fs::write(path, unset).unwrap();

    h.provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();

    let a = provider_id(&h, "ci-a").await.unwrap();
    assert_eq!(h.token_store.get_token(a).await.unwrap(), "");
}

#[tokio::test]
async fn test_apply_config_file_retries_a_token_the_store_failed_to_save() {
    let h = Harness::new().await;
    config_file(&h, &[("ci-a", "CI A", 30)]);
    h.provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();
    let a = provider_id(&h, "ci-a").await.unwrap();
    let path = h.config_state.config_path();
    let rotated = std::fs::read_to_string(path)
        .unwrap()
        .replace("token-ci-a", "rotated");
    std::fs::write(path, rotated).unwrap();

    h.token_store.fail_next_store();
    assert!(h
        .provider_service
        .apply_config_file(&h.config_state)
        .await
        .is_err());
    assert_eq!(h.stubs.token_of(a).as_deref(), Some("token-ci-a"));

    h.provider_service
        .apply_config_file(&h.config_state)
        .await
        .unwrap();

    assert_eq!(h.stubs.token_of(a).as_deref(), Some("rotated"));
    assert_eq!(
        h.config_state.get().await.providers["ci-a"].token,
        "rotated"
    );
}

#[tokio::test]
async fn test_config_watcher_retries_a_failed_apply_without_a_new_edit() {
    let h = Harness::new().await;
    let watcher = crate::application::ConfigWatcher::new(
        Arc::clone(&h.provider_service),
        Arc::clone(&h.config_state),
    )
    .with_poll(Duration::from_millis(20));
    watcher.start().await;

    execute(
        &h,
        "CREATE TRIGGER fail_insert BEFORE INSERT ON providers \
             BEGIN SELECT RAISE(ABORT, 'write failed'); END",
    )
    .await;
    config_file(&h, &[("ci-a", "CI A", 30)]);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(provider_id(&h, "ci-a").await, None);
    execute(&h, "DROP TRIGGER fail_insert").await;

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let id = loop {
        if let Some(id) = provider_id(&h, "ci-a").await {
            break id;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the watcher never retried the file"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    watcher.stop().await;

    assert!(h.provider_service.generation_of(id).await.is_some());
}

fn sign_in(token_endpoint: String, expires_in: chrono::Duration) -> SignInCredential {
    SignInCredential {
        token: "bearer".into(),
        refresh_token: Some("r1".into()),
        expires_at: Some(Utc::now() + expires_in),
        token_endpoint,
        client_id: "cli".into(),
        kind: pipedash_plugin_api::SignInToken::AccessToken,
        account: Some("dev@example.com".into()),
    }
}

async fn stored_sign_in(h: &Harness, id: i64) -> SignInCredential {
    SignInCredential::decode(&h.repository.get_provider(id).await.unwrap().token).unwrap()
}

async fn recovery_provider(h: &Harness) -> (i64, Idp, FeedStub) {
    let idp = Idp::start().await;
    idp.reply(
            200,
            serde_json::json!({
                "access_token": "fresh-access",
                "id_token": jwt(serde_json::json!({"email": "new@example.com", "exp": Utc::now().timestamp() + 3600})),
                "refresh_token": "fresh-refresh",
            }),
        );
    let server = FeedStub::start().await;
    server.respond_json(serde_json::json!({"issuer": idp.base_url, "client_id": "cli"}));
    let mut config = h.config("sso", 30);
    config.provider_type = STUB_OIDC.to_string();
    config
        .config
        .insert("server_url".into(), server.base_url.clone());
    config.token = sign_in(idp.token_endpoint(), chrono::Duration::minutes(30))
        .encode()
        .unwrap();
    let id = h.repository.add_provider(&config).await.unwrap();
    (id, idp, server)
}

async fn oidc_provider(h: &Harness, name: &str) -> i64 {
    let mut config = h.config(name, 30);
    config.provider_type = STUB_OIDC.to_string();
    h.repository.add_provider(&config).await.unwrap()
}

async fn complete_login(h: &Harness, start: &SignInStart) {
    let authorize = reqwest::Url::parse(&start.authorize_url).unwrap();
    let query: HashMap<_, _> = authorize.query_pairs().into_owned().collect();
    let mut callback = reqwest::Url::parse(&query["redirect_uri"]).unwrap();
    callback
        .query_pairs_mut()
        .append_pair("code", "code")
        .append_pair("state", &query["state"]);
    assert!(matches!(
        h.provider_service
            .complete_sign_in(&start.id, callback.as_str())
            .await
            .unwrap(),
        SignInStatus::Completed { .. }
    ));
}

#[tokio::test]
async fn provider_sign_in_starts_without_renewing_the_expired_credentials() {
    let h = Harness::new().await;
    let (id, idp, _server) = recovery_provider(&h).await;
    let expired = sign_in(idp.token_endpoint(), chrono::Duration::minutes(-1));
    h.repository
        .store_token(id, &expired.encode().unwrap())
        .await
        .unwrap();
    idp.reply(400, serde_json::json!({"error": "invalid_grant"}));

    let start = h.provider_service.start_provider_sign_in(id).await.unwrap();

    assert!(idp.forms.lock().unwrap().is_empty());
    assert_eq!(stored_sign_in(&h, id).await, expired);
    h.provider_service.cancel_sign_in(&start.id).await;
}

#[tokio::test]
async fn reconnect_preserves_concurrent_configuration_and_queues_a_fetch() {
    let h = Harness::new().await;
    let (id, _idp, server) = recovery_provider(&h).await;
    h.stubs.queue(
        StubProvider::new(id).failing(DomainError::ProviderAuthenticationFailed("expired".into())),
    );
    h.fetch_directly(id).await;
    let generation = h.provider_service.generation_of(id).await.unwrap();
    let start = h.provider_service.start_provider_sign_in(id).await.unwrap();
    complete_login(&h, &start).await;
    let settings = Gate::new();
    server.set_gate(Some(&settings));
    let fetch = Gate::new();
    h.stubs.queue(StubProvider::new(id).gated(&fetch));
    h.start().await;
    let reconnect = tokio::spawn({
        let service = Arc::clone(&h.provider_service);
        let flow = start.id.clone();
        async move { service.reconnect_provider(id, &flow).await }
    });
    settings.arrived().await;
    let lock = h.provider_service.commit_lock(id);
    let guard = lock.lock().await;
    let mut edited = h.repository.get_provider(id).await.unwrap();
    edited.name = "Renamed while signing in".into();
    edited.refresh_interval = 90;
    edited
        .config
        .insert("selected_items".into(), "new-app".into());
    h.repository.update_provider(id, &edited).await.unwrap();
    let edited = h.repository.get_provider(id).await.unwrap();
    drop(guard);
    settings.open();
    reconnect.await.unwrap().unwrap();
    fetch.arrived().await;

    let saved = h.repository.get_provider(id).await.unwrap();
    assert_eq!(saved.name, edited.name);
    assert_eq!(saved.refresh_interval, edited.refresh_interval);
    assert_eq!(saved.config, edited.config);
    assert_eq!(saved.version, edited.version);
    assert_eq!(
        stored_sign_in(&h, id).await.account.as_deref(),
        Some("new@example.com")
    );
    assert_eq!(
        stored_sign_in(&h, id).await.refresh_token.as_deref(),
        Some("fresh-refresh")
    );
    assert_ne!(
        h.provider_service.generation_of(id).await.unwrap(),
        generation
    );
    let schedule = h.scheduler.view(id, Instant::now()).unwrap();
    assert_eq!(
        (schedule.failures, schedule.last_failure, schedule.in_flight),
        (0, None, true)
    );
    assert_eq!(h.bus.count("provider-updated"), 1);
    assert!(h.bus.events().iter().any(|event| matches!(
        event,
        CoreEvent::ProviderUpdated { provider, .. }
            if matches!(provider.last_fetch_status, FetchStatus::Never)
                && provider.last_fetch_error.is_none()
                && provider.recovery.is_none()
    )));
    assert_eq!(
        h.provider_service.sign_in_status(&start.id).await.unwrap(),
        SignInStatus::Expired
    );
    fetch.open();
    h.bus
        .wait_for("the reconnected provider fetch", |events| {
            events
                .iter()
                .any(|event| matches!(event, CoreEvent::PipelinesUpdated { .. }))
        })
        .await;
}

#[tokio::test]
async fn reconnect_retains_the_flow_and_runtime_when_token_persistence_fails() {
    let h = Harness::new().await;
    let (id, _idp, _server) = recovery_provider(&h).await;
    let original = h.repository.get_provider(id).await.unwrap();
    let lease = h.provider_service.acquire(id).await.unwrap();
    let start = h.provider_service.start_provider_sign_in(id).await.unwrap();
    complete_login(&h, &start).await;
    h.token_store.fail_next_store();

    assert!(h
        .provider_service
        .reconnect_provider(id, &start.id)
        .await
        .is_err());

    assert_eq!(
        h.repository.get_provider(id).await.unwrap().token,
        original.token
    );
    assert_eq!(
        h.provider_service.generation_of(id).await,
        Some(lease.generation)
    );
    assert_eq!(h.bus.count("provider-updated"), 0);
    assert!(matches!(
        h.provider_service.sign_in_status(&start.id).await.unwrap(),
        SignInStatus::Completed { .. }
    ));
    h.provider_service
        .reconnect_provider(id, &start.id)
        .await
        .unwrap();
    assert_eq!(h.bus.count("provider-updated"), 1);
    assert_eq!(
        h.provider_service.sign_in_status(&start.id).await.unwrap(),
        SignInStatus::Expired
    );
}

#[tokio::test]
async fn reconnect_rejects_other_providers_setup_flows_and_deleted_providers() {
    let h = Harness::new().await;
    let (id, _idp, _server) = recovery_provider(&h).await;
    let other = h.add_provider("other").await;
    let start = h.provider_service.start_provider_sign_in(id).await.unwrap();
    complete_login(&h, &start).await;

    assert!(matches!(
        h.provider_service
            .reconnect_provider(other, &start.id)
            .await,
        Err(DomainError::InvalidConfig(_))
    ));
    let mut stolen = h.repository.get_provider(other).await.unwrap();
    stolen
        .config
        .insert(SIGN_IN_CONFIG_KEY.into(), start.id.clone());
    assert!(matches!(
        h.provider_service.update_provider(other, stolen).await,
        Err(DomainError::InvalidConfig(_))
    ));
    let config = h.repository.get_provider(id).await.unwrap();
    let setup = h
        .provider_service
        .start_sign_in(&config.provider_type, config.config)
        .await
        .unwrap();
    complete_login(&h, &setup).await;
    assert!(matches!(
        h.provider_service.reconnect_provider(id, &setup.id).await,
        Err(DomainError::InvalidConfig(_))
    ));
    h.provider_service.remove_provider(id).await.unwrap();
    assert!(matches!(
        h.provider_service.reconnect_provider(id, &start.id).await,
        Err(DomainError::ProviderNotFound(_))
    ));
    assert_eq!(h.bus.count("provider-updated"), 0);
    assert_eq!(
        h.repository.get_provider(other).await.unwrap().token,
        "token"
    );
}

#[tokio::test]
async fn deleting_during_reconnect_does_not_restore_the_provider() {
    let h = Harness::new().await;
    let (id, _idp, server) = recovery_provider(&h).await;
    let start = h.provider_service.start_provider_sign_in(id).await.unwrap();
    complete_login(&h, &start).await;
    let settings = Gate::new();
    server.set_gate(Some(&settings));
    let reconnect = tokio::spawn({
        let service = Arc::clone(&h.provider_service);
        async move { service.reconnect_provider(id, &start.id).await }
    });
    settings.arrived().await;

    h.provider_service.remove_provider(id).await.unwrap();
    settings.open();

    assert!(matches!(
        reconnect.await.unwrap(),
        Err(DomainError::ProviderNotFound(_))
    ));
    assert!(h.repository.get_provider(id).await.is_err());
    assert!(h.provider_service.generation_of(id).await.is_none());
    assert!(h.scheduler.view(id, Instant::now()).is_none());
    assert_eq!(h.bus.count("provider-updated"), 0);
}

#[tokio::test]
async fn reconnect_rejects_replaced_credentials_and_changed_sign_in_settings() {
    let h = Harness::new().await;
    let (id, _idp, server) = recovery_provider(&h).await;
    let original = h.repository.get_provider(id).await.unwrap();
    let start = h.provider_service.start_provider_sign_in(id).await.unwrap();
    complete_login(&h, &start).await;
    h.repository
        .store_token(id, "changed-by-another-client")
        .await
        .unwrap();

    assert!(matches!(
        h.provider_service.reconnect_provider(id, &start.id).await,
        Err(DomainError::ConcurrentModification(_))
    ));
    assert_eq!(
        h.repository.get_provider(id).await.unwrap().token,
        "changed-by-another-client"
    );
    h.repository.store_token(id, &original.token).await.unwrap();
    server.respond_json(
        serde_json::json!({"issuer": "https://different.example.com", "client_id": "different"}),
    );
    assert!(matches!(
        h.provider_service.reconnect_provider(id, &start.id).await,
        Err(DomainError::ConcurrentModification(_))
    ));
    assert_eq!(
        h.repository.get_provider(id).await.unwrap().token,
        original.token
    );
    assert_eq!(h.bus.count("provider-updated"), 0);
}

#[tokio::test]
async fn concurrent_reconnects_commit_a_completed_flow_only_once() {
    let h = Harness::new().await;
    let (id, _idp, _server) = recovery_provider(&h).await;
    let start = h.provider_service.start_provider_sign_in(id).await.unwrap();
    complete_login(&h, &start).await;

    let (first, second) = tokio::join!(
        h.provider_service.reconnect_provider(id, &start.id),
        h.provider_service.reconnect_provider(id, &start.id),
    );

    assert_ne!(first.is_ok(), second.is_ok());
    assert_eq!(h.bus.count("provider-updated"), 1);
    assert_eq!(
        h.provider_service.sign_in_status(&start.id).await.unwrap(),
        SignInStatus::Expired
    );
}

#[tokio::test]
async fn reconnect_requires_completion_and_cancel_discards_completed_recovery() {
    let h = Harness::new().await;
    let (id, _idp, _server) = recovery_provider(&h).await;
    let original = h.repository.get_provider(id).await.unwrap().token;
    let start = h.provider_service.start_provider_sign_in(id).await.unwrap();

    assert!(matches!(
        h.provider_service.reconnect_provider(id, &start.id).await,
        Err(DomainError::InvalidConfig(_))
    ));
    complete_login(&h, &start).await;
    h.provider_service.cancel_sign_in(&start.id).await;
    assert_eq!(
        h.provider_service.sign_in_status(&start.id).await.unwrap(),
        SignInStatus::Expired
    );
    assert!(h
        .provider_service
        .reconnect_provider(id, &start.id)
        .await
        .is_err());
    assert_eq!(h.repository.get_provider(id).await.unwrap().token, original);
    assert_eq!(h.bus.count("provider-updated"), 0);
}

#[tokio::test]
async fn renew_is_scheduled_a_minute_before_the_sign_in_expires() {
    let h = Harness::with_real_plugins().await;
    let id = h.add_provider("sso").await;
    let credential = sign_in(
        "http://127.0.0.1:9/token".into(),
        chrono::Duration::minutes(30),
    );
    h.repository
        .store_token(id, &credential.encode().unwrap())
        .await
        .unwrap();

    let lease = h.provider_service.acquire(id).await.unwrap();

    assert_eq!(
        lease.provider.renew_at(),
        Some(credential.expires_at.unwrap() - chrono::Duration::seconds(60))
    );
}

#[tokio::test]
async fn renew_refreshes_and_stores_an_expired_sign_in() {
    let h = Harness::new().await;
    let idp = Idp::start().await;
    idp.reply(
        200,
        serde_json::json!({"access_token": "new", "refresh_token": "r2", "expires_in": 3600}),
    );
    let (id, _) = h.add_stub(StubProvider::new).await;
    let expired = sign_in(idp.token_endpoint(), chrono::Duration::minutes(-1));
    h.repository
        .store_token(id, &expired.encode().unwrap())
        .await
        .unwrap();

    h.provider_service.acquire(id).await.unwrap();

    let renewed = stored_sign_in(&h, id).await;
    assert_eq!(renewed.token, "new");
    assert_eq!(renewed.refresh_token.as_deref(), Some("r2"));
    assert_eq!(renewed.account.as_deref(), Some("dev@example.com"));
}

#[tokio::test]
async fn renew_with_a_revoked_sign_in_asks_to_sign_in_again() {
    let h = Harness::new().await;
    let idp = Idp::start().await;
    idp.reply(400, serde_json::json!({"error": "invalid_grant"}));
    let (id, _) = h.add_stub(StubProvider::new).await;
    let expired = sign_in(idp.token_endpoint(), chrono::Duration::minutes(-1));
    h.repository
        .store_token(id, &expired.encode().unwrap())
        .await
        .unwrap();

    let error = h.provider_service.acquire(id).await.err().unwrap();

    assert!(
        matches!(error, DomainError::ProviderAuthenticationFailed(m) if m == crate::domain::SIGN_IN_EXPIRED)
    );
}

#[tokio::test]
async fn stored_refresh_rejections_ask_to_sign_in_again() {
    let h = Harness::new().await;
    let idp = Idp::start().await;
    let id = oidc_provider(&h, "sso").await;
    let expired = sign_in(idp.token_endpoint(), chrono::Duration::minutes(-1));
    h.repository
        .store_token(id, &expired.encode().unwrap())
        .await
        .unwrap();

    for description in [
        "Refresh token is invalid or has already been claimed by another client.",
        "Refresh token expired.",
    ] {
        idp.reply(
            400,
            serde_json::json!({"error": "invalid_request", "error_description": description}),
        );

        let error = h.provider_service.acquire(id).await.err().unwrap();

        assert!(matches!(
            error,
            DomainError::ProviderAuthenticationFailed(_)
        ));
        assert_eq!(
            ProviderRecovery::for_failure(Some(error.failure_kind()), true),
            Some(ProviderRecovery::SignIn)
        );
        assert_eq!(stored_sign_in(&h, id).await, expired);
    }
}

#[tokio::test]
async fn unknown_invalid_request_stays_an_api_error() {
    let h = Harness::new().await;
    let idp = Idp::start().await;
    idp.reply(
        400,
        serde_json::json!({"error": "invalid_request", "error_description": "Unknown request"}),
    );
    let id = oidc_provider(&h, "sso").await;
    let expired = sign_in(idp.token_endpoint(), chrono::Duration::minutes(-1));
    h.repository
        .store_token(id, &expired.encode().unwrap())
        .await
        .unwrap();

    let error = h.provider_service.acquire(id).await.err().unwrap();

    assert!(matches!(error, DomainError::ApiError(_)));
}

#[tokio::test]
async fn invalid_request_server_errors_stay_transient() {
    let h = Harness::new().await;
    let idp = Idp::start().await;
    idp.reply(
        503,
        serde_json::json!({
            "error": "invalid_request",
            "error_description": "Refresh token expired.",
        }),
    );
    let id = oidc_provider(&h, "sso").await;
    let expired = sign_in(idp.token_endpoint(), chrono::Duration::minutes(-1));
    h.repository
        .store_token(id, &expired.encode().unwrap())
        .await
        .unwrap();

    let error = h.provider_service.acquire(id).await.err().unwrap();

    assert!(matches!(error, DomainError::ServiceUnavailable { .. }));
}

#[tokio::test]
async fn editing_a_signed_in_provider_keeps_its_session() {
    let h = Harness::with_real_plugins().await;
    let id = h.add_provider("sso").await;
    let credential = sign_in(
        "http://127.0.0.1:9/token".into(),
        chrono::Duration::minutes(30),
    );
    h.repository
        .store_token(id, &credential.encode().unwrap())
        .await
        .unwrap();

    let editing = h.provider_service.provider_for_editing(id).await.unwrap();
    assert_eq!(editing.token, "");
    assert_eq!(
        editing.config.get(SIGN_IN_CONFIG_KEY),
        Some(&format!("provider:{id}"))
    );

    h.provider_service
        .update_provider(id, editing)
        .await
        .unwrap();

    assert_eq!(stored_sign_in(&h, id).await, credential);
    let other = h.add_provider("other").await;
    let mut stolen = h
        .provider_service
        .provider_for_editing(other)
        .await
        .unwrap();
    stolen
        .config
        .insert(SIGN_IN_CONFIG_KEY.into(), format!("provider:{id}"));
    assert!(matches!(
        h.provider_service.update_provider(other, stolen).await,
        Err(DomainError::InvalidConfig(_))
    ));
}

#[tokio::test]
async fn editing_during_a_renewal_keeps_the_renewed_session() {
    let h = Harness::with_real_plugins().await;
    let server = FeedStub::start().await;
    server.respond_json(serde_json::json!({ "items": [] }));
    let original = sign_in(
        "http://127.0.0.1:9/token".into(),
        chrono::Duration::minutes(30),
    );
    let mut config = h.config("sso", 30);
    config.provider_type = STUB_OIDC.to_string();
    config
        .config
        .insert("server_url".into(), server.base_url.clone());
    config.token = original.encode().unwrap();
    let id = h.repository.add_provider(&config).await.unwrap();
    let editing = h.provider_service.provider_for_editing(id).await.unwrap();

    let checks = Gate::new();
    server.set_gate(Some(&checks));
    let edit = tokio::spawn({
        let service = Arc::clone(&h.provider_service);
        async move { service.update_provider(id, editing).await }
    });
    checks.arrived().await;
    let renewed = SignInCredential {
        token: "renewed".into(),
        refresh_token: Some("r2".into()),
        expires_at: Some(Utc::now() + chrono::Duration::minutes(45)),
        ..original.clone()
    };
    h.repository
        .store_token(id, &renewed.encode().unwrap())
        .await
        .unwrap();
    checks.open();
    edit.await.unwrap().unwrap();

    assert_eq!(stored_sign_in(&h, id).await, renewed);
    assert_eq!(
        h.provider_service
            .acquire(id)
            .await
            .unwrap()
            .provider
            .renew_at(),
        renewed.renew_at()
    );
}
