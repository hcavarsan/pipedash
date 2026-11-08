use super::*;

#[test]
fn run_numbers_are_exact_javascript_numbers_in_build_order() {
    let older = 2_116_011_604_377_079_808_i64;
    let newer = 2_116_011_604_377_079_811_i64;
    let next_ms = 2_116_011_604_381_274_112_i64;

    for build_id in [older, newer, next_ms] {
        assert!(run_number_of(build_id) < JAVASCRIPT_SAFE_LIMIT);
    }
    assert!(run_number_of(older) < run_number_of(newer));
    assert!(run_number_of(newer) < run_number_of(next_ms));

    for sequential in [4_521, 4_294_967_296, JAVASCRIPT_SAFE_LIMIT - 1] {
        assert_eq!(run_number_of(sequential), sequential);
    }
}

#[test]
fn snowflake_bounds_bracket_a_build() {
    let build_id = 2_116_011_604_377_079_811_i64;
    let (start, end) = snowflake_span(run_number_of(build_id)).unwrap();
    assert!(start <= build_id && build_id < end);

    let millis = (build_id >> SNOWFLAKE_TIME_SHIFT) + SNOWFLAKE_EPOCH_MS;
    assert!(snowflake_at(millis).unwrap() <= build_id);
    assert!(build_id < snowflake_at(millis + 1).unwrap());

    assert_eq!(snowflake_at(SNOWFLAKE_EPOCH_MS), None);
    assert_eq!(snowflake_span(i64::MAX), None);
}
