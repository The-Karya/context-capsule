// context-capsule — core: the Timeline read from realistic session folders — a recording across midnight, a
// "Looked at" read, voice and typed notes, gaps, a correction (assign.json), a recording still going — then every
// rule for stretches, the day's summary, day bar and "Where your time went", search, folding, and deleting a read,
// a stretch or a whole recording.

use chrono::{FixedOffset, NaiveDate};
use context_capsule_core::gaps::Why;
use context_capsule_core::timeline::{self, Colour, Item, Refused};
use context_capsule_core::{Paths, Project};
use std::path::PathBuf;

/// Two hours east of UTC (the fixtures' local time).
fn tz() -> FixedOffset {
    FixedOffset::east_opt(2 * 3600).unwrap()
}

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

const A_EVENTS: &str = concat!(
    // 23:15 local, the 19th: a page, five lines.
    r#"{"t":"2026-10-19T21:15:03Z","end":"2026-10-19T21:30:00Z","app":"Google Chrome","bundle":"chrome.exe","title":"Pricing — Linear","where":"linear.app/acme/pricing","project":"web","project_by":"auto","src":"ax","text":"Pricing\nFree\nTeams $12\nEnterprise\nContact us"}"#, "\n",
    // Looked at on the other screen.
    r#"{"t":"2026-10-19T21:40:00Z","end":"2026-10-19T21:40:00Z","app":"Figma","bundle":"figma.exe","title":"Pricing v2","focus":"pointer","src":"ocr","text":"Annual −20%"}"#, "\n",
    // A voice note at 23:59:30.
    r#"{"t":"2026-10-19T21:59:30Z","end":"2026-10-19T21:59:50Z","kind":"voice","src":"voice","app":"Figma","bundle":"figma.exe","title":"Pricing v2","project":"web","project_by":"auto","text":"We keep annual at twenty percent off because churn drops."}"#, "\n",
    // 00:05 local, the 20th.
    r#"{"t":"2026-10-19T22:05:00Z","end":"2026-10-19T22:20:00Z","app":"Code","bundle":"code.exe","title":"main.rs — capsule","where":"C:\\src\\capsule\\main.rs","src":"ax","text":"fn main() {}"}"#, "\n",
);

const A_GAPS: &str = concat!(
    r#"{"t":"2026-10-19T21:45:00Z","end":"2026-10-19T21:55:00Z","why":"private_app"}"#, "\n",
    r#"{"t":"2026-10-19T22:10:00Z","end":"2026-10-19T22:14:30Z","why":"locked"}"#, "\n",
    "not json\n",
    r#"{"t":"2026-10-19T22:16:00Z","end":"2026-10-19T22:17:00Z","why":"new_reason"}"#, "\n",
);

const B_EVENTS: &str = concat!(
    r##"{"t":"2026-10-20T07:30:03Z","end":"2026-10-20T08:00:00Z","app":"Slack","bundle":"slack.exe","title":"#design — Acme","src":"ax","text":"Ana: can we ship Friday?"}"##, "\n",
    r#"{"t":"2026-10-20T08:01:00Z","end":"2026-10-20T08:01:00Z","kind":"note","src":"typed","app":"","bundle":"","title":"","text":"Decided: ship Friday, without annual."}"#, "\n",
    r#"{"t":"2026-10-20T08:02:00Z","end":"2026-10-20T08:10:00Z","app":"Google Chrome","bundle":"chrome.exe","title":"Acme report","where":"docs.google.com/document/d/1AbC","src":"ax","text":"Q3 report"}"#, "\n",
);

