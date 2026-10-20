// context-capsule — core: time per project, for the window's Home (Today and This week), and Claude's corrections
// of which project a stretch of work belongs to (assign.json in a recording's folder).
//! A port of `WorkStats` and `Assignments` in `capture/Sources/Core.swift`: the same rules, so the Mac,
//! Windows and Linux show the same numbers. A correction wins, then the project the recorder chose, then
//! today's clues. Notes and windows only looked at on another screen don't count; each moment counts at
//! least one 3-second sample.

use crate::clock;
use crate::projects::{Project, Projects};
use chrono::{DateTime, Datelike, Duration as Days, Local, NaiveTime, TimeZone, Timelike, Utc};
use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;

/// Like Foundation's default `ISO8601DateFormatter`: "2026-10-19T10:00:00Z" (or an offset), whole seconds.
pub fn parse_iso(s: &str) -> Option<DateTime<Utc>> {
    if s.contains('.') {
        return None;
    }
    DateTime::parse_from_rfc3339(s).ok().map(|t| t.with_timezone(&Utc))
}

/// One correction: from `from` (open when None) up to `to` (open when None), the work is `project`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub project: String,
}

pub struct Assignments;

impl Assignments {
    /// `{"assign":[{"project","from","to"}]}`; a missing or broken file means none.
    pub fn load(path: &Path) -> Vec<Fix> {
        std::fs::read(path).map(|d| Self::parse(&d)).unwrap_or_default()
    }

    pub fn parse(data: &[u8]) -> Vec<Fix> {
        let Ok(serde_json::Value::Object(obj)) = serde_json::from_slice::<serde_json::Value>(data) else { return vec![] };
        // Like Swift's `as? [[String: Any]]`: one entry that isn't an object drops the list.
        let Some(list) = obj.get("assign").and_then(|v| v.as_array()) else { return vec![] };
        if !list.iter().all(|v| v.is_object()) {
            return vec![];
        }
        list.iter()
            .filter_map(|a| {
                let project = a.get("project")?.as_str()?.to_string();
                let time = |k: &str| a.get(k).and_then(|v| v.as_str()).and_then(parse_iso);
                Some(Fix { from: time("from"), to: time("to"), project })
            })
            .collect()
    }

    /// The correction covering `t` (later ones win); "" means "no project"; None: none.
    pub fn at(fixes: &[Fix], t: DateTime<Utc>) -> Option<&str> {
        let mut hit = None;
        for f in fixes {
            if f.from.is_none_or(|from| t >= from) && f.to.is_none_or(|to| t < to) {
                hit = Some(f.project.as_str());
            }
        }
        hit
    }
}

/// Time between two moments: all of it, and per project (most first).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Totals {
    pub seconds: f64,
    /// Project id ("" = not sorted yet) → seconds, most first.
    pub by_project: Vec<(String, f64)>,
}

pub struct WorkStats;

impl WorkStats {
    /// Shortest time one moment counts for (one sample).
    pub const MIN_SECONDS: f64 = 3.0;

    /// Time spent in windows in use between `from` and `to`, from the recordings in `sessions`.
    pub fn totals(sessions: &Path, from: SystemTime, to: SystemTime, projects: &[Project]) -> Totals {
        // A recording can start the day before.
        let earliest = clock::session_id(from - std::time::Duration::from_secs(86_400));
        Self::totals_since(sessions, &earliest, from, to, projects)
    }

    /// The same, reading recordings whose id is `earliest` or later.
    pub fn totals_since(sessions: &Path, earliest: &str, from: SystemTime, to: SystemTime, projects: &[Project]) -> Totals {
        let (from, to) = (DateTime::<Utc>::from(from), DateTime::<Utc>::from(to));
        let mut by: HashMap<String, f64> = HashMap::new();
        let ids: Vec<String> = std::fs::read_dir(sessions)
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        for id in ids.iter().filter(|id| id.chars().count() == 15 && id.as_str() >= earliest) {
            let dir = sessions.join(id);
            let Ok(text) = std::fs::read_to_string(dir.join("events.jsonl")) else { continue };
            let fixes = Assignments::load(&dir.join("assign.json"));
            for line in text.split('\n').filter(|l| !l.is_empty()) {
                let Ok(serde_json::Value::Object(obj)) = serde_json::from_str::<serde_json::Value>(line) else { continue };
                let s = |k: &str| obj.get(k).and_then(|v| v.as_str());
                let Some(t) = s("t").and_then(parse_iso) else { continue };
                // Notes ("kind") and windows only looked at ("focus": "pointer") don't count.
                if t < from || t >= to || obj.contains_key("kind") || s("focus") == Some("pointer") {
                    continue;
                }
                let end = s("end").and_then(parse_iso).unwrap_or(t);
                // The same order as the Claude side: a correction, what the recorder chose, then today's clues.
                let mut project = s("project").unwrap_or("").to_string();
                if let Some(fixed) = Assignments::at(&fixes, t) {
                    project = fixed.to_string();
                } else if project.is_empty() {
                    if let Some(p) = Projects::detect(projects, s("title").unwrap_or(""), s("where").unwrap_or(""), s("app").unwrap_or("")) {
                        project = p.id.clone();
                    }
                }
                let seconds = (end - t).num_milliseconds() as f64 / 1000.0;
                *by.entry(project).or_default() += seconds.max(Self::MIN_SECONDS);
            }
        }
        let mut sorted: Vec<(String, f64)> = by.into_iter().collect();
        sorted.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Totals { seconds: sorted.iter().map(|(_, s)| s).sum(), by_project: sorted }
    }

