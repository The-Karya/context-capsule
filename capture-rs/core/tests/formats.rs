// context-capsule — core tests: times follow the system's clock and date order once the platform says so
// (its own test binary: `formats::set` is for the whole process).

use chrono::NaiveDate;
use context_capsule_core::formats;

#[test]
fn twelve_hour_system() {
    let today = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
    assert_eq!((formats::time(16, 5), formats::date(today, false)), ("16:05".into(), "Friday 23 October".into()), "24-hour until told otherwise");
    formats::set(formats::Formats::H12);
    assert_eq!(formats::time(16, 5), "4:05 PM");
    assert_eq!(formats::date(NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(), true), "Wednesday, December 31, 2025");
    formats::set(formats::Formats::H24);
    assert_eq!(formats::time(4, 49), "04:49");
}
