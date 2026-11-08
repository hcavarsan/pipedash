use super::*;
use crate::application::test_support::{
    pipeline,
    Harness,
    StubProvider,
};
use crate::application::RefreshMode;
use crate::domain::{
    PipelineStatus,
    RefreshState,
};

async fn wait_for_joiners(claim: &FetchClaim, receivers: usize) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while claim.result.receiver_count() < receivers {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("refresh did not join the scheduler claim");
}

#[tokio::test]
async fn joined_refreshes_fetch_real_pipeline_data_after_a_run_claim_is_released() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    *stub.pipelines.lock().unwrap() = Ok(vec![pipeline(id, "fresh", PipelineStatus::Success)]);
    h.repository
        .update_pipelines_cache(id, &[pipeline(id, "stale", PipelineStatus::Running)])
        .await
        .unwrap();
    let lease = h.provider_service.acquire(id).await.unwrap();
    let ForcedClaim::Claimed(claim) =
        h.scheduler
            .claim_forced(id, lease.generation, Instant::now())
    else {
        panic!("expected a run-detail claim");
    };
    let receivers = claim.result.receiver_count();
    h.refresh_manager.set_mode(RefreshMode::Idle);
    h.start().await;
    let service = Arc::clone(&h.pipeline_service);
    let first = tokio::spawn(async move { service.refresh_provider(id).await });
    let service = Arc::clone(&h.pipeline_service);
    let second = tokio::spawn(async move { service.refresh_provider(id).await });
    wait_for_joiners(&claim, receivers + 2).await;
    claim.release();

    let states = [first.await.unwrap().state, second.await.unwrap().state];
    assert!(states.contains(&RefreshState::Ok));
    assert!(states
        .iter()
        .all(|state| matches!(state, RefreshState::Ok | RefreshState::Recent)));
    assert_eq!(stub.fetch_count(), 1);
    assert_eq!(h.cached_names(id).await, ["fresh"]);
}

#[tokio::test]
async fn joined_refresh_does_not_retry_through_a_new_quota_pause() {
    let h = Harness::new().await;
    let (id, stub) = h.add_stub(StubProvider::new).await;
    let lease = h.provider_service.acquire(id).await.unwrap();
    let ForcedClaim::Claimed(claim) =
        h.scheduler
            .claim_forced(id, lease.generation, Instant::now())
    else {
        panic!("expected a run-detail claim");
    };
    let receivers = claim.result.receiver_count();
    h.refresh_manager.set_mode(RefreshMode::Idle);
    h.start().await;
    let service = Arc::clone(&h.pipeline_service);
    let refresh = tokio::spawn(async move { service.refresh_provider(id).await });
    wait_for_joiners(&claim, receivers + 1).await;
    h.scheduler.set_quota_pause(
        id,
        lease.generation,
        Some(Utc::now() + chrono::Duration::minutes(10)),
        Instant::now(),
        Utc::now(),
    );
    claim.release();

    let result = refresh.await.unwrap();
    assert_eq!(result.state, RefreshState::Recent);
    assert_eq!(result.kind, Some(FailureKind::RateLimited));
    assert_eq!(stub.fetch_count(), 0);
}

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

fn scheduler_with(entries: &[(i64, u64)]) -> (Arc<FetchScheduler>, HashMap<i64, Generation>) {
    let scheduler = Arc::new(FetchScheduler::default());
    let generations = entries
        .iter()
        .map(|(id, interval)| {
            let generation = Generation::next();
            scheduler.register(*id, generation, secs(*interval));
            (*id, generation)
        })
        .collect();

    (scheduler, generations)
}

fn no_floors() -> HashMap<i64, Duration> {
    HashMap::new()
}

fn due_ids(scheduler: &Arc<FetchScheduler>, now: Instant) -> Vec<i64> {
    scheduler
        .claim_due(now, &no_floors())
        .iter()
        .map(FetchClaim::id)
        .collect()
}

