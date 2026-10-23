// context-capsule — core: how this computer writes times of day and dates, so the window follows the system's
// 12/24-hour clock and date order (Windows: Region settings; Linux: the locale), like the Mac does.
//! The platform sets them once at start (and again when the user changes them). Until then, and in tests:
//! 24-hour "09:05" and "Friday 23 October".

use chrono::{Datelike, NaiveDate};
use std::sync::RwLock;

/// The three ways the window writes a time or a date.
#[derive(Clone, Copy)]
pub struct Formats {
    /// A time of day (hour 0–23, minute): "4:49 AM" / "04:49".
    pub time: fn(u32, u32) -> String,
    /// The day bar's hour marks (hour 0–23): "4 AM" / "04:00".
    pub hour: fn(u32) -> String,
    /// A day, with its weekday: "Friday 23 October" / "Friday, October 23"; `year`: with the year.
    pub date: fn(NaiveDate, bool) -> String,
}

impl Formats {
    /// 24-hour clock, day before month: "04:49", "04:00", "Friday 23 October".
    pub const H24: Formats = Formats { time: time_24, hour: hour_24, date: day_month };
    /// 12-hour clock, month before day: "4:49 AM", "4 AM", "Friday, October 23".
    pub const H12: Formats = Formats { time: time_12, hour: hour_12, date: month_day };
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

/// An hour mark as this computer writes it.
pub fn hour(hour: u32) -> String {
    (get().hour)(hour)
}

/// A day with its weekday, as this computer writes it.
pub fn date(day: NaiveDate, year: bool) -> String {
    (get().date)(day, year)
}

/// "04:49"
pub fn time_24(h: u32, m: u32) -> String {
    format!("{h:02}:{m:02}")
}

/// "4:49 AM", "12:05 PM"
pub fn time_12(h: u32, m: u32) -> String {
    format!("{}:{m:02} {}", twelve(h), am_pm(h))
}

/// "04:00"
pub fn hour_24(h: u32) -> String {
    format!("{h:02}:00")
}

/// "4 AM"
pub fn hour_12(h: u32) -> String {
    format!("{} {}", twelve(h), am_pm(h))
}

fn twelve(h: u32) -> u32 {
    match h % 12 {
        0 => 12,
        x => x,
    }
}

fn am_pm(h: u32) -> &'static str {
    if h < 12 {
        "AM"
    } else {
        "PM"
    }
}

/// "Friday 23 October" ("Friday 23 October 2025")
pub fn day_month(d: NaiveDate, year: bool) -> String {
    d.format(if year { "%A %-d %B %Y" } else { "%A %-d %B" }).to_string()
}

/// "Friday, October 23" ("Friday, October 23, 2025")
pub fn month_day(d: NaiveDate, year: bool) -> String {
    let s = d.format("%A, %B %-d").to_string();
    if year {
        format!("{s}, {}", d.year())
    } else {
        s
    }
}

/// Does a C time format (the locale's `nl_langinfo(T_FMT)` / `T_FMT_AMPM` on Linux, e.g. "%I:%M:%S %p", "%r",
/// "%H:%M:%S", "%T") use a 12-hour clock?
pub fn c_format_is_12_hour(t_fmt: &str) -> bool {
    ["%I", "%l", "%r", "%p", "%P"].iter().any(|k| t_fmt.contains(k))
}

/// Does a Windows time picture (LOCALE_SSHORTTIME, e.g. "h:mm tt", "HH:mm") use a 12-hour clock? (h is 12-hour, H 24.)
pub fn windows_picture_is_12_hour(picture: &str) -> bool {
    let mut quoted = false;
    for c in picture.chars() {
        match c {
            '\'' => quoted = !quoted,
            'h' if !quoted => return true,
            'H' if !quoted => return false,
            _ => {}
        }
    }
    false
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
        assert_eq!(hour_24(9), "09:00");
        assert_eq!(hour_12(0), "12 AM");
        assert_eq!(hour_12(13), "1 PM");
    }

    #[test]
    fn dates() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        assert_eq!(day_month(d, false), "Friday 23 October");
        assert_eq!(day_month(d, true), "Friday 23 October 2026");
        assert_eq!(month_day(d, false), "Friday, October 23");
        assert_eq!(month_day(d, true), "Friday, October 23, 2026");
    }

    #[test]
    fn which_clock() {
        for twelve in ["%I:%M:%S %p", "%r", "%l:%M %P"] {
            assert!(c_format_is_12_hour(twelve), "{twelve}");
        }
        for day in ["%H:%M:%S", "%T", "%k:%M", ""] {
            assert!(!c_format_is_12_hour(day), "{day}");
        }
        assert!(windows_picture_is_12_hour("h:mm tt"));
        assert!(windows_picture_is_12_hour("hh:mm tt"));
        assert!(!windows_picture_is_12_hour("HH:mm"));
        assert!(!windows_picture_is_12_hour("H.mm' h'"), "the quoted h is a literal");
        assert!(!windows_picture_is_12_hour(""));
    }
}