    /// Days in a row with a recording, ending `today` (or yesterday, when nothing is recorded yet today).
    /// Session ids are "yyyyMMdd-HHmmss" in local time.
    pub fn streak(session_ids: &[String], today: chrono::NaiveDate) -> u32 {
        let days: std::collections::HashSet<String> =
            session_ids.iter().filter(|id| id.chars().count() == 15).map(|id| id.chars().take(8).collect()).collect();
        let key = |d: chrono::NaiveDate| d.format("%Y%m%d").to_string();
        let mut day = today;
        if !days.contains(&key(day)) {
            day = day.pred_opt().unwrap_or(day);
        }
        let mut count = 0;
        while days.contains(&key(day)) {
            count += 1;
            let Some(before) = day.pred_opt() else { break };
            day = before;
        }
        count
    }

    /// The recording folders' names in `sessions`.
    pub fn session_ids(sessions: &Path) -> Vec<String> {
        std::fs::read_dir(sessions)
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default()
    }

    /// "1 h 05 m", "12 m", "40 s".
    pub fn duration(seconds: f64) -> String {
        let s = seconds.max(0.0) as u64;
        if s < 60 {
            return format!("{s} s");
        }
        let m = s / 60;
        if m < 60 {
            format!("{m} m")
        } else {
            format!("{} h {:02} m", m / 60, m % 60)
        }
    }

    /// "Good morning" / "Good afternoon" / "Good evening".
    pub fn greeting(hour: u32) -> &'static str {
        if hour < 12 {
            "Good morning"
        } else if hour < 18 {
            "Good afternoon"
        } else {
            "Good evening"
        }
    }

    /// Midnight today, local time.
    pub fn day_start(now: SystemTime) -> SystemTime {
        let local = DateTime::<Local>::from(now);
        let midnight = local.date_naive().and_time(NaiveTime::MIN);
        Local.from_local_datetime(&midnight).earliest().map(SystemTime::from).unwrap_or(now)
    }

    /// The start of this week, local time. `sunday_first`: weeks start on Sunday (else Monday).
    pub fn week_start(now: SystemTime, sunday_first: bool) -> SystemTime {
        let local = DateTime::<Local>::from(now);
        let back = if sunday_first { local.weekday().num_days_from_sunday() } else { local.weekday().num_days_from_monday() };
        let day = local.date_naive() - Days::days(back as i64);
        Local.from_local_datetime(&day.and_time(NaiveTime::MIN)).earliest().map(SystemTime::from).unwrap_or(now)
    }

    /// The local hour now (for the greeting).
    pub fn hour(now: SystemTime) -> u32 {
        DateTime::<Local>::from(now).hour()
    }
}

/// Weeks start on Sunday in these regions (else Monday): from the locale's region ("en-US" → "US").
pub fn sunday_first(locale: &str) -> bool {
    const SUNDAY: &[&str] = &["US", "CA", "MX", "BR", "JP", "KR", "TW", "HK", "IL", "PH", "IN", "ZA", "AU", "SA", "PE", "CO", "VE", "GT", "HN", "PA", "PR", "DO", "SV", "NI", "BZ", "JM", "TH", "ID", "KE", "ZW", "BW", "MO", "PK", "BD", "NP", "ET", "MT"];
    let region = locale.split(['-', '_', '.', '@']).nth(1).unwrap_or("").to_ascii_uppercase();
    SUNDAY.contains(&region.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrections() {
        let fixes = Assignments::parse(
            br#"{"assign":[{"project":"a"},{"project":"b","from":"2026-10-19T10:00:00Z","to":"2026-10-19T11:00:00Z"}]}"#,
        );
        assert_eq!(fixes.len(), 2);
        let t = |s: &str| parse_iso(s).unwrap();
        assert_eq!(Assignments::at(&fixes, t("2026-10-19T10:30:00Z")), Some("b"), "later ones win");
        assert_eq!(Assignments::at(&fixes, t("2026-10-19T11:00:00Z")), Some("a"), "`to` is not included");
        assert!(Assignments::parse(br#"{"assign":[{"project":"a"}, 3]}"#).is_empty(), "like Swift: a bad entry drops the list");
        assert!(Assignments::parse(b"nope").is_empty());
        assert!(parse_iso("2026-10-19T10:00:00.5Z").is_none(), "whole seconds only, like Foundation");
        assert!(parse_iso("2026-10-19T12:00:00+02:00").is_some());
    }

    #[test]
    fn weeks() {
        assert!(sunday_first("en-US") && sunday_first("en_IN.UTF-8"));
        assert!(!sunday_first("en-GB") && !sunday_first("de-DE") && !sunday_first("fr"));
        let now = SystemTime::now();
        let day = WorkStats::day_start(now);
        assert!(day <= now && now.duration_since(day).unwrap().as_secs() < 25 * 3600);
        let week = WorkStats::week_start(now, false);
        assert!(week <= day && day.duration_since(week).unwrap().as_secs() <= 7 * 86_400);
        assert_eq!(DateTime::<Local>::from(week).weekday(), chrono::Weekday::Mon);
        assert_eq!(DateTime::<Local>::from(WorkStats::week_start(now, true)).weekday(), chrono::Weekday::Sun);
    }
}
