// context-capsule — core: what the window shows about saved capsules — their front matter and "In short" line,
// newest first, grouped by project (the growing "Whole project" capsule first, then days, newest first).
//! A port of `CapsuleSummary` (capture/Sources/Core.swift) and `LibraryModel.groups` / `CapsuleRow`
//! (capture/Sources/Library.swift).

use crate::projects::Project;
use crate::reminder::front_matter;
use chrono::{Local, NaiveDate};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapsuleSummary {
    pub path: PathBuf,
    pub title: String,
    pub project: String,
    /// "daily", "project" (the growing one), or "" (a one-off capsule).
    pub kind: String,
    pub date: String,
    pub created: String,
    pub author: String,
    pub in_short: String,
    pub draft: bool,
}

/// Capsule files end with this.
pub const SUFFIX: &str = ".capsule.md";

impl CapsuleSummary {
    /// Reads `---` front matter (simple `key: value` lines) and the lines under `## In short`.
    pub fn parse(text: &str, path: &Path) -> CapsuleSummary {
        let front = front_matter(text);
        let get = |k: &str| front.iter().rev().find(|(key, _)| key == k).map(|(_, v)| v.clone()).unwrap_or_default();
        // The body starts after the closing "\n---" (front_matter found it, so this does too).
        let body = match text.strip_prefix("---\n").and_then(|rest| rest.find("\n---").map(|end| &rest[end + 4..])) {
            Some(body) => body,
            None => text,
        };
        let mut in_short = String::new();
        if let Some(start) = find_ignoring_case(body, "## In short") {
            let rest = &body[start + "## In short".len()..];
            let section = rest.find("\n## ").map_or(rest, |end| &rest[..end]);
            in_short = section
                .split(['\n', '\r'])
                .map(|l| l.trim_matches([' ', '\t']))
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
        }
        let file = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        let fallback = strip_extension(strip_extension(&file)).to_string();
        let title = get("title");
        CapsuleSummary {
            path: path.to_path_buf(),
            title: if title.is_empty() { fallback } else { title },
            project: get("project"),
            kind: get("kind"),
            date: get("date"),
            created: get("created"),
            author: get("author"),
            in_short,
            draft: get("draft") == "true",
        }
    }

    /// Every `*.capsule.md` in the folder (not its .history), newest first.
    pub fn all(folder: &Path) -> Vec<CapsuleSummary> {
        let Ok(entries) = std::fs::read_dir(folder) else { return vec![] };
        let mut list: Vec<CapsuleSummary> = entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(SUFFIX) && e.path().is_file())
            .filter_map(|e| std::fs::read_to_string(e.path()).ok().map(|t| Self::parse(&t, &e.path())))
            .collect();
        list.sort_by(|a, b| (&b.created, &b.title).cmp(&(&a.created, &a.title)));
        list
    }

    /// How a row is named: "Whole project" for the growing capsule, the day for daily ones, else the title.
    pub fn label(&self, today: NaiveDate) -> String {
        if self.kind == "project" {
            return "Whole project".into();
        }
        if self.kind == "daily" && !self.date.is_empty() {
            return day_label(&self.date, today);
        }
        self.title.clone()
    }

    /// The day shown on the right, for capsules not already named by their day.
    pub fn when(&self, today: NaiveDate) -> String {
        if self.kind == "daily" || self.created.chars().count() < 10 {
            return String::new();
        }
        day_label(&self.created.chars().take(10).collect::<String>(), today)
    }

    /// The search box on Capsules: every word typed is in the title or "In short" (any letter case). Empty: all.
    pub fn matches(&self, query: &str) -> bool {
        let hay = format!("{} {}", self.title, self.in_short).to_lowercase();
        query.split_whitespace().all(|w| hay.contains(&w.to_lowercase()))
    }

    /// Made this week (not the growing project capsule, which is rewritten every day).
    pub fn made_since(&self, since: chrono::DateTime<chrono::Utc>) -> bool {
        self.kind != "project" && crate::stats::parse_iso(&self.created).is_some_and(|c| c >= since)
    }
}

/// "Today", "Yesterday", else "19 Oct". A text that isn't a date is shown as it is.
pub fn day_label(ymd: &str, today: NaiveDate) -> String {
    let Ok(d) = NaiveDate::parse_from_str(ymd, "%Y-%m-%d") else { return ymd.to_string() };
    if d == today {
        "Today".into()
    } else if today.pred_opt() == Some(d) {
        "Yesterday".into()
    } else {
        d.format("%-d %b").to_string()
    }
}

/// Today, local time (for labels).
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// The capsules that match the search box (all of them when it's empty).
pub fn search(capsules: &[CapsuleSummary], query: &str) -> Vec<CapsuleSummary> {
    capsules.iter().filter(|c| c.matches(query)).cloned().collect()
}

