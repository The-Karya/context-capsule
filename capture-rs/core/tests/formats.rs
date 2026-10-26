// context-capsule — core tests: times follow the system's clock once the platform says so
// (its own test binary: `formats::set` is for the whole process).

use context_capsule_core::formats;

#[test]
fn twelve_hour_system() {
    assert_eq!(formats::time(16, 5), "16:05", "24-hour until told otherwise");
    formats::set(formats::Formats::H12);
    assert_eq!(formats::time(16, 5), "4:05 PM");
    formats::set(formats::Formats::H24);
    assert_eq!(formats::time(4, 49), "04:49");
}