fn only_claim(scheduler: &Arc<FetchScheduler>, now: Instant) -> FetchClaim {
    let mut claims = scheduler.claim_due(now, &no_floors());
    assert_eq!(claims.len(), 1);
    claims.remove(0)
}

fn failed(kind: FailureKind, retry_after: Option<Duration>) -> Outcome {
    Outcome::Failed { kind, retry_after }
}

fn finish_with(claim: FetchClaim, now: Instant, outcome: Outcome) -> ScheduleView {
    let result = match &outcome {
        Outcome::Failed { .. } => {
            FetchResult::Committed(Err(DomainError::NetworkError("unreachable".to_string())))
        }
        _ => FetchResult::Committed(Ok(0)),
    };
    claim.complete(now, outcome, result).unwrap()
}

fn due_after(seconds: u64) -> Vec<i64> {
    let (scheduler, generations) = scheduler_with(&[(1, 10), (2, 600), (3, 0), (4, 30)]);
    let start = Instant::now();
    for id in [1, 2] {
        let ForcedClaim::Claimed(claim) = scheduler.claim_forced(id, generations[&id], start)
        else {
            panic!("expected a claim for {id}");
        };
        finish_with(claim, start, Outcome::Success);
    }

    due_ids(&scheduler, start + secs(seconds))
}

#[test]
fn each_provider_waits_for_its_own_interval() {
    assert_eq!(due_after(5), vec![4]);
    assert_eq!(due_after(60), vec![1, 4]);
    assert_eq!(due_after(599), vec![1, 4]);
    assert_eq!(due_after(600), vec![1, 2, 4]);
}

#[test]
fn counters_do_not_change_from_view_or_from_claiming() {
    let (scheduler, _) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();

    let before = scheduler.view(1, now).unwrap();
    assert_eq!(scheduler.view(1, now).unwrap(), before);
    let claim = only_claim(&scheduler, now);
    let claimed = scheduler.view(1, now).unwrap();

    assert_eq!(claimed.failures, before.failures);
    assert_eq!(claimed.next_attempt_in, before.next_attempt_in);
    assert!(claimed.in_flight);
    drop(claim);
}

#[test]
fn transient_failures_double_up_to_the_cap() {
    let (scheduler, _) = scheduler_with(&[(1, 30)]);
    let mut now = Instant::now();
    let mut waits = Vec::new();
    for _ in 0..7 {
        let view = finish_with(
            only_claim(&scheduler, now),
            now,
            failed(FailureKind::Network, None),
        );
        waits.push(view.next_attempt_in.as_secs());
        now += view.next_attempt_in;
    }

    assert_eq!(waits, vec![60, 120, 240, 480, 600, 600, 600]);
    assert_eq!(scheduler.view(1, now).unwrap().failures, 7);
}

#[test]
fn timeouts_and_unavailable_count_like_network_failures() {
    for kind in [FailureKind::Timeout, FailureKind::Unavailable] {
        let (scheduler, _) = scheduler_with(&[(1, 30)]);
        let now = Instant::now();

        let view = finish_with(only_claim(&scheduler, now), now, failed(kind, None));

        assert_eq!(view.failures, 1, "{kind:?}");
        assert_eq!(view.next_attempt_in, secs(60), "{kind:?}");
    }
}

#[test]
fn a_server_hint_extends_the_wait_but_never_shortens_it() {
    let (scheduler, _) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();

    let long = finish_with(
        only_claim(&scheduler, now),
        now,
        failed(FailureKind::Unavailable, Some(secs(300))),
    );
    let later = now + long.next_attempt_in;
    let short = finish_with(
        only_claim(&scheduler, later),
        later,
        failed(FailureKind::Unavailable, Some(secs(5))),
    );

    assert_eq!(long.next_attempt_in, secs(300));
    assert_eq!(short.next_attempt_in, secs(120));
}

