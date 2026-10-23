// context-capsule — core: the evening "Make today's capsules?" reminder (once a day, when something was recorded
// today and no capsule exists for it) and the clean-up of raw recordings older than keep_recordings_days.
// Capsules are never deleted. A port of `Reminder`, `Cleanup` and `DailyJobs` in capture/Sources.

use crate::applog::AppLog;
use crate::clock;
use crate::exclusion::Config;
use crate::paths::Paths;
use chrono::{DateTime, Duration as Days, FixedOffset, Local, NaiveDate, TimeZone, Timelike, Utc};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

/// The reminder's question and the "Make" action's words.
pub const REMIND_QUESTION: &str = "Make today's capsules?";
pub const MAKE_BUTTON: &str = "Make";
pub const LATER_BUTTON: &str = "Not now";
/// Names the app, so Claude uses Context Capsule's tools rather than guessing (Drive, Gmail…).
pub const DAY_PROMPT: &str = "Use Context Capsule to make my capsules for today.";
/// A new chat with DAY_PROMPT typed in (Claude may show a small caution note above it; that's fine).
pub const CLAUDE_NEW_CHAT: &str = "claude://claude.ai/new?q=Use%20Context%20Capsule%20to%20make%20my%20capsules%20for%20today.";
/// Only when Claude was opened without the link (the request is on the clipboard).
pub const PASTE_HINT: &str = "In Claude, press Ctrl+V, then Enter.";

pub struct Reminder;

impl Reminder {
    /// "18:00" → minutes after midnight.
    pub fn parse(hhmm: &str) -> Option<u32> {
        let mut p = hhmm.split(':');
        let (h, m) = (p.next()?, p.next()?);
        if p.next().is_some() {
            return None;
        }
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if !digits(h) || !digits(m) {
            return None;
        }
        let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
        (h < 24 && m < 60).then_some(h * 60 + m)
    }

    /// Time to ask: past the reminder time, something recorded today, no capsule made for today yet, not
    /// asked today already, and not recording right now.
    pub fn due(now_minutes: u32, at: Option<&str>, recorded_today: bool, capsule_today: bool, asked_today: bool, recording: bool) -> bool {
        let Some(target) = at.and_then(Self::parse) else { return false };
        now_minutes >= target && recorded_today && !capsule_today && !asked_today && !recording
    }
}

pub struct Cleanup;

impl Cleanup {
    /// Session folders (YYYYMMDD-HHMMSS) started before `keep_days` days ago, in local time.
    pub fn old_sessions(ids: &[String], keep_days: u32, now: SystemTime) -> Vec<String> {
        Self::old_sessions_on(ids, keep_days, DateTime::<Local>::from(now).date_naive())
    }

    /// The same at a fixed UTC offset (seconds east), for tests.
    pub fn old_sessions_at_offset(ids: &[String], keep_days: u32, now: SystemTime, offset_seconds: i32) -> Vec<String> {
        let tz = FixedOffset::east_opt(offset_seconds).unwrap_or(FixedOffset::east_opt(0).expect("UTC"));
        Self::old_sessions_on(ids, keep_days, DateTime::<Utc>::from(now).with_timezone(&tz).date_naive())
    }

    fn old_sessions_on(ids: &[String], keep_days: u32, today: NaiveDate) -> Vec<String> {
        if keep_days == 0 {
            return vec![];
        }
        let Some(cutoff) = today.checked_sub_signed(Days::days(keep_days as i64)) else { return vec![] };
        let cut = cutoff.format("%Y%m%d-000000").to_string();
        ids.iter().filter(|id| is_session_id(id) && id.as_str() < cut.as_str()).cloned().collect()
    }
}

/// "20261019-100103": a recording folder's name.
pub fn is_session_id(id: &str) -> bool {
    id.len() == 15 && id.as_bytes()[8] == b'-' && id.bytes().enumerate().all(|(i, b)| i == 8 || b.is_ascii_digit())
}

/// The `---` front matter of a capsule file: simple `key: value` lines (comments after " #" and quotes
/// dropped), like the Mac's `CapsuleSummary.parse`. Empty when there is none.
pub fn front_matter(text: &str) -> Vec<(String, String)> {
    let text = text.replace("\r\n", "\n");
    let Some(rest) = text.strip_prefix("---\n") else { return vec![] };
    let Some(end) = rest.find("\n---") else { return vec![] };
    let spaces = |s: &str| s.trim_matches([' ', '\t']).to_string();
    rest[..end]
        .split('\n')
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            let mut value = spaces(value);
            if let Some(hash) = value.find(" #") {
                value = spaces(&value[..hash]);
            }
            let quoted = value.chars().count() >= 2
                && (value.starts_with('"') || value.starts_with('\''))
                && value.ends_with(&value[..1]);
            if quoted {
                value = value[1..value.len() - 1].to_string();
            }
            Some((spaces(key), value))
        })
        .collect()
}

/// Is this capsule about `ymd` ("2026-10-19")? Its `date`, or (not the growing project capsule) the local
/// day it was `created` on.
pub fn capsule_is_for(text: &str, ymd: &str) -> bool {
    let front = front_matter(text);
    let get = |k: &str| front.iter().rev().find(|(key, _)| key == k).map(|(_, v)| v.as_str()).unwrap_or("");
    let created = get("created");
    get("date") == ymd || (get("kind") != "project" && created.chars().count() >= 10 && local_day(created) == ymd)
}

