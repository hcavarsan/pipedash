use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};
use std::sync::Arc;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use futures::stream::{
    FuturesUnordered,
    StreamExt,
};
use pipedash_plugin_api::StatusSource;
use reqwest::Client;
use tokio::sync::Semaphore;
use tokio::time::{
    timeout,
    timeout_at,
    Instant,
};

use super::super::alerts::{
    connection_recovery,
    connectivity,
    feed_candidates,
    feed_recovery,
    instance_candidates,
    instance_recovery,
    merge,
    Connectivity,
    Observation,
    CALM_CHECKS,
    CALM_FETCHES,
};
use super::super::feed::{
    fetch_feed,
    FeedOutcome,
};
use super::super::{
    check_instance,
    connection_subject,
    feed_key,
    feed_subject,
    instance_subject,
    CheckOutcome,
    FeedKey,
    FEED_SUBJECT_PREFIX,
};
use super::{
    LocalNotification,
    NotificationState,
};
use crate::application::services::pipeline_service::{
    overdue_run,
    tracks_overdue,
    FETCH_TIMEOUT,
    FETCH_TIMEOUT_MESSAGE,
    OVERDUE_CONCURRENT_CHECKS,
    OVERDUE_RECENT_RUNS,
};
use crate::application::services::provider_service::{
    controller_health,
    plugin_provider,
    quota_pause,
    ProviderFactory,
};
use crate::domain::{
    DomainError,
    DomainResult,
    FailureKind,
    Pipeline,
    PipelineStatus,
    Provider,
    ProviderConfig,
};
use crate::infrastructure::status_feeds::{
    evaluate,
    ParsedFeed,
};
use crate::infrastructure::HttpClientManager;
use crate::plugins::create_plugin_registry;

const SCAN_CONCURRENCY: usize = 4;
const RATE_LIMIT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_PAUSE: Duration = Duration::from_secs(60 * 60);
const MAX_OVERDUE_CHECKS: usize = 25;
const OVERDUE_BUDGET: Duration = Duration::from_secs(20);

enum Polled {
    Paused(DateTime<Utc>),
    Fetched {
        pipelines: Vec<Pipeline>,
        health: Option<CheckOutcome>,
    },
    Failed {
        error: DomainError,
        health: Option<CheckOutcome>,
    },
}

struct Target {
    id: i64,
    provider: Arc<dyn Provider>,
    controller: bool,
}

struct Overdue {
    id: i64,
    provider: Arc<dyn Provider>,
    selected: Vec<(Pipeline, bool)>,
}

enum Job {
    Feed(FeedKey),
    Poll(Target),
    Overdue(Overdue),
}

enum Done {
    Feed {
        key: FeedKey,
        feed: Option<Box<ParsedFeed>>,
    },
    Poll {
        target: Target,
        polled: Polled,
    },
    Overdue {
        id: i64,
        checked: Vec<(Pipeline, bool, Option<String>)>,
    },
}

pub async fn refresh_notifications(
    providers: &[ProviderConfig], proxy_url: &str, status_pages_enabled: bool,
    state: &mut NotificationState, deadline: Duration,
) -> DomainResult<Vec<LocalNotification>> {
    let http = Arc::new(HttpClientManager::new()?);
    http.set_proxy(proxy_url)?;
    let registry = Arc::new(create_plugin_registry());
    let build: ProviderFactory = {
        let registry = Arc::clone(&registry);
        let http = Arc::clone(&http);
        Arc::new(move |config| plugin_provider(&registry, &http, config))
    };
    let controller = move |provider_type: &str| controller_health(&registry, provider_type);

    scan(
        providers,
        &build,
        &controller,
        &http,
        status_pages_enabled,
        state,
        deadline,
    )
    .await
}

