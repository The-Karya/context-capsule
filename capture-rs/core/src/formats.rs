// context-capsule — core: how this computer writes a time of day, so the tray and the page follow the system's
// 12/24-hour clock (Linux: the locale), like the Mac does.
//! The platform sets it once at start. Until then, and in tests: 24-hour "09:05".

use std::sync::RwLock;

/// How a time is written.
#[derive(Clone, Copy)]
pub struct Formats {
    /// A time of day (hour 0–23, minute): "4:49 AM" / "04:49".
    pub time: fn(u32, u32) -> String,
}

impl Formats {
    /// 24-hour clock: "04:49".
    pub const H24: Formats = Formats { time: time_24 };
    /// 12-hour clock: "4:49 AM".
    pub const H12: Formats = Formats { time: time_12 };
}

static FORMATS: RwLock<Formats> = RwLock::new(Formats::H24);

/// Use these from now on (the whole app).
pub fn set(f: Formats) {
    *FORMATS.write().unwrap_or_else(|e| e.into_inner()) = f;
}

fn get() -> Formats {
    *FORMATS.read().unwrap_or_else(|e| e.into_inner())
}

/// A time of day as this computer writes it.
pub fn time(hour: u32, minute: u32) -> String {
    (get().time)(hour, minute)
}

/// "04:49"
pub fn time_24(h: u32, m: u32) -> String {
    format!("{h:02}:{m:02}")
}

/// "4:49 AM", "12:05 PM"
pub fn time_12(h: u32, m: u32) -> String {
    let twelve = match h % 12 {
        0 => 12,
        x => x,
    };
    format!("{twelve}:{m:02} {}", if h < 12 { "AM" } else { "PM" })
}

/// Does a C time format (the locale's `nl_langinfo(T_FMT)` / `T_FMT_AMPM` on Linux, e.g. "%I:%M:%S %p", "%r",
/// "%H:%M:%S", "%T") use a 12-hour clock?
pub fn c_format_is_12_hour(t_fmt: &str) -> bool {
    ["%I", "%l", "%r", "%p", "%P"].iter().any(|k| t_fmt.contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(time_24(4, 49), "04:49");
        assert_eq!(time_12(4, 49), "4:49 AM");
        assert_eq!(time_12(0, 5), "12:05 AM");
        assert_eq!(time_12(12, 0), "12:00 PM");
        assert_eq!(time_12(23, 59), "11:59 PM");
    }

    #[test]
    fn which_clock() {
        for twelve in ["%I:%M:%S %p", "%r", "%l:%M %P"] {
            assert!(c_format_is_12_hour(twelve), "{twelve}");
        }
        for day in ["%H:%M:%S", "%T", "%k:%M", ""] {
            assert!(!c_format_is_12_hour(day), "{day}");
        }
    }
}