#[test]
fn rate_limits_use_retry_after_capped_at_an_hour() {
    let (scheduler, _) = scheduler_with(&[(1, 30)]);
    let mut now = Instant::now();
    let mut waits = Vec::new();
    for retry_after in [Some(secs(120)), Some(secs(2 * 3600)), None, Some(secs(5))] {
        let view = finish_with(
            only_claim(&scheduler, now),
            now,
            failed(FailureKind::RateLimited, retry_after),
        );
        waits.push(view.next_attempt_in.as_secs());
        now += view.next_attempt_in;
    }

    assert_eq!(waits, vec![120, 3600, 30 * 8, 30]);
}

#[test]
fn an_interval_longer_than_the_wait_caps_is_kept() {
    let (scheduler, _) = scheduler_with(&[(1, 2 * 3600), (2, 2 * 3600)]);
    let now = Instant::now();
    let claims = scheduler.claim_due(now, &no_floors());
    let mut views = HashMap::new();
    for claim in claims {
        let id = claim.id();
        let outcome = if id == 1 {
            failed(FailureKind::RateLimited, Some(secs(3600)))
        } else {
            failed(FailureKind::Network, None)
        };
        views.insert(id, finish_with(claim, now, outcome));
    }

    assert_eq!(views[&1].next_attempt_in, secs(2 * 3600));
    assert_eq!(views[&2].next_attempt_in, secs(2 * 3600));
}

#[test]
fn auth_failures_do_not_count_and_success_resets() {
    let (scheduler, _) = scheduler_with(&[(1, 30)]);
    let mut now = Instant::now();

    let network = finish_with(
        only_claim(&scheduler, now),
        now,
        failed(FailureKind::Network, None),
    );
    now += network.next_attempt_in;
    let auth = finish_with(
        only_claim(&scheduler, now),
        now,
        failed(FailureKind::Auth, None),
    );
    now += auth.next_attempt_in;
    let success = finish_with(only_claim(&scheduler, now), now, Outcome::Success);

    assert_eq!(network.failures, 1);
    assert_eq!(auth.failures, 1);
    assert_eq!(auth.last_failure, Some(FailureKind::Auth));
    assert_eq!(auth.next_attempt_in, secs(30));
    assert_eq!(success.failures, 0);
    assert_eq!(success.last_failure, None);
    assert_eq!(success.next_attempt_in, secs(30));
}

#[test]
fn the_outage_floor_delays_but_still_lets_a_probe_through() {
    let (scheduler, _) = scheduler_with(&[(1, 10)]);
    let floors = HashMap::from([(1, secs(120))]);
    let start = Instant::now();
    finish_with(only_claim(&scheduler, start), start, Outcome::Success);

    assert!(scheduler.claim_due(start + secs(10), &floors).is_empty());
    assert!(scheduler.claim_due(start + secs(119), &floors).is_empty());
    assert_eq!(
        scheduler.next_due_in(1, start + secs(10), Some(secs(120))),
        Some(secs(110))
    );
    assert_eq!(
        scheduler.next_due_in(1, start + secs(10), None),
        Some(Duration::ZERO)
    );
    let probe = scheduler.claim_due(start + secs(120), &floors);

    assert_eq!(probe.len(), 1);
    let view = finish_with(
        probe.into_iter().next().unwrap(),
        start + secs(120),
        Outcome::Success,
    );
    assert_eq!(view.failures, 0);
    assert_eq!(view.next_attempt_in, secs(10));
}

#[test]
fn interval_zero_is_never_scheduled_but_can_be_forced() {
    let (scheduler, generations) = scheduler_with(&[(1, 0)]);
    let now = Instant::now();

    assert!(scheduler
        .claim_due(now + secs(86_400), &no_floors())
        .is_empty());
    assert!(matches!(
        scheduler.claim_forced(1, generations[&1], now),
        ForcedClaim::Claimed(_)
    ));
}