/// An ISO time's day in local time ("2026-10-19"); the first 10 characters when it can't be read.
fn local_day(iso: &str) -> String {
    match DateTime::parse_from_rfc3339(iso) {
        Ok(t) => t.with_timezone(&Local).format("%Y-%m-%d").to_string(),
        Err(_) => iso.chars().take(10).collect(),
    }
}

/// Did any recording that started on `day` ("20261019") save something?
pub fn recorded_on(sessions: &Path, day: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(sessions) else { return false };
    entries.flatten().any(|e| {
        e.file_name().to_string_lossy().starts_with(day)
            && std::fs::metadata(e.path().join("events.jsonl")).is_ok_and(|m| m.len() > 0)
    })
}

/// Is there a capsule for `day` ("20261019") in the capsules folder?
pub fn capsule_on(capsules: &Path, day: &str) -> bool {
    let ymd = format!("{}-{}-{}", &day[..4.min(day.len())], day.get(4..6).unwrap_or(""), day.get(6..8).unwrap_or(""));
    let Ok(entries) = std::fs::read_dir(capsules) else { return false };
    entries.flatten().any(|e| {
        e.file_name().to_string_lossy().ends_with(".capsule.md")
            && std::fs::read_to_string(e.path()).is_ok_and(|text| capsule_is_for(&text, &ymd))
    })
}

/// Delete recordings older than `keep_days` (the current one never is: it started today). Capsules are kept.
pub fn clean_up(paths: &Paths, keep_days: u32, now: SystemTime, log: &AppLog) -> usize {
    let ids: Vec<String> = std::fs::read_dir(paths.sessions())
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    let old = Cleanup::old_sessions(&ids, keep_days, now);
    if old.is_empty() {
        return 0;
    }
    let removed = old.iter().filter(|id| std::fs::remove_dir_all(paths.session_dir(id)).is_ok()).count();
    log.write(&format!("clean-up: removed {removed} recording(s) older than {keep_days} days (capsules kept)"));
    removed
}

/// Once a minute: clean up old recordings (once a day), and say when to ask "Make today's capsules?".
/// The day it last asked is kept in `.reminded` in the data folder, so a restart doesn't ask again.
pub struct DailyJobs {
    last_cleanup: Option<String>,
    next: Option<Instant>,
}

impl Default for DailyJobs {
    fn default() -> Self {
        Self::new()
    }
}

impl DailyJobs {
    pub const EVERY: Duration = Duration::from_secs(60);

    pub fn new() -> DailyJobs {
        DailyJobs { last_cleanup: None, next: None }
    }

    /// Call often; runs at most once a minute. true: show the reminder now (it is marked as asked).
    pub fn poll(&mut self, paths: &Paths, now: SystemTime, recording: bool, log: &AppLog) -> bool {
        let at = Instant::now();
        if self.next.is_some_and(|n| at < n) {
            return false;
        }
        self.next = Some(at + Self::EVERY);
        self.run(paths, now, recording, log)
    }

    /// One check, now.
    pub fn run(&mut self, paths: &Paths, now: SystemTime, recording: bool, log: &AppLog) -> bool {
        let config = Config::load(&paths.config());
        let day: String = clock::session_id(now).chars().take(8).collect();
        if self.last_cleanup.as_deref() != Some(day.as_str()) {
            self.last_cleanup = Some(day.clone());
            clean_up(paths, config.keep_days, now, log);
        }
        let local = Local.from_utc_datetime(&DateTime::<Utc>::from(now).naive_utc());
        let minutes = local.hour() * 60 + local.minute();
        let asked = std::fs::read_to_string(paths.reminded()).is_ok_and(|d| d.trim() == day);
        // Cheap checks first: the folders are only read once it's time.
        if !Reminder::due(minutes, config.remind_at.as_deref(), true, false, asked, recording) {
            return false;
        }
        if !recorded_on(&paths.sessions(), &day) || capsule_on(&paths.capsules(), &day) {
            return false;
        }
        let _ = crate::json::write_atomically(&paths.reminded(), &day);
        log.write("evening reminder shown");
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids() {
        assert!(is_session_id("20261019-100103"));
        assert!(!is_session_id("junk"));
        assert!(!is_session_id("20261019x100103"));
        assert!(!is_session_id("2026101a-100103"));
    }

    #[test]
    fn front_matter_lines() {
        let f = front_matter("---\ntitle: 'A: b'\nauthor: S   # note\n---\nbody");
        assert_eq!(f, vec![("title".into(), "A: b".into()), ("author".into(), "S".into())]);
        assert!(front_matter("no front matter").is_empty());
        assert!(capsule_is_for("---\nkind: project\ndate: 2026-10-19\n---\n", "2026-10-19"), "date wins");
        assert!(!capsule_is_for("---\nkind: project\ncreated: 2026-10-19T12:00:00Z\n---\n", "2026-10-19"), "the project capsule");
    }
}
