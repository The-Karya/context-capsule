// context-capsule — core tests: the window follows the system's clock and date order once the platform says so
// (its own test binary: `formats::set` is for the whole process).

use chrono::{FixedOffset, NaiveDate, TimeZone};
use context_capsule_core::{formats, timeline};

#[test]
fn twelve_hour_system() {
    let today = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
    assert_eq!(timeline::day_title(today, today), "Friday 23 October", "24-hour until told otherwise");
    formats::set(formats::Formats::H12);
    assert_eq!(timeline::day_title(today, today), "Friday, October 23");
    assert_eq!(timeline::day_title(NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(), today), "Wednesday, December 31, 2025");
    let tz = FixedOffset::east_opt(0).unwrap();
    let t = tz.with_ymd_and_hms(2026, 10, 23, 4, 49, 0).unwrap();
    assert_eq!(formats::time(16, 5), "4:05 PM");
    let block = timeline::Block { session: "20261023-044900".into(), start: t, end: t + chrono::Duration::minutes(84), live: false, items: vec![] };
    assert_eq!(block.line(&[]), "4:49 AM – 6:13 AM · 1 h 24 min");
    formats::set(formats::Formats::H24);
    assert_eq!(block.line(&[]), "04:49 – 06:13 · 1 h 24 min");
}