#[test]
fn forced_claims_bypass_next_attempt_and_the_outage_floor() {
    let (scheduler, generations) = scheduler_with(&[(1, 600)]);
    let start = Instant::now();
    finish_with(only_claim(&scheduler, start), start, Outcome::Success);
    let later = start + secs(20);

    assert!(scheduler.claim_due(later, &no_floors()).is_empty());
    assert!(matches!(
        scheduler.claim_forced(1, generations[&1], later),
        ForcedClaim::Claimed(_)
    ));
}

#[test]
fn forced_claims_wait_out_a_rate_limit_and_the_minimum_gap() {
    let (scheduler, generations) = scheduler_with(&[(1, 30), (2, 30)]);
    let start = Instant::now();
    let mut claims = scheduler.claim_due(start, &no_floors());
    let second = claims.pop().unwrap();
    let first = claims.pop().unwrap();
    finish_with(
        first,
        start,
        failed(FailureKind::RateLimited, Some(secs(120))),
    );
    finish_with(second, start, Outcome::Success);

    let during_rate_limit = scheduler.claim_forced(1, generations[&1], start + secs(60));
    let within_gap = scheduler.claim_forced(2, generations[&2], start + secs(9));
    let after_gap = scheduler.claim_forced(2, generations[&2], start + secs(10));
    let after_rate_limit = scheduler.claim_forced(1, generations[&1], start + secs(120));

    assert!(matches!(
        during_rate_limit,
        ForcedClaim::Recent(Outcome::Failed {
            kind: FailureKind::RateLimited,
            ..
        })
    ));
    assert!(matches!(within_gap, ForcedClaim::Recent(Outcome::Success)));
    assert!(matches!(after_gap, ForcedClaim::Claimed(_)));
    assert!(matches!(after_rate_limit, ForcedClaim::Claimed(_)));
}

#[test]
fn waiting_watch_gets_a_turn_without_bypassing_the_forced_gap() {
    let (scheduler, generations) = scheduler_with(&[(1, 5)]);
    let start = Instant::now();
    finish_with(only_claim(&scheduler, start), start, Outcome::Success);
    let waiting = scheduler.prioritize_forced(1, generations[&1]).unwrap();
    assert!(scheduler
        .claim_due(start + secs(5), &no_floors())
        .is_empty());
    assert!(matches!(
        scheduler.claim_forced(1, generations[&1], start + secs(9)),
        ForcedClaim::Recent(Outcome::Success)
    ));
    let ForcedClaim::Claimed(claim) = scheduler.claim_forced(1, generations[&1], start + secs(10))
    else {
        panic!("a waiting watch must get the next forced read");
    };
    finish_with(claim, start + secs(10), Outcome::Success);
    drop(waiting);
    assert_eq!(due_ids(&scheduler, start + secs(15)), vec![1]);
}

#[test]
fn stale_waiter_cancellation_cannot_release_a_replacement_provider() {
    let (scheduler, generations) = scheduler_with(&[(1, 5)]);
    let old = scheduler.prioritize_forced(1, generations[&1]).unwrap();
    let replacement = Generation::next();
    scheduler.register(1, replacement, secs(5));
    let current = scheduler.prioritize_forced(1, replacement).unwrap();
    drop(old);
    assert!(scheduler.claim_due(Instant::now(), &no_floors()).is_empty());
    drop(current);
    assert_eq!(due_ids(&scheduler, Instant::now()), vec![1]);
}

#[tokio::test]
async fn forced_claims_join_an_attempt_from_claim_time_until_it_completes() {
    let (scheduler, generations) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();
    let claim = only_claim(&scheduler, now);

    let ForcedClaim::Joined(joiner) = scheduler.claim_forced(1, generations[&1], now) else {
        panic!("expected to join the in-flight attempt");
    };
    let waiting = tokio::spawn(joiner.wait());
    tokio::task::yield_now().await;
    assert!(!waiting.is_finished());
    let ForcedClaim::Joined(late) = scheduler.claim_forced(1, generations[&1], now) else {
        panic!("expected to join the in-flight attempt");
    };
    claim.complete(now, Outcome::Success, FetchResult::Committed(Ok(3)));

    assert!(matches!(
        waiting.await.unwrap(),
        FetchResult::Committed(Ok(3))
    ));
    assert!(matches!(late.wait().await, FetchResult::Committed(Ok(3))));
    assert!(!scheduler.view(1, now).unwrap().in_flight);
}

