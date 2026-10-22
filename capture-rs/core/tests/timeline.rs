// context-capsule — core: the Timeline read from realistic session folders — a recording across midnight, a
// "Looked at" moment, voice and typed notes, gaps, a correction (assign.json), a recording still going — then
// search, folding, and deleting a moment or a whole recording.

use chrono::{FixedOffset, NaiveDate};
use context_capsule_core::gaps::Why;
use context_capsule_core::timeline::{self, Item, Refused, Row};
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

fn kinds(b: &timeline::Block) -> Vec<String> {
    b.items
        .iter()
        .map(|i| match i {
            Item::Moment(m) if m.is_note() => format!("note {}", m.time()),
            Item::Moment(m) if m.looked_at() => format!("looked {}", m.time()),
            Item::Moment(m) => format!("moment {}", m.time()),
            Item::Gap(g) => g.line(),
        })
        .collect()
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
    assert_eq!(a.line(&p), "23:15 – 23:59 · 2 moments · Website", "that day's part; notes aren't moments; mostly Website");
    assert_eq!((a.moments, a.notes), (2, 1));
    assert_eq!(
        kinds(a),
        ["moment 23:15 – 23:30", "looked 23:40", "Not read 23:45 – 23:55: a private app was in front", "note 23:59"],
        "time order, gaps among the moments"
    );
    let Item::Moment(first) = &a.items[0] else { panic!() };
    assert_eq!(first.details(&p), "Google Chrome · linear.app/acme/pricing · Website");
    assert_eq!(first.heading(), "Pricing — Linear");
    assert_eq!(first.shown_text(false), "Pricing\nFree\nTeams $12", "folded to three lines");
    assert!(first.folds());
    assert_eq!(first.shown_text(true), "Pricing\nFree\nTeams $12\nEnterprise\nContact us");
    let Item::Moment(voice) = &a.items[3] else { panic!() };
    assert!(voice.is_note() && !voice.folds());
    assert_eq!(voice.shown_text(false), "We keep annual at twenty percent off because churn drops.", "a voice note: all its words");
    assert_eq!(voice.note_caption(), "You said · 23:59 · in Figma");

    // The 20th: the rest of that recording, then two more.
    let day = f.day(d(2026, 10, 20));
    let ids: Vec<&str> = day.blocks.iter().map(|b| b.session.as_str()).collect();
    assert_eq!(ids, ["20261019-231500", "20261020-093000", "20261020-140000"]);
    let (a, b, c) = (&day.blocks[0], &day.blocks[1], &day.blocks[2]);
    assert_eq!(
        kinds(a),
        ["moment 00:05 – 00:20", "Not read 00:10 – 00:14: the screen was locked", "Not read 00:16 – 00:17: not read"],
        "a broken line is skipped; a reason this version doesn't know still shows"
    );
    assert_eq!(a.line(&p), "00:05 – 00:20 · 1 moment", "the part after midnight");
    let Item::Moment(code) = &a.items[0] else { panic!() };
    assert_eq!(code.details(&p), r"Code · C:\src\capsule\main.rs", "no project");
    assert_eq!(a.project, None);

    assert_eq!(b.line(&p), "09:30 – 10:10 · 2 moments · Mobile app", "a correction wins");
    assert_eq!(kinds(b), ["moment 09:30 – 10:00", "Not read 10:00: this window wasn't shared", "note 10:01", "moment 10:02 – 10:10"]);
    let Item::Moment(doc) = &b.items[3] else { panic!() };
    assert_eq!(doc.project, "report", "after the correction: the clue (any case)");
    let Item::Moment(note) = &b.items[2] else { panic!() };
    assert_eq!(note.heading(), "A window");
    assert_eq!(note.note_caption(), "You typed · 10:01");
    assert_eq!(note.project, "app", "the correction covers notes too");

    assert!(c.live && !a.live);
    assert_eq!(c.line(&p), "14:00 – 14:29 · 1 moment");

    // What the page shows: "Off" between recordings, not after the one still going.
    let (rows, more) = day.rows(100);
    assert!(!more);
    let offs: Vec<String> = rows.iter().filter_map(|r| if let Row::Off(s) = r { Some(s.clone()) } else { None }).collect();
    assert_eq!(offs, ["Off  00:20 – 09:30", "Off  10:10 – 14:00"]);
    assert_eq!(rows.iter().filter(|r| matches!(r, Row::Block(_))).count(), 3);
    // Paged: three items, then Show more.
    let (rows, more) = day.rows(3);
    assert!(more);
    assert_eq!(rows.iter().filter(|r| matches!(r, Row::Item(..))).count(), 3);
    assert!(!matches!(rows.last(), Some(Row::Off(_))));

    // Nothing that day; the folder that isn't a recording is never read.
    assert!(f.day(d(2026, 10, 18)).is_empty());
    assert_eq!(f.day(d(2026, 10, 23)).blocks.len(), 1);
}

