use chrono::{
    DateTime,
    Utc,
};

use super::DateRange;

#[test]
fn today_cutoff_uses_client_offset() {
    let now = "2026-10-27T02:00:00Z".parse::<DateTime<Utc>>().unwrap();

    assert_eq!(
        DateRange::Today.cutoff(-300, now),
        "2026-10-26T05:00:00Z".parse::<DateTime<Utc>>().unwrap()
    );
    assert_eq!(
        DateRange::Day.cutoff(0, now),
        "2026-10-26T02:00:00Z".parse::<DateTime<Utc>>().unwrap()
    );
}
