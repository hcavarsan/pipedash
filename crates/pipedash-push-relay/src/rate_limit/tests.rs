use super::*;

#[test]
fn a_key_gets_its_limit_per_window_and_no_more() {
    let limiter = RateLimiter::new(3, Duration::from_secs(60));
    let start = Instant::now();

    assert!((0..3).all(|_| limiter.allow("a", start)));
    assert!(!limiter.allow("a", start));
    assert!(!limiter.allow("a", start + Duration::from_secs(59)));
}

#[test]
fn the_window_resets_after_it_elapses() {
    let limiter = RateLimiter::new(1, Duration::from_secs(60));
    let start = Instant::now();

    assert!(limiter.allow("a", start));
    assert!(!limiter.allow("a", start));
    assert!(limiter.allow("a", start + Duration::from_secs(60)));
    assert!(!limiter.allow("a", start + Duration::from_secs(61)));
}

#[test]
fn keys_are_limited_independently() {
    let limiter = RateLimiter::new(1, Duration::from_secs(60));
    let start = Instant::now();

    assert!(limiter.allow("a", start));
    assert!(limiter.allow("b", start));
    assert!(!limiter.allow("a", start));
    assert!(!limiter.allow("b", start));
}

#[test]
fn pruning_forgets_only_the_keys_whose_window_has_ended() {
    let limiter = RateLimiter::new(1, Duration::from_secs(60));
    let start = Instant::now();
    assert!(limiter.allow("old", start));
    assert!(limiter.allow("recent", start + Duration::from_secs(50)));
    assert_eq!(limiter.tracked(), 2);

    limiter.prune(start + Duration::from_secs(59));
    assert_eq!(limiter.tracked(), 2);

    limiter.prune(start + Duration::from_secs(60));
    assert_eq!(limiter.tracked(), 1);
    assert!(!limiter.allow("recent", start + Duration::from_secs(61)));
    assert!(limiter.allow("old", start + Duration::from_secs(61)));

    limiter.prune(start + Duration::from_secs(200));
    assert_eq!(limiter.tracked(), 0);
}