#[tokio::test]
async fn a_dropped_claim_cancels_its_joiners_without_counting_a_failure() {
    let (scheduler, generations) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();
    let claim = only_claim(&scheduler, now);
    let ForcedClaim::Joined(joiner) = scheduler.claim_forced(1, generations[&1], now) else {
        panic!("expected to join the in-flight attempt");
    };

    drop(claim);

    assert!(matches!(joiner.wait().await, FetchResult::Cancelled));
    let view = scheduler.view(1, now).unwrap();
    assert_eq!(view.failures, 0);
    assert_eq!(view.last_failure, None);
    assert!(!view.in_flight);
    let remaining = scheduler.view(1, Instant::now()).unwrap().next_attempt_in;
    assert!(remaining <= secs(30));
    assert!(remaining > Duration::ZERO);
}

#[test]
fn an_old_claim_changes_nothing_after_reset() {
    let (scheduler, generations) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();
    let old = only_claim(&scheduler, now);
    let fresh = Generation::next();
    scheduler.reset(1, fresh, secs(30));
    let current = only_claim(&scheduler, Instant::now());
    let before = scheduler.view(1, now).unwrap();

    let stale = old.complete(
        now,
        failed(FailureKind::Network, None),
        FetchResult::Superseded,
    );
    let after = scheduler.view(1, now).unwrap();

    assert!(stale.is_none());
    assert_eq!(after, before);
    assert!(after.in_flight);
    assert_eq!(after.failures, 0);
    assert_ne!(generations[&1], fresh);
    assert_eq!(current.generation(), fresh);
}

#[test]
fn an_old_claim_dropped_after_reset_leaves_the_new_attempt_alone() {
    let (scheduler, _) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();
    let old = only_claim(&scheduler, now);
    scheduler.reset(1, Generation::next(), secs(30));
    let current = only_claim(&scheduler, Instant::now());

    drop(old);

    assert!(scheduler.view(1, now).unwrap().in_flight);
    drop(current);
    assert!(!scheduler.view(1, now).unwrap().in_flight);
}

#[test]
fn claims_after_remove_are_refused_and_the_entry_is_not_recreated() {
    let (scheduler, generations) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();
    let old = only_claim(&scheduler, now);
    scheduler.remove(1);

    assert!(matches!(
        scheduler.claim_forced(1, generations[&1], now),
        ForcedClaim::Refused
    ));
    assert!(scheduler.claim_due(now, &no_floors()).is_empty());
    old.complete(now, Outcome::Success, FetchResult::Superseded);
    assert!(scheduler.view(1, now).is_none());
    assert!(scheduler.claim_due(now, &no_floors()).is_empty());
    scheduler.set_interval(1, generations[&1], secs(5));
    assert!(scheduler.view(1, now).is_none());
}

#[test]
fn a_reused_id_with_a_new_generation_ignores_old_claims() {
    let (scheduler, generations) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();
    let old = only_claim(&scheduler, now);
    scheduler.remove(1);
    let reused = Generation::next();
    scheduler.register(1, reused, secs(30));

    assert!(matches!(
        scheduler.claim_forced(1, generations[&1], now),
        ForcedClaim::Refused
    ));
    old.complete(
        now,
        failed(FailureKind::Network, None),
        FetchResult::Superseded,
    );
    let view = scheduler.view(1, now).unwrap();

    assert_eq!(view.failures, 0);
    assert!(!view.in_flight);
    assert!(matches!(
        scheduler.claim_forced(1, reused, now),
        ForcedClaim::Claimed(_)
    ));
}