const C_EVENTS: &str = concat!(
    r#"{"t":"2026-10-20T12:00:03Z","end":"2026-10-20T12:29:00Z","app":"Code","bundle":"code.exe","title":"timeline.rs — capsule","src":"ax","text":"pub fn load_day"}"#, "\n",
);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!("capsule-timeline-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let s = root.join("sessions");
        let write = |id: &str, file: &str, text: &str| {
            std::fs::create_dir_all(s.join(id)).unwrap();
            std::fs::write(s.join(id).join(file), text).unwrap();
        };
        write("20261019-231500", "session.json", r#"{"id":"20261019-231500","started_at":"2026-10-19T21:15:00Z","ended_at":"2026-10-19T22:20:01Z","host":"pc","version":1}"#);
        write("20261019-231500", "events.jsonl", A_EVENTS);
        write("20261019-231500", "gaps.jsonl", A_GAPS);
        write("20261020-093000", "session.json", r#"{"id":"20261020-093000","started_at":"2026-10-20T07:30:00Z","ended_at":"2026-10-20T08:10:02Z","host":"pc","version":1,"screens":2}"#);
        write("20261020-093000", "events.jsonl", B_EVENTS);
        write("20261020-093000", "assign.json", r#"{"assign":[{"from":"2026-10-20T07:00:00Z","to":"2026-10-20T08:01:30Z","project":"app"}]}"#);
        write("20261020-093000", "gaps.jsonl", concat!(r#"{"t":"2026-10-20T08:00:10Z","end":"2026-10-20T08:00:50Z","why":"not_shared"}"#, "\n"));
        write("20261020-140000", "session.json", r#"{"id":"20261020-140000","started_at":"2026-10-20T12:00:00Z","ended_at":null,"host":"pc","version":1}"#);
        write("20261020-140000", "events.jsonl", C_EVENTS);
        // Not recordings, or another day.
        write("notes", "events.jsonl", A_EVENTS);
        write("20261023-090000", "events.jsonl", concat!(r#"{"t":"2026-10-23T07:00:03Z","end":"2026-10-23T07:00:03Z","app":"Code","bundle":"code.exe","title":"x","src":"ax","text":""}"#, "\n"));
        std::fs::write(root.join("state.json"), r#"{"recording":true,"session_id":"20261020-140000"}"#).unwrap();
        Fixture { root }
    }

    fn paths(&self) -> Paths {
        Paths::new(&self.root)
    }

    fn day(&self, date: NaiveDate) -> timeline::Day {
        let live = timeline::recording_session(&self.paths());
        let projects = projects();
        timeline::load_day_in(&self.paths().sessions(), live.as_deref(), date, &tz(), &projects)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn projects() -> Vec<Project> {
    vec![
        Project { id: "web".into(), name: "Website".into(), clues: vec!["linear.app".into()] },
        Project { id: "app".into(), name: "Mobile app".into(), clues: vec![] },
        Project { id: "report".into(), name: "Client report".into(), clues: vec!["docs.google.com/document/d/1abc".into()] },
    ]
}


/// What an opened recording lists, one line each.
fn kinds(b: &timeline::Block) -> Vec<String> {
    b.items
        .iter()
        .map(|i| match i {
            Item::Stretch(s) => format!("{} {} · {}", s.time(), s.heading(), s.reads_line()),
            Item::Note(m) => format!("note {}", m.time()),
            Item::Gap(g) => g.line(),
        })
        .collect()
}

fn stretch(i: &Item) -> &timeline::Stretch {
    let Item::Stretch(s) = i else { panic!("not a stretch: {i:?}") };
    s
}

#[test]
fn a_day_across_recordings() {
    let f = Fixture::new("day");
    let p = projects();

    // The 19th: the recording that crosses midnight, up to 23:59:59 local.
    let day = f.day(d(2026, 10, 19));
    assert_eq!(day.blocks.len(), 1);
    let a = &day.blocks[0];
    assert_eq!(a.session, "20261019-231500");
    assert_eq!(a.line(&p), "23:15 – 23:59 · 45 min · Website", "that day's part; its project");
    assert_eq!(
        kinds(a),
        [
            "23:15 – 23:30 Google Chrome · Pricing — Linear · 1 read",
            "23:40 Figma · Pricing v2 · 1 read",
            "Not read 23:45 – 23:55: a private app was in front",
            "note 23:59"
        ],
        "time order, gaps among the stretches"
    );
    let first = stretch(&a.items[0]);
    assert_eq!((first.location.as_str(), first.project.as_str(), first.length().as_str()), ("linear.app/acme/pricing", "web", "15 min"));
    let read = &first.reads[0];
    assert_eq!(read.shown_text(false), "Pricing\nFree\nTeams $12", "folded to three lines");
    assert!(read.folds());
    assert_eq!(read.shown_text(true), "Pricing\nFree\nTeams $12\nEnterprise\nContact us");
    assert!(stretch(&a.items[1]).reads[0].looked_at());
    let Item::Note(voice) = &a.items[3] else { panic!() };
    assert!(!voice.folds());
    assert_eq!(voice.shown_text(false), "We keep annual at twenty percent off because churn drops.", "a voice note: all its words");
    assert_eq!(voice.note_caption(), "You said · 23:59 · in Figma");

    // The 20th: the rest of that recording, then two more.
    let day = f.day(d(2026, 10, 20));
    let ids: Vec<&str> = day.blocks.iter().map(|b| b.session.as_str()).collect();
    assert_eq!(ids, ["20261019-231500", "20261020-093000", "20261020-140000"]);
    let (a, b, c) = (&day.blocks[0], &day.blocks[1], &day.blocks[2]);
    assert_eq!(
        kinds(a),
        ["00:05 – 00:20 Code · main.rs — capsule · 1 read", "Not read 00:10 – 00:14: the screen was locked", "Not read 00:16 – 00:17: not read"],
        "a broken line is skipped; a reason this version doesn't know still shows"
    );
    assert_eq!(a.line(&p), "00:05 – 00:20 · 15 min · Code", "the part after midnight; no project: its apps");

    assert_eq!(b.line(&p), "09:30 – 10:10 · 40 min · Mobile app, Client report", "projects by time; a correction wins");
    assert_eq!(
        kinds(b),
        ["09:30 – 10:00 Slack · #design — Acme · 1 read", "Not read 10:00: this window wasn't shared", "note 10:01", "10:02 – 10:10 Google Chrome · Acme report · 1 read"]
    );
    assert_eq!(stretch(&b.items[3]).project, "report", "after the correction: the clue (any case)");
    let Item::Note(note) = &b.items[2] else { panic!() };
    assert_eq!(note.note_caption(), "You typed · 10:01");
    assert_eq!(note.project, "app", "the correction covers notes too");

    assert!(c.live && !a.live);
    assert_eq!(c.line(&p), "14:00 – 14:29 · 29 min · Code");

    // The header: the day, the sum of the stretches, the recordings.
    assert_eq!(day.recorded(), 900 + 1797 + 480 + 1737);
    assert_eq!(day.summary(), "1 h 22 min recorded · 3 recordings");

    // Where the time went: by project, "Not sorted" among them, grey.
    let went: Vec<(String, String, Colour)> = day.time_went(&p).into_iter().map(|s| (s.name, s.time, s.colour)).collect();
    assert_eq!(
        went,
        [
            ("Not sorted".to_string(), "44 min".to_string(), Colour::Grey),
            ("Mobile app".into(), "30 min".into(), Colour::Blue),
            ("Client report".into(), "8 min".into(), Colour::Teal),
        ]
    );
    assert_eq!(day.time_went(&p)[0].share, 1.0);
    // Home's bars take the same colours.
    let mut home = day.project_colours();
    home.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(home, [("app".to_string(), Colour::Blue), ("report".to_string(), Colour::Teal)]);

    // The day bar: the hour before the first stretch to the hour after the last.
    let bar = day.bar().unwrap();
    assert_eq!((bar.from.format("%H:%M").to_string(), bar.to.format("%H:%M").to_string()), ("00:00".into(), "15:00".into()));
    assert_eq!(bar.hours.len(), 16);
    assert_eq!(bar.hours[0], (0.0, "00:00".to_string()));
    assert_eq!(bar.hours[15].0, 1.0);
    assert_eq!(bar.segments.len(), 4);
    let slack = &bar.segments[1];
    assert_eq!((slack.colour, slack.key.clone()), (Colour::Blue, ("20261020-093000".to_string(), 0)));
    assert!((slack.from - (9.5 * 3600.0 + 3.0) / (15.0 * 3600.0)).abs() < 1e-9);
    assert_eq!(day.stretch(&slack.key).map(|s| s.app.as_str()), Some("Slack"));
    assert_eq!(bar.segments[0].colour, Colour::Grey, "no project");
    assert_eq!(day.colour(day.stretch(&slack.key).unwrap()), Colour::Blue);

    // Nothing that day; the folder that isn't a recording is never read.
    assert!(f.day(d(2026, 10, 18)).is_empty());
    assert_eq!(f.day(d(2026, 10, 18)).summary(), "nothing recorded");
    assert!(f.day(d(2026, 10, 18)).bar().is_none());
    assert_eq!(f.day(d(2026, 10, 23)).blocks.len(), 1);
}

// MARK: stretches

/// A recording at 09:00 local on 21 Oct with these events.jsonl lines (and gaps.jsonl, assign.json).
struct One {
    root: PathBuf,
}

const ID: &str = "20261021-090000";

impl One {
    fn new(name: &str, events: &[String], gaps: &[String], assign: Option<&str>) -> One {
        let root = std::env::temp_dir().join(format!("capsule-stretch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("sessions").join(ID);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("events.jsonl"), events.iter().map(|e| format!("{e}\n")).collect::<String>()).unwrap();
        std::fs::write(dir.join("gaps.jsonl"), gaps.iter().map(|e| format!("{e}\n")).collect::<String>()).unwrap();
        if let Some(a) = assign {
            std::fs::write(dir.join("assign.json"), a).unwrap();
        }
        One { root }
    }

    fn day(&self, projects: &[Project]) -> timeline::Day {
        timeline::load_day_in(&self.root.join("sessions"), None, d(2026, 10, 21), &tz(), projects)
    }

    fn block(&self) -> timeline::Block {
        self.day(&projects()).blocks.remove(0)
    }
}

impl Drop for One {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// "2026-10-21T07:00:00Z" + `s` seconds (09:00 local).
fn at(s: i64) -> String {
    let t = chrono::DateTime::parse_from_rfc3339("2026-10-21T07:00:00Z").unwrap() + chrono::Duration::seconds(s);
    t.to_utc().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// A read from `s` to `s + 3` seconds.
fn read(s: i64, app: &str, title: &str, more: serde_json::Value) -> String {
    let mut v = serde_json::json!({"t": at(s), "end": at(s + 3), "app": app, "bundle": "", "title": title, "src": "ax", "text": ""});
    if let serde_json::Value::Object(extra) = more {
        for (k, x) in extra {
            v[k] = x;
        }
    }
    v.to_string()
}

fn plain(s: i64, app: &str, title: &str) -> String {
    read(s, app, title, serde_json::json!({}))
}

fn shapes(b: &timeline::Block) -> Vec<String> {
    b.items
        .iter()
        .map(|i| match i {
            Item::Stretch(s) => format!("{} {}", s.heading(), s.reads.len()),
            Item::Note(_) => "note".into(),
            Item::Gap(_) => "gap".into(),
        })
        .collect()
}

#[test]
fn same_app_and_title_in_a_row_is_one_stretch() {
    let one = One::new(
        "same",
        &[
            plain(0, "Chrome", "Pricing"),
            plain(3, "Chrome", "Pricing"),
            plain(6, "Chrome", "Pricing"),
            plain(9, "Chrome", "Docs"),
            plain(12, "Code", "Docs"),
            plain(15, "Chrome", "Pricing"),
        ],
        &[],
        None,
    );
    let b = one.block();
    assert_eq!(shapes(&b), ["Chrome · Pricing 3", "Chrome · Docs 1", "Code · Docs 1", "Chrome · Pricing 1"], "a new title or app starts a new one");
    let s = stretch(&b.items[0]);
    assert_eq!((s.time(), s.length(), s.reads_line()), ("09:00".to_string(), "under 1 min".to_string(), "3 reads".to_string()));
    assert_eq!(s.seconds(), 9, "the first read's t to the last one's end");
    assert_eq!(stretch(&b.items[1]).reads_line(), "1 read");
    assert_eq!(s.key(), (ID.to_string(), 0));
    assert_eq!(stretch(&b.items[3]).key(), (ID.to_string(), 5));
}

#[test]
fn an_empty_title_matches_only_an_empty_title() {
    let one = One::new(
        "empty",
        &[plain(0, "Finder", ""), plain(3, "Finder", ""), plain(6, "Finder", "Downloads"), plain(9, "Finder", ""), plain(12, "", ""), plain(15, "", "")],
        &[],
        None,
    );
    let b = one.block();
    assert_eq!(shapes(&b), ["Finder 2", "Finder · Downloads 1", "Finder 1", "A window 2"], "the app alone when there's no title");
}

#[test]
fn a_note_a_gap_or_a_pause_ends_a_stretch() {
    let note = serde_json::json!({"t": at(6), "end": at(8), "kind": "voice", "src": "voice", "app": "Chrome", "title": "Pricing", "text": "Annual stays."}).to_string();
    let one = One::new(
        "breaks",
        &[
            plain(0, "Chrome", "Pricing"),
            plain(3, "Chrome", "Pricing"),
            note,
            plain(9, "Chrome", "Pricing"),
            plain(30, "Chrome", "Pricing"),
            // a gap 40–50, then the same window again
            plain(60, "Chrome", "Pricing"),
            // 60 s after the last end (63): still the same stretch
            plain(123, "Chrome", "Pricing"),
            // 61 s after the last end (126): a new one
            plain(187, "Chrome", "Pricing"),
        ],
        &[serde_json::json!({"t": at(40), "end": at(50), "why": "private_app"}).to_string()],
        None,
    );
    let b = one.block();
    assert_eq!(shapes(&b), ["Chrome · Pricing 2", "note", "Chrome · Pricing 2", "gap", "Chrome · Pricing 2", "Chrome · Pricing 1"]);
    let Item::Gap(g) = &b.items[3] else { panic!() };
    assert_eq!(g.line(), "Not read 09:00: a private app was in front");
}

#[test]
fn what_a_stretch_shows() {
    let long = "x".repeat(130);
    let one = One::new(
        "shows",
        &[
            read(0, "Chrome", "Pricing", serde_json::json!({"text": "short\nAlso short line here!", "project": "web"})),
            read(3, "Chrome", "Pricing", serde_json::json!({"where": "linear.app/pricing", "text": format!("Menu\n   Pricing for teams of any size   \n{long}"), "project": "app"})),
            read(6, "Chrome", "Pricing", serde_json::json!({"where": "linear.app/other", "text": "tiny", "project": "app"})),
            read(9, "Chrome", "Pricing", serde_json::json!({"text": "", "project": "web"})),
            read(12, "Chrome", "Pricing", serde_json::json!({"project": "web"})),
            // Another stretch: its longest text has no line of 20 characters.
            read(100, "Code", "", serde_json::json!({"text": "fn main() {}\nlet x = 1;"})),
            // And one whose longest line is cut at 120.
            read(200, "Mail", "Inbox", serde_json::json!({"text": long})),
        ],
        &[],
        None,
    );
    let b = one.block();
    let s = stretch(&b.items[0]);
    assert_eq!(s.location, "linear.app/pricing", "the first where that isn't empty");
    assert_eq!(s.peek.as_deref(), Some("Pricing for teams of any size"), "the read with the most text: its first line of 20 or more, trimmed");
    assert_eq!(s.project, "web", "the most used project");
    assert_eq!(s.reads.len(), 5);
    assert_eq!(stretch(&b.items[1]).peek, None, "no line that long");
    assert_eq!(stretch(&b.items[1]).project, "", "none");
    let cut = stretch(&b.items[2]).peek.clone().unwrap();
    assert_eq!((cut.chars().count(), cut.ends_with("x…")), (121, true), "cut at 120 characters, then …");

    // A tie: the project seen first.
    let one = One::new("tie", &[read(0, "A", "", serde_json::json!({"project": "app"})), read(3, "A", "", serde_json::json!({"project": "web"}))], &[], None);
    assert_eq!(stretch(&one.block().items[0]).project, "app");
}

#[test]
fn the_project_counts_corrections() {
    // The recorder said web for all three; a correction moves the last two to app.
    let one = One::new(
        "assign",
        &[
            read(0, "Chrome", "Pricing", serde_json::json!({"project": "web"})),
            read(3, "Chrome", "Pricing", serde_json::json!({"project": "web"})),
            read(6, "Chrome", "Pricing", serde_json::json!({"project": "web"})),
        ],
        &[],
        Some(&serde_json::json!({"assign": [{"from": at(3), "to": at(100), "project": "app"}]}).to_string()),
    );
    assert_eq!(stretch(&one.block().items[0]).project, "app");
}

#[test]
fn recorded_time_and_where_it_went() {
    // 09:00–09:30 Website in Chrome, 09:30–09:40 nothing sorted in Code, 10:00–10:20 Website again.
    let mut events = vec![];
    for s in (0..1800).step_by(3) {
        events.push(read(s, "Chrome", "Pricing", serde_json::json!({"project": "web"})));
    }
    for s in (1800..2400).step_by(3) {
        events.push(plain(s, "Code", "main.rs"));
    }
    for s in (3600..4800).step_by(3) {
        events.push(read(s, "Chrome", "Docs", serde_json::json!({"project": "web"})));
    }
    let one = One::new("time", &events, &[], None);
    let p = projects();
    let day = one.day(&p);
    assert_eq!(day.recorded(), 1800 + 600 + 1200, "the sum of the stretches");
    assert_eq!(day.summary(), "1 h recorded · 1 recording");
    let went: Vec<(String, String)> = day.time_went(&p).into_iter().map(|s| (s.name, s.time)).collect();
    assert_eq!(went, [("Website".to_string(), "50 min".to_string()), ("Not sorted".into(), "10 min".into())]);
    assert_eq!(day.time_went(&p)[1].share, 0.2);
    assert_eq!(day.blocks[0].line(&p), "09:00 – 10:20 · 1 h 20 min · Website");
    let bar = day.bar().unwrap();
    assert_eq!(bar.hours.iter().map(|h| h.1.as_str()).collect::<Vec<_>>(), ["09:00", "10:00", "11:00"]);
    assert_eq!(bar.segments.iter().map(|s| s.colour).collect::<Vec<_>>(), [Colour::Blue, Colour::Grey, Colour::Blue]);

    // No project anywhere: per app, each its colour; the recording names its top 2 apps.
    let mut events = vec![];
    for (i, app) in ["Chrome", "Code", "Slack", "Chrome"].iter().enumerate() {
        let from = i as i64 * 600;
        for s in (from..from + if *app == "Slack" { 60 } else { 300 }).step_by(3) {
            events.push(plain(s, app, "x"));
        }
    }
    let one = One::new("apps", &events, &[], None);
    let day = one.day(&[]);
    assert!(day.by_app());
    assert!(day.project_colours().is_empty(), "by app: Home's bars have no project colours to match");
    let went: Vec<(String, Colour)> = day.time_went(&[]).into_iter().map(|s| (s.name, s.colour)).collect();
    assert_eq!(went, [("Chrome".to_string(), Colour::Blue), ("Code".into(), Colour::Teal), ("Slack".into(), Colour::Orange)]);
    assert_eq!(day.blocks[0].names(&[]), ["Chrome", "Code"]);
    assert_eq!(day.bar().unwrap().segments.iter().map(|s| s.colour).collect::<Vec<_>>(), [Colour::Blue, Colour::Teal, Colour::Orange, Colour::Blue]);
    // A read with no app, per app: "Other", grey.
    let one = One::new("other", &[plain(0, "", "x"), plain(3, "", "x"), plain(100, "Code", "y")], &[], None);
    let went: Vec<(String, Colour)> = one.day(&[]).time_went(&[]).into_iter().map(|s| (s.name, s.colour)).collect();
    assert_eq!(went, [("Other".to_string(), Colour::Grey), ("Code".into(), Colour::Blue)]);
}

#[test]
fn lengths() {
    assert_eq!(timeline::length(59), "under 1 min");
    assert_eq!(timeline::length(60), "1 min");
    assert_eq!(timeline::length(38 * 60 + 29), "38 min");
    assert_eq!(timeline::length(38 * 60 + 30), "39 min", "to the nearest minute");
    assert_eq!(timeline::length(59 * 60 + 40), "1 h");
    assert_eq!(timeline::length(3600), "1 h");
    assert_eq!(timeline::length(110 * 60), "1 h 50 min");
    assert_eq!(timeline::length(-5), "under 1 min");
    assert_eq!((timeline::reads(1), timeline::reads(24)), ("1 read".to_string(), "24 reads".to_string()));
    assert_eq!((timeline::recordings(1), timeline::recordings(3)), ("1 recording".to_string(), "3 recordings".to_string()));
    for (i, c) in Colour::ORDER.iter().enumerate() {
        assert!(!Colour::ORDER[..i].contains(c) && *c != Colour::Grey);
    }
}

#[test]
fn search_the_day() {
    let f = Fixture::new("search");
    let day = f.day(d(2026, 10, 20));
    let found = day.search("ACME");
    let titles: Vec<String> = found.stretches().map(|s| s.heading()).collect();
    assert_eq!(titles, ["Slack · #design — Acme", "Google Chrome · Acme report"], "title, any case");
    assert_eq!(found.blocks.iter().map(|b| b.items.len()).collect::<Vec<_>>(), [0, 2, 0], "every recording stays listed");
    assert!(found.blocks.iter().all(|b| b.items.iter().all(|i| !matches!(i, Item::Gap(_)))), "no gaps");
    let hits = |q: &str| day.search(q).blocks.iter().map(|b| b.items.len()).collect::<Vec<_>>();
    assert_eq!(hits("docs.google ship"), [0, 0, 0], "every word must be in the same read");
    assert_eq!(hits("slack friday"), [0, 1, 0], "app and text");
    assert_eq!(hits("capsule\\main"), [1, 0, 0], "where");
    assert_eq!(hits("ACMÉ"), [0, 2, 0], "accents don't matter");
    assert_eq!(timeline::searchable("Éclair ＦＵＬＬ"), "eclair full");
    assert_eq!(day.search("  "), day, "empty: the whole day");
    let notes = day.search("without annual");
    assert!(matches!(notes.blocks[1].items[..], [Item::Note(_)]), "notes that match");
    let key = ("20261020-093000".to_string(), 1);
    assert_eq!(day.find(&key).map(|m| m.kind.as_str()), Some("note"));

    // Only the reads that match are listed; the stretch keeps its count and key.
    let events: Vec<String> = ["one apple", "two pears", "three apples"].iter().enumerate().map(|(i, t)| read(i as i64 * 3, "Notes", "Fruit", serde_json::json!({"text": t}))).collect();
    let one = One::new("search", &events, &[], None);
    let day = one.day(&[]);
    let found = day.search("apple");
    let s = found.stretches().next().unwrap();
    assert_eq!((s.reads.len(), s.shown_reads().len(), s.key()), (3, 2, (ID.to_string(), 0)));
    assert_eq!(s.shown_reads().iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["one apple", "three apples"]);
    assert_eq!(s.reads_line(), "2 of 3 reads match");
    assert_eq!(day.stretches().next().unwrap().reads_line(), "3 reads");
    assert_eq!(found.recorded(), day.recorded());
}

#[test]
fn delete_a_read_a_stretch_or_a_recording() {
    let f = Fixture::new("delete");
    let paths = f.paths();
    let day = f.day(d(2026, 10, 19));
    let looked = stretch(&day.blocks[0].items[1]).reads[0].clone();
    timeline::delete_read(&paths, &looked).unwrap();
    let left = std::fs::read_to_string(paths.session_dir("20261019-231500").join("events.jsonl")).unwrap();
    let expected: Vec<&str> = A_EVENTS.split('\n').enumerate().filter(|(i, _)| *i != 1).map(|(_, l)| l).collect();
    assert_eq!(left, expected.join("\n"), "exactly that line is gone; the rest byte for byte");
    assert_eq!(timeline::delete_read(&paths, &looked), Err(Refused::NotFound), "deleted already");
    // A stale index: found by its exact line.
    let day = f.day(d(2026, 10, 19));
    let Some(Item::Note(voice)) = day.blocks[0].items.last() else { panic!() };
    assert_eq!((timeline::delete_note_menu(voice), timeline::delete_note_question(voice)), ("Delete this voice note…", "Delete this voice note? It can't be undone."));
    timeline::delete_read(&paths, &timeline::Moment { index: 0, ..voice.clone() }).unwrap();
    assert!(!std::fs::read_to_string(paths.session_dir("20261019-231500").join("events.jsonl")).unwrap().contains("churn"));
    // The same line with another t is not it.
    let code = stretch(&f.day(d(2026, 10, 20)).blocks[0].items[0]).reads[0].clone();
    assert_eq!(timeline::delete_read(&paths, &timeline::Moment { t_raw: "2026-10-19T22:05:01Z".into(), ..code }), Err(Refused::NotFound));

    // Not while it's recording.
    let live = f.day(d(2026, 10, 20)).blocks[2].clone();
    let s = live.stretches().next().unwrap();
    assert_eq!(timeline::delete_read(&paths, &s.reads[0]), Err(Refused::Recording));
    assert_eq!(timeline::delete_stretch(&paths, s), Err(Refused::Recording));
    assert_eq!(timeline::delete_session(&paths, &live.session), Err(Refused::Recording));
    assert_eq!(Refused::Recording.message(), ("Stop first".into(), "This recording is still going. Stop it, then delete.".into()));
    assert_eq!(Refused::NotFound.message().0, "Already gone");

    // A whole recording: its folder.
    timeline::delete_session(&paths, "20261020-093000").unwrap();
    assert!(!paths.session_dir("20261020-093000").exists());
    assert_eq!(timeline::delete_session(&paths, "20261020-093000"), Err(Refused::NotFound));
    assert_eq!(timeline::delete_session(&paths, ".."), Err(Refused::NotFound), "never outside sessions/");
    assert_eq!(timeline::delete_session(&paths, "notes"), Err(Refused::NotFound));
    assert_eq!(f.day(d(2026, 10, 20)).blocks.len(), 2);

    // A whole stretch: all its lines (two the same, byte for byte), nothing else.
    let same = plain(3, "Chrome", "Pricing");
    let events = [plain(0, "Chrome", "Pricing"), same.clone(), same.clone(), plain(9, "Code", "x")];
    let one = One::new("stretch", &events, &[], None);
    let paths = Paths::new(&one.root);
    let s = stretch(&one.block().items[0]).clone();
    assert_eq!(s.reads.len(), 3);
    assert_eq!(timeline::delete_stretch_detail(&s, "this PC"), "All 3 reads from 09:00 are removed from this PC. Capsules already made stay as they are.");
    timeline::delete_stretch(&paths, &s).unwrap();
    assert_eq!(std::fs::read_to_string(paths.session_dir(ID).join("events.jsonl")).unwrap(), format!("{}\n", events[3]));
    assert_eq!(timeline::delete_stretch(&paths, &s), Err(Refused::NotFound), "gone: nothing is removed");
    assert_eq!(std::fs::read_to_string(paths.session_dir(ID).join("events.jsonl")).unwrap(), format!("{}\n", events[3]));
}

#[test]
fn words_and_folding() {
    assert_eq!(timeline::session_prompt("20261020-093000"), "Use Context Capsule to make a capsule from my session 20261020-093000.");
    assert_eq!(timeline::fold("a\nb\nc"), ("a\nb\nc".to_string(), false));
    assert_eq!(timeline::fold("a\nb\nc\n"), ("a\nb\nc".to_string(), false), "a last newline isn't a line");
    assert_eq!(timeline::fold("a\r\nb\r\nc\r\nd"), ("a\nb\nc".to_string(), true));
    assert_eq!(timeline::fold("  a  \n\n\t\n b"), ("a\nb".to_string(), false), "blank lines left out, lines trimmed");
    let long = "x".repeat(500);
    let (text, more) = timeline::fold(&long);
    assert!(more && text.chars().count() == 300 && text.ends_with('…'));
    let day = timeline::Day { date: d(2026, 10, 20), blocks: vec![] };
    assert!(day.is_empty());
    assert_eq!(timeline::empty_line(false, true, "Alt+C"), "Nothing recorded today yet. Start contextualizing (Alt+C) when you begin work.");
    assert_eq!(timeline::empty_line(false, false, "Alt+C"), "Nothing recorded on this day.");
    assert_eq!(timeline::empty_line(true, true, "Alt+C"), "Nothing on this day matches.");
    assert_eq!(timeline::delete_read_detail("this computer"), "It's removed from this computer. Capsules already made stay as they are.");
    assert!(timeline::is_session_id("20261019-231500") && !timeline::is_session_id("2026101-9231500") && !timeline::is_session_id("../../etc/x"));
    for w in Why::ALL {
        assert!(!w.words().is_empty());
    }
}

#[test]
fn choosing_the_day() {
    let today = d(2026, 10, 23);
    assert_eq!(timeline::day_title(today, today), "Friday 23 October");
    assert_eq!(timeline::day_title(d(2025, 12, 31), today), "Wednesday 31 December 2025", "the year when it isn't this year");
    assert_eq!(timeline::day_before(d(2026, 1, 1)), d(2025, 12, 31));
    assert_eq!(timeline::day_after(d(2026, 10, 22), today), Some(today));
    assert_eq!(timeline::day_after(today, today), None, "no future days");

    // October 2026 starts on a Thursday: five weeks, Monday first.
    let weeks = timeline::month_weeks(2026, 10);
    assert_eq!(weeks.len(), 5);
    assert_eq!(weeks[0], [None, None, None, Some(d(2026, 10, 1)), Some(d(2026, 10, 2)), Some(d(2026, 10, 3)), Some(d(2026, 10, 4))]);
    assert_eq!(weeks[4][5], Some(d(2026, 10, 31)));
    assert_eq!(weeks[4][6], None);
    assert_eq!(timeline::month_weeks(2027, 2).len(), 4, "February 2027: Monday 1st to Sunday 28th");
    assert_eq!(timeline::month_title(2026, 10), "October 2026");

    // The dots: the month's days with a recording, from the folders' names.
    let recorded = [d(2026, 10, 20), d(2026, 9, 30), d(2026, 10, 19), d(2025, 10, 5), d(2026, 10, 20)];
    assert_eq!(timeline::recorded_in_month(&recorded, 2026, 10), [19, 20]);
    assert_eq!(timeline::recorded_in_month(&recorded, 2026, 11), Vec::<u32>::new());
    let f = Fixture::new("days");
    let on_disk = timeline::days_with_recordings(&f.paths().sessions());
    assert_eq!(timeline::recorded_in_month(&on_disk, 2026, 10), [19, 20, 23], "not the folder that isn't a recording");
}
