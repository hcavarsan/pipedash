mod observations;
mod overdue;

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
use pipedash_plugin_api::{
    PluginRegistry,
    StatusSource,
};
use reqwest::Client;
use tokio::time::{
    timeout_at,
    Instant,
};

use self::observations::{
    poll,
    read_feed,
};
use self::overdue::check_overdue;
use super::super::checks::check_instance;
use super::super::state::CheckOutcome;
use super::super::{
    feed_key,
    feed_subject,
    FeedKey,
    FEED_SUBJECT_PREFIX,
};
use super::{
    LocalNotification,
    NotificationState,
};
use crate::application::services::provider_service::plugin::{
    controller_health,
    plugin_provider,
};
use crate::application::services::provider_service::ProviderFactory;
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    Provider,
    ProviderConfig,
};
use crate::infrastructure::status_feeds::ParsedFeed;
use crate::infrastructure::HttpClientManager;

const SCAN_CONCURRENCY: usize = 4;
const MAX_OVERDUE_CHECKS: usize = 25;

enum Polled {
    Paused(DateTime<Utc>),
    Fetched(Vec<Pipeline>),
    Failed(DomainError),
}

struct Target {
    id: i64,
    provider: Arc<dyn Provider>,
}

struct Overdue {
    id: i64,
    provider: Arc<dyn Provider>,
    selected: Vec<(Pipeline, bool)>,
}

enum Job {
    Feed(FeedKey),
    Health(Target),
    Poll(Target),
    Overdue(Overdue),
}

enum Done {
    Feed {
        key: FeedKey,
        feed: Option<Box<ParsedFeed>>,
    },
    Health {
        id: i64,
        outcome: CheckOutcome,
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
    state: &mut NotificationState, deadline: Duration, registry: Arc<PluginRegistry>,
) -> DomainResult<Vec<LocalNotification>> {
    let http = Arc::new(HttpClientManager::new()?);
    http.set_proxy(proxy_url)?;
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
    let mut polled = Vec::new();
    let mut provider_jobs = Vec::new();
    let mut feeds: HashMap<FeedKey, Vec<(i64, StatusSource)>> = HashMap::new();
    for (id, config) in configs {
        state.enroll(id, config.display_name(), Some(config.version.unwrap_or(1)));
        let provider = match build(config) {
            Ok(provider) => provider,
            Err(error) => {
                state.stamp(id, wall);
                found.extend(state.settle(id, Polled::Failed(error), Instant::now(), wall));
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
            if controller(&config.provider_type) {
                provider_jobs.push(Job::Health(Target {
                    id,
                    provider: Arc::clone(&provider),
                }));
            }
            provider_jobs.push(Job::Poll(Target { id, provider }));
            polled.push(id);
        }
    }

    let mut feed_keys: Vec<FeedKey> = feeds.keys().cloned().collect();
    feed_keys.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    for users in feeds.values_mut() {
        users.sort_by_key(|(id, _)| *id);
    }
    polled.sort_by_key(|id| (state.overdue_at(*id), *id));
    let mut entitled: HashSet<i64> = polled.into_iter().take(MAX_OVERDUE_CHECKS).collect();
    let jobs: Vec<Job> = feed_keys
        .into_iter()
        .map(Job::Feed)
        .chain(provider_jobs)
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
                Done::Poll { target, polled } if entitled.remove(&target.id) => {
                    let share = overdue_left.div_ceil(entitled.len() + 1);
                    match polled {
                        Polled::Fetched(pipelines) => {
                            state.serve_overdue(target.id, wall);
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
        Job::Health(target) => {
            let outcome = check_instance(&*target.provider, target.id).await;
            Done::Health {
                id: target.id,
                outcome,
            }
        }
        Job::Poll(target) => {
            let polled = poll(&*target.provider).await;
            Done::Poll { target, polled }
        }
        Job::Overdue(job) => check_overdue(job).await,
    }
}

impl NotificationState {
    fn attempted_at(&self, id: i64) -> Option<DateTime<Utc>> {
        self.providers.get(&id).and_then(|mark| mark.attempted_at)
    }

    fn overdue_at(&self, id: i64) -> Option<DateTime<Utc>> {
        self.providers.get(&id).and_then(|mark| mark.overdue_at)
    }

    fn serve_overdue(&mut self, id: i64, wall: DateTime<Utc>) {
        if let Some(mark) = self.providers.get_mut(&id) {
            mark.overdue_at = Some(wall);
        }
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
}
