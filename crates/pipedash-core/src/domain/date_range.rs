use chrono::{
    DateTime,
    Duration,
    FixedOffset,
    NaiveTime,
    Offset,
    TimeZone,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateRange {
    #[serde(rename = "today")]
    Today,
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "30d")]
    Month,
    #[serde(rename = "60d")]
    TwoMonths,
    #[serde(rename = "90d")]
    Quarter,
}

impl DateRange {
    pub const ALL: [DateRange; 6] = [
        DateRange::Today,
        DateRange::Day,
        DateRange::Week,
        DateRange::Month,
        DateRange::TwoMonths,
        DateRange::Quarter,
    ];

    pub fn cutoff(self, utc_offset_minutes: i32, now: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            DateRange::Today => {
                let offset = utc_offset_minutes
                    .checked_mul(60)
                    .and_then(FixedOffset::east_opt)
                    .unwrap_or_else(|| Utc.fix());
                let midnight = now
                    .with_timezone(&offset)
                    .date_naive()
                    .and_time(NaiveTime::MIN);
                offset
                    .from_local_datetime(&midnight)
                    .single()
                    .map(|local| local.with_timezone(&Utc))
                    .unwrap_or(now)
            }
            DateRange::Day => now - Duration::hours(24),
            DateRange::Week => now - Duration::days(7),
            DateRange::Month => now - Duration::days(30),
            DateRange::TwoMonths => now - Duration::days(60),
            DateRange::Quarter => now - Duration::days(90),
        }
    }
}

#[cfg(test)]
mod tests;