#[test]
fn register_keeps_an_existing_entry_of_the_same_generation() {
    let (scheduler, generations) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();
    finish_with(
        only_claim(&scheduler, now),
        now,
        failed(FailureKind::Network, None),
    );

    scheduler.register(1, generations[&1], secs(30));

    assert_eq!(scheduler.view(1, now).unwrap().failures, 1);
}

#[test]
fn a_shorter_interval_brings_the_next_attempt_forward() {
    let (scheduler, generations) = scheduler_with(&[(1, 3600)]);
    let now = Instant::now();
    finish_with(only_claim(&scheduler, now), now, Outcome::Success);

    scheduler.set_interval(1, generations[&1], secs(30));

    assert!(scheduler.view(1, Instant::now()).unwrap().next_attempt_in <= secs(30));
    assert_eq!(due_ids(&scheduler, now + secs(30)), vec![1]);
}

#[test]
fn a_quota_pause_postpones_every_outcome_until_the_reset() {
    let (scheduler, generations) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();
    let wall = Utc::now();
    let reset = wall + chrono::Duration::minutes(20);
    let claim = only_claim(&scheduler, now);

    let changed = scheduler.set_quota_pause(1, generations[&1], Some(reset), now, wall);
    let repeated = scheduler.set_quota_pause(1, generations[&1], Some(reset), now, wall);
    let view = claim.complete(now, Outcome::Cancelled, FetchResult::Cancelled);

    assert!(changed);
    assert!(!repeated);
    assert_eq!(view.unwrap().next_attempt_in, secs(20 * 60));
    assert_eq!(scheduler.paused_until(1, wall), Some(reset));
    assert!(scheduler
        .claim_due(now + secs(20 * 60 - 1), &no_floors())
        .is_empty());
    assert_eq!(due_ids(&scheduler, now + secs(20 * 60)), vec![1]);
}

#[test]
fn a_quota_pause_turns_manual_refreshes_into_a_rate_limit_answer() {
    let (scheduler, generations) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();
    let wall = Utc::now();
    scheduler.set_quota_pause(
        1,
        generations[&1],
        Some(wall + chrono::Duration::minutes(5)),
        now,
        wall,
    );

    let forced = scheduler.claim_forced(1, generations[&1], now + secs(60));

    let ForcedClaim::Recent(Outcome::Failed { kind, retry_after }) = forced else {
        panic!("expected a recent rate-limit answer, got {forced:?}");
    };
    assert_eq!(kind, FailureKind::RateLimited);
    assert_eq!(retry_after, Some(secs(240)));
}

#[test]
fn a_quota_pause_clears_when_the_quota_is_back_and_ignores_stale_generations() {
    let (scheduler, generations) = scheduler_with(&[(1, 30)]);
    let now = Instant::now();
    let wall = Utc::now();
    let reset = wall + chrono::Duration::minutes(5);
    let stale = Generation::next();

    let ignored = scheduler.set_quota_pause(1, stale, Some(reset), now, wall);
    scheduler.set_quota_pause(1, generations[&1], Some(reset), now, wall);
    let cleared = scheduler.set_quota_pause(1, generations[&1], None, now, wall);
    let already_clear = scheduler.set_quota_pause(1, generations[&1], None, now, wall);

    assert!(!ignored);
    assert!(cleared);
    assert!(!already_clear);
    assert_eq!(scheduler.paused_until(1, wall), None);
    let past = wall - chrono::Duration::seconds(1);
    assert!(!scheduler.set_quota_pause(1, generations[&1], Some(past), now, wall));
    assert_eq!(scheduler.paused_until(1, wall), None);
}

#[test]
fn generations_are_never_reused() {
    let first = Generation::next();
    let second = Generation::next();

    assert_ne!(first, second);
    assert!(second > first);
}