/// One project's capsules (or "Other" for capsules without one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub title: String,
    pub items: Vec<CapsuleSummary>,
}

/// A project's name from projects.json (its id when it isn't there).
pub fn project_name(projects: &[Project], id: &str) -> String {
    projects.iter().find(|p| p.id == id).map_or_else(|| id.to_string(), |p| p.name.clone())
}

/// Projects with capsules by name, then capsules with no project ("Other"). In each: the growing capsule
/// first, then days, newest first.
pub fn groups(capsules: &[CapsuleSummary], projects: &[Project]) -> Vec<Group> {
    let mut order: Vec<&str> = vec![];
    for c in capsules {
        if !order.contains(&c.project.as_str()) {
            order.push(&c.project);
        }
    }
    order.sort_by(|a, b| match (a.is_empty(), b.is_empty()) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => project_name(projects, a).to_lowercase().cmp(&project_name(projects, b).to_lowercase()),
    });
    order
        .into_iter()
        .map(|p| {
            let mut items: Vec<CapsuleSummary> = capsules.iter().filter(|c| c.project == p).cloned().collect();
            items.sort_by(|a, b| {
                (b.kind == "project").cmp(&(a.kind == "project")).then_with(|| (&b.date, &b.created).cmp(&(&a.date, &a.created)))
            });
            Group { title: if p.is_empty() { "Other".into() } else { project_name(projects, p) }, items }
        })
        .collect()
}

fn strip_extension(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

/// Byte offset of `needle` in `hay`, ignoring ASCII case.
fn find_ignoring_case(hay: &str, needle: &str) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    (0..=h.len().checked_sub(n.len())?).find(|&i| hay.is_char_boundary(i) && h[i..i + n.len()].eq_ignore_ascii_case(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(project: &str, kind: &str, date: &str, created: &str) -> CapsuleSummary {
        CapsuleSummary {
            path: PathBuf::from(format!("/c/{project}-{kind}-{date}.capsule.md")),
            title: format!("{project} {date}"),
            project: project.into(),
            kind: kind.into(),
            date: date.into(),
            created: created.into(),
            ..Default::default()
        }
    }

    #[test]
    fn labels() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        assert_eq!(day_label("2026-10-20", today), "Today");
        assert_eq!(day_label("2026-10-19", today), "Yesterday");
        assert_eq!(day_label("2026-10-05", today), "5 Oct");
        assert_eq!(day_label("someday", today), "someday");
        let mut c = cap("web", "project", "", "2026-10-19T18:00:00Z");
        assert_eq!(c.label(today), "Whole project");
        assert_eq!(c.when(today), "Yesterday");
        c.kind = "daily".into();
        c.date = "2026-10-20".into();
        assert_eq!((c.label(today).as_str(), c.when(today).as_str()), ("Today", ""));
        c.kind = String::new();
        assert_eq!(c.label(today), "web ", "a one-off capsule: its title");
    }

    #[test]
    fn search_box() {
        let mut c = cap("web", "daily", "2026-10-19", "2026-10-19T18:00:00Z");
        c.title = "Pricing page".into();
        c.in_short = "Moved Teams to $12 after the Acme call.".into();
        assert!(c.matches("") && c.matches("  "));
        assert!(c.matches("pricing") && c.matches("ACME teams"), "any case, every word, title or In short");
        assert!(!c.matches("pricing figma"));
        let other = cap("app", "daily", "2026-10-18", "2026-10-18T18:00:00Z");
        assert_eq!(search(&[c.clone(), other], "acme"), vec![c]);
    }

    #[test]
    fn grouped_by_project() {
        let projects = vec![Project { id: "web".into(), name: "Website".into(), clues: vec![] }, Project { id: "app".into(), name: "app".into(), clues: vec![] }];
        let list = vec![
            cap("web", "daily", "2026-10-18", "2026-10-18T18:00:00Z"),
            cap("", "", "", "2026-10-19T09:00:00Z"),
            cap("web", "daily", "2026-10-19", "2026-10-19T18:00:00Z"),
            cap("app", "daily", "2026-10-19", "2026-10-19T18:00:00Z"),
            cap("web", "project", "", "2026-10-17T18:00:00Z"),
        ];
        let g = groups(&list, &projects);
        let titles: Vec<&str> = g.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, ["app", "Website", "Other"], "by name, case-insensitive; no project last");
        let web: Vec<(&str, &str)> = g[1].items.iter().map(|c| (c.kind.as_str(), c.date.as_str())).collect();
        assert_eq!(web, [("project", ""), ("daily", "2026-10-19"), ("daily", "2026-10-18")]);
        let since = crate::stats::parse_iso("2026-10-19T00:00:00Z").unwrap();
        assert_eq!(list.iter().filter(|c| c.made_since(since)).count(), 3, "the growing capsule doesn't count");
    }
}