pub(super) async fn scan(
    providers: &[ProviderConfig], build: &ProviderFactory,
    controller: &(dyn Fn(&str) -> bool + Sync), http: &HttpClientManager,
    status_pages_enabled: bool, state: &mut NotificationState, deadline: Duration,
) -> DomainResult<Vec<LocalNotification>> {
    let end = Instant::now() + deadline;
    let wall = Utc::now();
    let ids: HashSet<i64> = providers.iter().filter_map(|config| config.id).collect();
    state.retain_providers(&ids);
    if !status_pages_enabled {
        state.drop_feeds();
    }

    let mut seen = HashSet::new();
    let mut configs: Vec<(i64, &ProviderConfig)> = providers
        .iter()
        .filter_map(|config| Some((config.id?, config)))
        .filter(|(id, _)| seen.insert(*id))
        .collect();
    configs.sort_by_key(|(id, _)| (state.attempted_at(*id), *id));

    let mut found = Vec::new();
    let mut targets = Vec::new();
    let mut feeds: HashMap<FeedKey, Vec<(i64, StatusSource)>> = HashMap::new();
    for (id, config) in configs {
        state.enroll(id, config.display_name(), Some(config.version.unwrap_or(1)));
        let provider = match build(config) {
            Ok(provider) => provider,
            Err(error) => {
                state.stamp(id, wall);
                let polled = Polled::Failed {
                    error,
                    health: None,
                };
                found.extend(state.settle(id, polled, Instant::now(), wall));
                continue;
            }
        };
        let source = status_pages_enabled
            .then(|| provider.status_source().cloned())
            .flatten();
        state.set_feed(id, source.as_ref().map(|s| feed_subject(&feed_key(s))));
        if let Some(source) = source {
            feeds
                .entry(feed_key(&source))
                .or_default()
                .push((id, source));
        }
        if !state.paused(id, wall) {
            targets.push(Target {
                id,
                controller: controller(&config.provider_type),
                provider,
            });
        }
    }

    let mut feed_keys: Vec<FeedKey> = feeds.keys().cloned().collect();
    feed_keys.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    for users in feeds.values_mut() {
        users.sort_by_key(|(id, _)| *id);
    }
    let mut polls_left = targets.len();
    let jobs: Vec<Job> = feed_keys
        .into_iter()
        .map(Job::Feed)
        .chain(targets.into_iter().map(Job::Poll))
        .collect();

    let client = http.status_client().ok();
    let mut queue: VecDeque<Job> = jobs.into();
    let mut overdue_left = MAX_OVERDUE_CHECKS;
    let run = async {
        let mut running = FuturesUnordered::new();
        loop {
            while running.len() < SCAN_CONCURRENCY {
                let Some(job) = queue.pop_front() else {
                    break;
                };
                if let Job::Poll(target) = &job {
                    state.stamp(target.id, wall);
                }
                running.push(execute(job, client.as_deref()));
            }
            let Some(done) = running.next().await else {
                break;
            };
            let follow = match &done {
                Done::Poll { target, polled } => {
                    let share = overdue_left.div_ceil(polls_left.max(1));
                    polls_left = polls_left.saturating_sub(1);
                    match polled {
                        Polled::Fetched { pipelines, .. } => {
                            state.overdue_job(target, pipelines, share)
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(job) = &follow {
                overdue_left -= job.selected.len();
            }
            found.extend(state.apply(done, &feeds, Instant::now(), wall));
            queue.extend(follow.map(Job::Overdue));
        }
    };
    let _ = timeout_at(end, run).await;

    Ok(found)
}

async fn execute(job: Job, client: Option<&Client>) -> Done {
    match job {
        Job::Feed(key) => {
            let feed = read_feed(client, &key).await;
            Done::Feed { key, feed }
        }
        Job::Poll(target) => {
            let polled = poll(&*target.provider, target.id, target.controller).await;
            Done::Poll { target, polled }
        }
        Job::Overdue(job) => check_overdue(job).await,
    }
}

async fn read_feed(client: Option<&Client>, key: &FeedKey) -> Option<Box<ParsedFeed>> {
    match fetch_feed(client?, &key.0, key.1, None).await {
        FeedOutcome::Parsed { feed, .. } => Some(feed),
        _ => None,
    }
}

async fn poll(provider: &dyn Provider, id: i64, controller: bool) -> Polled {
    let paused = match timeout(RATE_LIMIT_TIMEOUT, provider.fetch_rate_limit()).await {
        Ok(Ok(Some(rate_limit))) => quota_pause(&rate_limit, Utc::now()),
        _ => None,
    };
    if let Some(until) = paused {
        return Polled::Paused(until);
    }
    let health = if controller {
        Some(check_instance(provider, id).await)
    } else {
        None
    };
    let fetched = match timeout(FETCH_TIMEOUT, provider.fetch_pipelines()).await {
        Ok(fetched) => fetched,
        Err(_) => Err(DomainError::Timeout(FETCH_TIMEOUT_MESSAGE.to_string())),
    };

    match fetched {
        Ok(pipelines) => Polled::Fetched { pipelines, health },
        Err(error) => Polled::Failed { error, health },
    }
}

async fn check_overdue(job: Overdue) -> Done {
    let now = Utc::now();
    let permits = Semaphore::new(OVERDUE_CONCURRENT_CHECKS);
    let provider = &*job.provider;
    let mut running: FuturesUnordered<_> = job
        .selected
        .into_iter()
        .map(|(pipeline, known)| {
            let permits = &permits;
            async move {
                let _permit = permits.acquire().await.ok()?;
                let runs = timeout(
                    FETCH_TIMEOUT,
                    provider.fetch_run_history(&pipeline.id, OVERDUE_RECENT_RUNS),
                )
                .await
                .ok()?
                .ok()?;
                let overdue = overdue_run(&runs, now);

                Some((pipeline, known, overdue))
            }
        })
        .collect();
    let mut checked = Vec::new();
    let _ = timeout(OVERDUE_BUDGET, async {
        while let Some(result) = running.next().await {
            checked.extend(result);
        }
    })
    .await;

    Done::Overdue {
        id: job.id,
        checked,
    }
}

impl NotificationState {
    fn attempted_at(&self, id: i64) -> Option<DateTime<Utc>> {
        self.providers.get(&id).and_then(|mark| mark.attempted_at)
    }

    fn paused(&self, id: i64, wall: DateTime<Utc>) -> bool {
        self.providers
            .get(&id)
            .and_then(|mark| mark.paused_until)
            .is_some_and(|until| until > wall)
    }

    fn stamp(&mut self, id: i64, wall: DateTime<Utc>) {
        if let Some(mark) = self.providers.get_mut(&id) {
            mark.attempted_at = Some(wall);
        }
    }

    fn pause(&mut self, id: i64, until: Option<DateTime<Utc>>) {
        if let Some(mark) = self.providers.get_mut(&id) {
            mark.paused_until = until;
        }
    }

    fn set_feed(&mut self, id: i64, subject: Option<String>) {
        if let Some(mark) = self.providers.get_mut(&id) {
            mark.feed = subject;
        }
    }

    fn drop_feeds(&mut self) {
        for mark in self.providers.values_mut() {
            mark.feed = None;
            mark.feed_seed = None;
        }
        self.alerts
            .retain(|subject| !subject.starts_with(FEED_SUBJECT_PREFIX));
        self.delivered
            .retain(|subject, _| !subject.starts_with(FEED_SUBJECT_PREFIX));
    }

    fn overdue_job(
        &self, target: &Target, pipelines: &[Pipeline], limit: usize,
    ) -> Option<Overdue> {
        let marks = self.pipelines.get(&target.id);
        let mark_of = |pipeline: &Pipeline| marks.and_then(|marks| marks.get(&pipeline.id));
        let stamp = |pipeline: &Pipeline| mark_of(pipeline).and_then(|mark| mark.checked_at);
        let mut eligible: Vec<&Pipeline> = pipelines
            .iter()
            .filter(|pipeline| tracks_overdue(pipeline))
            .collect();
        eligible.sort_by(|a, b| stamp(a).cmp(&stamp(b)).then_with(|| a.id.cmp(&b.id)));
        eligible.truncate(limit);
        if eligible.is_empty() {
            return None;
        }
        let selected = eligible
            .into_iter()
            .map(|pipeline| {
                let known = mark_of(pipeline).is_some_and(|mark| {
                    mark.checked_at.is_some() || mark.status != PipelineStatus::Running
                });

                (pipeline.clone(), known)
            })
            .collect();

        Some(Overdue {
            id: target.id,
            provider: Arc::clone(&target.provider),
            selected,
        })
    }

    fn settle_overdue(
        &mut self, id: i64, checked: Vec<(Pipeline, bool, Option<String>)>, wall: DateTime<Utc>,
    ) -> Vec<LocalNotification> {
        let mut found = Vec::new();
        for (pipeline, known, overdue) in checked {
            let Some(mark) = self
                .pipelines
                .get_mut(&id)
                .and_then(|marks| marks.get_mut(&pipeline.id))
            else {
                continue;
            };
            mark.checked_at = Some(wall);
            let Some(run) = overdue else {
                continue;
            };
            let changed = mark.overdue.as_deref() != Some(run.as_str());
            mark.overdue = Some(run);
            if known && changed {
                found.push(self.pipeline_notice(
                    &pipeline,
                    format!("{} is running longer than usual", pipeline.name),
                ));
            }
        }

        found
    }

    fn apply(
        &mut self, done: Done, feeds: &HashMap<FeedKey, Vec<(i64, StatusSource)>>, now: Instant,
        wall: DateTime<Utc>,
    ) -> Vec<LocalNotification> {
        match done {
            Done::Poll { target, polled } => self.settle(target.id, polled, now, wall),
            Done::Overdue { id, checked } => self.settle_overdue(id, checked, wall),
            Done::Feed { key, feed } => match (feed, feeds.get(&key)) {
                (Some(feed), Some(users)) => self.observe_feed(&key, &feed, users, now, wall),
                _ => {
                    self.alerts.gap(&feed_subject(&key));
                    Vec::new()
                }
            },
        }
    }

    fn settle(
        &mut self, id: i64, polled: Polled, now: Instant, wall: DateTime<Utc>,
    ) -> Vec<LocalNotification> {
        let name = self
            .providers
            .get(&id)
            .map(|mark| mark.name.clone())
            .unwrap_or_default();
        self.pause(id, None);
        match polled {
            Polled::Paused(until) => {
                self.pause(id, Some(until));
                self.alerts.gap(&instance_subject(id));
                Vec::new()
            }
            Polled::Failed { error, health } => {
                let until = error
                    .retry_after()
                    .and_then(|wait| chrono::Duration::from_std(wait.min(MAX_PAUSE)).ok())
                    .map(|wait| wall + wait);
                self.pause(id, until);
                if health.is_none() {
                    self.alerts.gap(&instance_subject(id));
                }
                let mut found = self.observe_connection(id, &name, Some(error.failure_kind()), now);
                found.extend(self.observe_instance(id, &name, health, now));
                found
            }
            Polled::Fetched { pipelines, health } => {
                let mut found = self.observe_connection(id, &name, None, now);
                found.extend(self.observe_pipelines(id, &pipelines));
                found.extend(self.observe_instance(id, &name, health, now));
                found
            }
        }
    }

    fn observe_connection(
        &mut self, id: i64, name: &str, kind: Option<FailureKind>, now: Instant,
    ) -> Vec<LocalNotification> {
        let mark = self.providers.entry(id).or_default();
        match kind {
            None => mark.failures = 0,
            Some(kind) if kind.is_transient() => {
                mark.failures = mark.failures.saturating_add(1);
            }
            Some(_) => {}
        }
        let subject = connection_subject(id);
        match connectivity(name, kind, mark.failures, id) {
            Connectivity::Reset => {
                self.alerts.reset(&subject);
                Vec::new()
            }
            Connectivity::Ignore => Vec::new(),
            Connectivity::Observe(candidates) => {
                let alerts = self.alerts.observe(
                    &subject,
                    now,
                    Observation {
                        candidates,
                        recovery: connection_recovery(name),
                        provider_ids: vec![id],
                        calm_needed: CALM_FETCHES,
                    },
                );
                self.deliver_all(alerts)
            }
        }
    }

    fn observe_instance(
        &mut self, id: i64, name: &str, health: Option<CheckOutcome>, now: Instant,
    ) -> Vec<LocalNotification> {
        let subject = instance_subject(id);
        match health {
            None => Vec::new(),
            Some(CheckOutcome::Components(components)) => {
                let alerts = self.alerts.observe(
                    &subject,
                    now,
                    Observation {
                        candidates: instance_candidates(name, &components, id),
                        recovery: instance_recovery(name),
                        provider_ids: vec![id],
                        calm_needed: CALM_CHECKS,
                    },
                );
                self.deliver_all(alerts)
            }
            Some(_) => {
                self.alerts.gap(&subject);
                Vec::new()
            }
        }
    }

    fn observe_feed(
        &mut self, key: &FeedKey, feed: &ParsedFeed, users: &[(i64, StatusSource)], now: Instant,
        wall: DateTime<Utc>,
    ) -> Vec<LocalNotification> {
        let mut candidates = Vec::new();
        let mut recovery = None;
        for (id, source) in users {
            let eval = evaluate(feed, &source.components, &source.page_url, wall);
            candidates.extend(feed_candidates(source, &eval, *id));
            recovery.get_or_insert_with(|| feed_recovery(source));
        }
        let Some(recovery) = recovery else {
            return Vec::new();
        };
        let subject = feed_subject(key);
        let seeds: Vec<Option<Vec<String>>> = users
            .iter()
            .map(|(id, _)| {
                self.providers
                    .get_mut(id)
                    .and_then(|mark| mark.feed_seed.take())
            })
            .collect();
        if !self.alerts.has(&subject) && seeds.iter().any(Option::is_some) {
            self.alerts
                .baseline(&subject, seeds.into_iter().flatten().flatten().collect());
        }
        let alerts = self.alerts.observe(
            &subject,
            now,
            Observation {
                candidates: merge(candidates),
                recovery,
                provider_ids: users.iter().map(|(id, _)| *id).collect(),
                calm_needed: CALM_CHECKS,
            },
        );

        self.deliver_all(alerts)
    }
}