#[test]
fn search_the_day() {
    let f = Fixture::new("search");
    let day = f.day(d(2026, 10, 20));
    let found = day.search("ACME");
    let titles: Vec<String> = found.blocks.iter().flat_map(|b| b.moments().map(|m| m.heading())).collect();
    assert_eq!(titles, ["#design — Acme", "Acme report"], "title, any case; no gaps; only recordings with a match");
    assert_eq!(day.search("docs.google ship").blocks.len(), 0, "every word must be in the same moment");
    assert_eq!(day.search("slack friday").blocks[0].items.len(), 1, "app and text");
    assert_eq!(day.search("capsule\\main").blocks[0].session, "20261019-231500", "where");
    assert_eq!(day.search("ACMÉ").blocks.len(), 1, "accents don't matter");
    assert_eq!(timeline::searchable("Éclair ＦＵＬＬ"), "eclair full");
    assert_eq!(day.search("  "), day, "empty: the whole day");
    let key = ("20261020-093000".to_string(), 1);
    assert_eq!(day.find(&key).map(|m| m.kind.as_str()), Some("note"));
}

#[test]
fn delete_a_moment_or_a_recording() {
    let f = Fixture::new("delete");
    let paths = f.paths();
    let day = f.day(d(2026, 10, 19));
    let Item::Moment(looked) = &day.blocks[0].items[1] else { panic!() };
    timeline::delete_moment(&paths, &looked.session, looked.index, &looked.t_raw, &looked.line).unwrap();
    let left = std::fs::read_to_string(paths.session_dir("20261019-231500").join("events.jsonl")).unwrap();
    let expected: Vec<&str> = A_EVENTS.split('\n').enumerate().filter(|(i, _)| *i != 1).map(|(_, l)| l).collect();
    assert_eq!(left, expected.join("\n"), "exactly that line is gone; the rest byte for byte");
    assert_eq!(
        timeline::delete_moment(&paths, &looked.session, looked.index, &looked.t_raw, &looked.line),
        Err(Refused::NotFound),
        "deleted already"
    );
    // A stale index: found by its exact line.
    let day = f.day(d(2026, 10, 19));
    let Item::Moment(voice) = day.blocks[0].items.iter().rev().find(|i| matches!(i, Item::Moment(_))).unwrap() else { panic!() };
    timeline::delete_moment(&paths, &voice.session, 0, &voice.t_raw, &voice.line).unwrap();
    assert!(!std::fs::read_to_string(paths.session_dir("20261019-231500").join("events.jsonl")).unwrap().contains("churn"));
    // The same line with another t is not it.
    let Item::Moment(code) = &f.day(d(2026, 10, 20)).blocks[0].items[0] else { panic!() };
    assert_eq!(timeline::delete_moment(&paths, &code.session, code.index, "2026-10-19T22:05:01Z", &code.line), Err(Refused::NotFound));

    // Not while it's recording.
    let live = f.day(d(2026, 10, 20)).blocks[2].clone();
    let m = live.moments().next().unwrap();
    assert_eq!(timeline::delete_moment(&paths, &m.session, m.index, &m.t_raw, &m.line), Err(Refused::Recording));
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
    assert_eq!(timeline::intro("this PC"), "Everything recorded on this day, as it was read. It stays on this PC.");
    assert_eq!(timeline::delete_moment_detail("this computer"), "It's removed from this computer. Capsules already made stay as they are.");
    assert!(timeline::is_session_id("20261019-231500") && !timeline::is_session_id("2026101-9231500") && !timeline::is_session_id("../../etc/x"));
    let today = d(2026, 10, 22);
    assert_eq!(timeline::day_label(today, today), "Today");
    assert_eq!(timeline::day_label(d(2026, 10, 19), today), "19 Oct");
    let days = timeline::picker_days(&[d(2026, 10, 23), d(2026, 10, 21), d(2026, 10, 20), d(2026, 10, 19)], today, 3);
    assert_eq!(days, [today, d(2026, 10, 21), d(2026, 10, 20)], "today, yesterday, then recorded days");
    for w in Why::ALL {
        assert!(!w.words().is_empty());
    }
}
