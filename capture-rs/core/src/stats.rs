// context-capsule — core: Claude's corrections of which project a stretch of work belongs to (assign.json in a
// recording's folder), and reading the files' ISO times.

use chrono::{DateTime, Utc};
use std::path::Path;

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
}
