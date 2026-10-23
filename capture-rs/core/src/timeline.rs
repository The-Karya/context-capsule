// context-capsule — core: the Timeline — everything recorded on one day, short enough to read at a glance: a block
// per recording (session), its reads grouped into stretches (one window, the same app and title, in a row), voice
// and typed notes, and the stretches that weren't read (gaps.jsonl), in time order; the day's recorded time, its
// day bar and where the time went; search; deleting a read, a stretch or a whole recording. The words are here
// too, so Windows and Linux say the same as the Mac (SPEC.md, "Timeline"; `Timeline` in capture/Sources).
//! Nothing here leaves the computer. Times are shown in local time; session ids are local, events UTC.

use crate::capsules::project_name;
use crate::gaps::Why;
use crate::json::write_atomically;
use crate::paths::Paths;
use crate::projects::{Project, Projects};
use crate::stats::{parse_iso, Assignments};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use std::collections::HashMap;
use std::path::Path;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

// MARK: words

pub const TIMELINE: &str = "Timeline";
pub const SEE_TODAYS_TIMELINE: &str = "See today's timeline";
pub const TODAY: &str = "Today";
pub const DAY_BEFORE: &str = "Day before";
pub const DAY_AFTER: &str = "Day after";
pub const PICK_A_DAY: &str = "Pick a day";
pub const SEARCH_DAY: &str = "Search this day";
pub const WHERE_TIME_WENT: &str = "Where your time went";
pub const RECORDINGS: &str = "Recordings";
/// Work with no project, in "Where your time went".
pub const NOT_SORTED: &str = "Not sorted";
/// Reads with no app, when "Where your time went" is per app.
pub const OTHER: &str = "Other";
pub const SHOW_READS: &str = "Show what was read";
pub const HIDE_READS: &str = "Hide what was read";
pub const SHOW_ALL: &str = "Show all";
pub const SHOW_LESS: &str = "Show less";
pub const SHOW_MORE: &str = "Show more";
pub const LOOKED_AT: &str = "Looked at";
pub const LOOKED_AT_HELP: &str = "The mouse was over it on another screen. You weren't typing in it.";
pub const RECORDING_NOW: &str = "Recording now";
pub const MAKE_CAPSULE: &str = "Make a capsule";
pub const MAKE_CAPSULE_HELP: &str = "Your AI makes a capsule from this recording.";
pub const DELETE: &str = "Delete";
pub const DELETE_READ_MENU: &str = "Delete this read…";
pub const DELETE_STRETCH_MENU: &str = "Delete this stretch…";
pub const DELETE_BLOCK_MENU: &str = "Delete this recording…";
pub const CANCEL: &str = "Cancel";
pub const LOADING: &str = "Reading…";
pub const NOTHING_NEW: &str = "Nothing new on screen";
pub const NOTHING_IN_IT: &str = "Nothing was recorded.";
pub const NO_MATCHES: &str = "Nothing on this day matches.";
pub const NOTHING_ON_DAY: &str = "Nothing recorded on this day.";
pub const DELETE_READ: &str = "Delete this read? It can't be undone.";
pub const DELETE_STRETCH: &str = "Delete this stretch? It can't be undone.";
pub const DELETE_BLOCK: &str = "Delete this recording? It can't be undone.";
pub const STOP_FIRST: &str = "Stop first";
pub const STOP_FIRST_BODY: &str = "This recording is still going. Stop it, then delete.";
pub const ALREADY_GONE: &str = "Already gone";
pub const ALREADY_GONE_BODY: &str = "It isn't there anymore. The page is up to date now.";
pub const CANT_DELETE: &str = "Couldn't delete";

/// When the day shows nothing. `record_key`: the current shortcut ("Alt+C").
pub fn empty_line(searching: bool, today: bool, record_key: &str) -> String {
    if searching {
        NO_MATCHES.into()
    } else if today {
        format!("Nothing recorded today yet. Start contextualizing ({record_key}) when you begin work.")
    } else {
        NOTHING_ON_DAY.into()
    }
}

/// "Delete this voice note…" / "Delete this note…" (a typed one).
pub fn delete_note_menu(m: &Moment) -> &'static str {
    if m.kind == "voice" {
        "Delete this voice note…"
    } else {
        "Delete this note…"
    }
}

/// "Delete this voice note? It can't be undone." / "Delete this note? …"
pub fn delete_note_question(m: &Moment) -> &'static str {
    if m.kind == "voice" {
        "Delete this voice note? It can't be undone."
    } else {
        "Delete this note? It can't be undone."
    }
}

/// Under "Delete this read? It can't be undone." (and a note's).
pub fn delete_read_detail(this_computer: &str) -> String {
    format!("It's removed from {this_computer}. Capsules already made stay as they are.")
}

/// Under "Delete this stretch? It can't be undone."
pub fn delete_stretch_detail(s: &Stretch, this_computer: &str) -> String {
    let n = s.reads.len();
    let what = if n == 1 { "Its read".to_string() } else { format!("All {n} reads") };
    let verb = if n == 1 { "is" } else { "are" };
    format!("{what} from {} {verb} removed from {this_computer}. Capsules already made stay as they are.", span(s.start, s.end))
}

/// Under "Delete this recording? It can't be undone."
pub fn delete_block_detail(b: &Block, this_computer: &str) -> String {
    format!(
        "Everything recorded from {} to {} is removed from {this_computer}. Capsules already made stay as they are.",
        hm(b.start),
        hm(b.end)
    )
}

/// What "Make a capsule" asks Your AI (it uses get_session_activity).
pub fn session_prompt(session: &str) -> String {
    format!("Use Context Capsule to make a capsule from my session {session}.")
}

/// "38 min", "1 h 50 min", "2 h" (to the nearest minute); under a minute: "under 1 min".
pub fn length(seconds: i64) -> String {
    if seconds < 60 {
        return "under 1 min".into();
    }
    let minutes = (seconds + 30) / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// "24 reads", "1 read".
pub fn reads(n: usize) -> String {
    format!("{n} read{}", if n == 1 { "" } else { "s" })
}

/// "3 recordings", "1 recording".
pub fn recordings(n: usize) -> String {
    format!("{n} recording{}", if n == 1 { "" } else { "s" })
}

/// How many reads "Show what was read" lists before "Show more" (and how many more each time).
pub const PAGE: usize = 150;
/// Read text is folded to this many lines, with Show all.
pub const FOLD_LINES: usize = 3;
/// …each at most this many characters.
pub const FOLD_WIDTH: usize = 300;
/// Show all shows at most this much (a read can hold 100,000 characters).
pub const SHOW_ALL_CHARS: usize = 40_000;
/// A stretch ends when the next read starts more than this long after the previous one's end.
const PAUSE_MS: i64 = 60_000;
/// The peek: the first line at least this long…
const PEEK_MIN: usize = 20;
/// …cut at this many characters.
const PEEK_MAX: usize = 120;

// MARK: colours

/// A project's colour on the day bar and in "Where your time went": plain system colours in this order, "Not
/// sorted" grey. No purple.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Colour {
    Blue,
    Teal,
    Orange,
    Green,
    Pink,
    Brown,
    Grey,
}

impl Colour {
    /// The order projects get them in (longest first; a seventh starts again at blue).
    pub const ORDER: [Colour; 6] = [Colour::Blue, Colour::Teal, Colour::Orange, Colour::Green, Colour::Pink, Colour::Brown];
}

// MARK: the day

/// One events.jsonl line, as the timeline shows it: a read (screen text) or a voice or typed note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moment {
    pub session: String,
    /// Its line number in events.jsonl (0-based) and the exact line: what Delete matches.
    pub index: usize,
    pub line: String,
    /// `t` as written.
    pub t_raw: String,
    pub t: DateTime<FixedOffset>,
    pub end: DateTime<FixedOffset>,
    pub app: String,
    pub bundle: String,
    pub title: String,
    pub location: String,
    /// The project id, as Claude and Home see it: a correction (assign.json), the recorder's, then clues. "": none.
    pub project: String,
    /// "" for a read, "voice" or "note".
    pub kind: String,
    pub src: String,
    pub focus: String,
    pub text: String,
}

/// Identifies a read or note on the page (Show all, Delete).
pub type MomentKey = (String, usize);

impl Moment {
    /// A voice or typed note: shown as a speech bubble with all its words.
    pub fn is_note(&self) -> bool {
        self.kind == "voice" || self.kind == "note"
    }
    /// Read because the mouse was over it on another shared screen.
    pub fn looked_at(&self) -> bool {
        self.focus == "pointer"
    }
    pub fn key(&self) -> MomentKey {
        (self.session.clone(), self.index)
    }
    /// Every word typed is in its title, where, app or text (any letter case or accent). Empty: all.
    pub fn matches(&self, query: &str) -> bool {
        self.has(&words(query))
    }
    fn has(&self, words: &[String]) -> bool {
        if words.is_empty() {
            return true;
        }
        let hay = searchable(&format!("{}\n{}\n{}\n{}", self.title, self.location, self.app, self.text));
        words.iter().all(|w| hay.contains(w.as_str()))
    }
    /// "10:01 – 10:03", or "10:01" within one minute.
    pub fn time(&self) -> String {
        span(self.t, self.end)
    }
    /// The text to show: a read's first lines (`all`: all of it, up to `SHOW_ALL_CHARS`); a note's words.
    pub fn shown_text(&self, all: bool) -> String {
        if self.is_note() {
            return cut(self.text.trim(), SHOW_ALL_CHARS);
        }
        if all {
            return cut(&self.text, SHOW_ALL_CHARS);
        }
        fold(&self.text).0
    }
    /// The text is longer than its folded form (Show all is offered).
    pub fn folds(&self) -> bool {
        !self.is_note() && fold(&self.text).1
    }
    /// Under a note's bubble: "You said · 10:14 · in Figma" / "You typed · 10:14".
    pub fn note_caption(&self) -> String {
        let who = if self.kind == "voice" { "You said" } else { "You typed" };
        let app = if self.app.trim().is_empty() { String::new() } else { format!(" · in {}", self.app.trim()) };
        format!("{who} · {}{app}", hm(self.t))
    }
}

/// Identifies a stretch: its session and its first read's line.
pub type StretchKey = (String, usize);

/// Reads in a row in one window (the same app and title), until a note, a gap or a pause of over a minute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stretch {
    pub session: String,
    /// The first read's `t` and the last one's `end`.
    pub start: DateTime<FixedOffset>,
    pub end: DateTime<FixedOffset>,
    pub app: String,
    pub title: String,
    /// The first `where` that isn't empty.
    pub location: String,
    /// The first line of 20 or more characters from the read with the most text, cut at 120.
    pub peek: Option<String>,
    /// The project most of its reads are in. "": none.
    pub project: String,
    /// Its reads, in time order.
    pub reads: Vec<Moment>,
    /// While searching: the reads that match (None when not searching).
    pub found: Option<Vec<Moment>>,
}

impl Stretch {
    fn of(reads: Vec<Moment>) -> Stretch {
        let (first, last) = (&reads[0], &reads[reads.len() - 1]);
        let location = reads.iter().map(|m| m.location.as_str()).find(|w| !w.is_empty()).unwrap_or("").to_string();
        // The read with the most text (the first of equals).
        let fullest = reads.iter().fold(first, |a, m| if m.text.len() > a.text.len() { m } else { a });
        let peek = peek(&fullest.text);
        // The most used project (the first seen of equals).
        let mut counts: Vec<(&str, usize)> = vec![];
        for m in reads.iter().filter(|m| !m.project.is_empty()) {
            match counts.iter_mut().find(|c| c.0 == m.project) {
                Some(c) => c.1 += 1,
                None => counts.push((&m.project, 1)),
            }
        }
        let project = counts.iter().fold(None, |best: Option<(&str, usize)>, c| if best.is_some_and(|b| b.1 >= c.1) { best } else { Some(*c) });
        Stretch {
            session: first.session.clone(),
            start: first.t,
            end: last.end.max(first.t),
            app: first.app.clone(),
            title: first.title.clone(),
            location,
            peek,
            project: project.map(|p| p.0.to_string()).unwrap_or_default(),
            reads,
            found: None,
        }
    }

    pub fn key(&self) -> StretchKey {
        (self.session.clone(), self.reads[0].index)
    }
    /// How long it lasted.
    pub fn seconds(&self) -> i64 {
        (self.end - self.start).num_seconds().max(0)
    }
    /// "10:00 – 10:38"
    pub fn time(&self) -> String {
        span(self.start, self.end)
    }
    /// "38 min"
    pub fn length(&self) -> String {
        length(self.seconds())
    }
    /// "Google Chrome · Pricing — Linear"; the app alone when there's no title.
    pub fn heading(&self) -> String {
        let (app, title) = (self.app.trim(), self.title.trim());
        match (app.is_empty(), title.is_empty()) {
            (false, false) => format!("{app} · {title}"),
            (false, true) => app.into(),
            (true, false) => title.into(),
            (true, true) => "A window".into(),
        }
    }
    /// "24 reads"; while searching: "2 of 24 reads match".
    pub fn reads_line(&self) -> String {
        match &self.found {
            Some(found) => format!("{} of {} match", found.len(), reads(self.reads.len())),
            None => reads(self.reads.len()),
        }
    }
    /// What Show what was read lists: the reads, or while searching those that match.
    pub fn shown_reads(&self) -> &[Moment] {
        self.found.as_deref().unwrap_or(&self.reads)
    }
}

/// A stretch that wasn't read (gaps.jsonl), cut to the day.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gap {
    pub session: String,
    pub t: DateTime<FixedOffset>,
    pub end: DateTime<FixedOffset>,
    /// The code as written ("private_app"…); one this version doesn't know still shows ("not read").
    pub why: String,
}

impl Gap {
    /// "Not read 10:40 – 10:55: a private app was in front"
    pub fn line(&self) -> String {
        let words = Why::parse(&self.why).map_or("not read", Why::words);
        format!("Not read {}: {words}", span(self.t, self.end))
    }
}

/// What an opened recording lists, in time order.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Stretch(Stretch),
    Note(Moment),
    Gap(Gap),
}

impl Item {
    pub fn t(&self) -> DateTime<FixedOffset> {
        match self {
            Item::Stretch(s) => s.start,
            Item::Note(m) => m.t,
            Item::Gap(g) => g.t,
        }
    }
}

/// Reads, notes and gaps in time order, grouped into stretches.
fn group(parts: Vec<Part>) -> Vec<Item> {
    let mut items = vec![];
    let mut run: Vec<Moment> = vec![];
    for part in parts {
        match part {
            Part::Moment(m) if !m.is_note() => {
                if let Some(last) = run.last() {
                    let same = last.app == m.app && last.title == m.title && (m.t - last.end).num_milliseconds() <= PAUSE_MS;
                    if !same {
                        items.push(Item::Stretch(Stretch::of(std::mem::take(&mut run))));
                    }
                }
                run.push(m);
            }
            other => {
                if !run.is_empty() {
                    items.push(Item::Stretch(Stretch::of(std::mem::take(&mut run))));
                }
                items.push(match other {
                    Part::Moment(m) => Item::Note(m),
                    Part::Gap(g) => Item::Gap(g),
                });
            }
        }
    }
    if !run.is_empty() {
        items.push(Item::Stretch(Stretch::of(run)));
    }
    items
}

/// One events.jsonl or gaps.jsonl line, before grouping. Most are moments: not boxed.
#[allow(clippy::large_enum_variant)]
enum Part {
    Moment(Moment),
    Gap(Gap),
}

impl Part {
    fn t(&self) -> DateTime<FixedOffset> {
        match self {
            Part::Moment(m) => m.t,
            Part::Gap(g) => g.t,
        }
    }
}

/// One recording (session), as far as it is on the day.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub session: String,
    /// From its first read, note or gap that day to its last (else when it started).
    pub start: DateTime<FixedOffset>,
    pub end: DateTime<FixedOffset>,
    /// Still recording.
    pub live: bool,
    /// This day's stretches, notes and gaps, in time order.
    pub items: Vec<Item>,
}

impl Block {
    /// Folded: "10:00 – 11:40 · 1 h 40 min · Website, Client report" (its projects by time, else its top 2 apps).
    pub fn line(&self, projects: &[Project]) -> String {
        let mut parts = vec![span(self.start, self.end), length((self.end - self.start).num_seconds())];
        let names = self.names(projects);
        if !names.is_empty() {
            parts.push(names.join(", "));
        }
        parts.join(" · ")
    }

    /// Its projects, longest first; if none, its 2 apps with the most time.
    pub fn names(&self, projects: &[Project]) -> Vec<String> {
        let by_project = by_time(self.stretches(), |s| s.project.as_str());
        if !by_project.is_empty() {
            return by_project.into_iter().map(|(id, _)| project_name(projects, id)).collect();
        }
        by_time(self.stretches(), |s| s.app.trim()).into_iter().take(2).map(|(app, _)| app.to_string()).collect()
    }

    pub fn stretches(&self) -> impl Iterator<Item = &Stretch> {
        self.items.iter().filter_map(|i| if let Item::Stretch(s) = i { Some(s) } else { None })
    }

    /// The sum of its stretches' lengths.
    pub fn recorded(&self) -> i64 {
        self.stretches().map(Stretch::seconds).sum()
    }
}

/// Time per key ("" left out), longest first (the first seen of equals).
fn by_time<'a>(stretches: impl Iterator<Item = &'a Stretch>, key: impl Fn(&'a Stretch) -> &'a str) -> Vec<(&'a str, i64)> {
    let mut out: Vec<(&str, i64)> = vec![];
    for s in stretches {
        let k = key(s);
        if k.is_empty() {
            continue;
        }
        match out.iter_mut().find(|o| o.0 == k) {
            Some(o) => o.1 += s.seconds(),
            None => out.push((k, s.seconds())),
        }
    }
    out.sort_by_key(|o| std::cmp::Reverse(o.1)); // stable
    out
}

/// One row of "Where your time went": a project (or an app), its time and its share of the longest row (0–1).
#[derive(Clone, Debug, PartialEq)]
pub struct Share {
    pub name: String,
    pub seconds: i64,
    pub time: String,
    pub share: f64,
    pub colour: Colour,
}

/// The day bar: from the start of the first stretch's hour to the end of the last one's; each stretch a segment.
#[derive(Clone, Debug, PartialEq)]
pub struct DayBar {
    pub from: DateTime<FixedOffset>,
    pub to: DateTime<FixedOffset>,
    /// Where each hour is (0–1) and its label ("10:00").
    pub hours: Vec<(f64, String)>,
    pub segments: Vec<Segment>,
}

/// One stretch on the day bar: where it starts and ends (0–1), its colour, and which stretch it is (a click opens it).
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub from: f64,
    pub to: f64,
    pub colour: Colour,
    pub key: StretchKey,
}

/// One day: its recordings in time order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Day {
    pub date: NaiveDate,
    pub blocks: Vec<Block>,
}

impl Day {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn stretches(&self) -> impl Iterator<Item = &Stretch> {
        self.blocks.iter().flat_map(Block::stretches)
    }

    /// A read or a note.
    pub fn find(&self, key: &MomentKey) -> Option<&Moment> {
        self.blocks.iter().filter(|b| b.session == key.0).flat_map(|b| &b.items).find_map(|i| match i {
            Item::Stretch(s) => s.reads.iter().find(|m| m.index == key.1),
            Item::Note(m) if m.index == key.1 => Some(m),
            _ => None,
        })
    }

    pub fn stretch(&self, key: &StretchKey) -> Option<&Stretch> {
        self.stretches().find(|s| s.session == key.0 && s.reads[0].index == key.1)
    }

    pub fn block(&self, session: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.session == session)
    }

    /// The sum of the stretches' lengths.
    pub fn recorded(&self) -> i64 {
        self.blocks.iter().map(Block::recorded).sum()
    }

    /// Under the day control: "1 h 50 min recorded · 3 recordings" ("nothing recorded" when there's none).
    pub fn summary(&self) -> String {
        let mut parts = vec![];
        if self.stretches().next().is_some() {
            parts.push(format!("{} recorded", length(self.recorded())));
        }
        parts.push(if self.blocks.is_empty() { "nothing recorded".into() } else { recordings(self.blocks.len()) });
        parts.join(" · ")
    }

    /// No stretch has a project: "Where your time went" and the day bar go by app.
    pub fn by_app(&self) -> bool {
        self.stretches().all(|s| s.project.is_empty())
    }

    /// What a stretch is counted under: its project, or its app when no stretch has one. "": Not sorted.
    fn key_of<'a>(&self, s: &'a Stretch, by_app: bool) -> &'a str {
        if by_app {
            s.app.trim()
        } else {
            s.project.as_str()
        }
    }

    /// "Where your time went": one row per project (or per app), longest first; "Not sorted" among them, grey.
    pub fn time_went(&self, projects: &[Project]) -> Vec<Share> {
        let by_app = self.by_app();
        let mut rows: Vec<(&str, i64)> = vec![];
        for s in self.stretches() {
            let k = self.key_of(s, by_app);
            match rows.iter_mut().find(|r| r.0 == k) {
                Some(r) => r.1 += s.seconds(),
                None => rows.push((k, s.seconds())),
            }
        }
        rows.sort_by_key(|r| std::cmp::Reverse(r.1)); // stable: the first seen of equals
        let longest = rows.first().map_or(0, |r| r.1);
        let colours = self.colours();
        rows.into_iter()
            .map(|(k, seconds)| Share {
                name: match (k.is_empty(), by_app) {
                    (true, false) => NOT_SORTED.into(),
                    (true, true) => OTHER.into(),
                    (false, true) => k.to_string(),
                    (false, false) => project_name(projects, k),
                },
                seconds,
                time: length(seconds),
                share: if longest > 0 { seconds as f64 / longest as f64 } else { 0.0 },
                colour: colours.get(k).copied().unwrap_or(Colour::Grey),
            })
            .collect()
    }

    /// A stretch's colour: its project's (or its app's, when it goes by app); none: grey.
    pub fn colour(&self, s: &Stretch) -> Colour {
        self.colours().get(self.key_of(s, self.by_app())).copied().unwrap_or(Colour::Grey)
    }

    /// Each project's (or app's) colour: in `Colour::ORDER`, longest first; none: grey.
    fn colours(&self) -> HashMap<&str, Colour> {
        let by_app = self.by_app();
        by_time(self.stretches(), |s| self.key_of(s, by_app))
            .into_iter()
            .enumerate()
            .map(|(i, (k, _))| (k, Colour::ORDER[i % Colour::ORDER.len()]))
            .collect()
    }

    /// The day bar; None when there's no stretch.
    pub fn bar(&self) -> Option<DayBar> {
        let first = self.stretches().map(|s| s.start).min()?;
        let last = self.stretches().map(|s| s.end).max()?;
        let hour = |t: DateTime<FixedOffset>| t - chrono::Duration::seconds(t.timestamp().rem_euclid(3600)) - chrono::Duration::nanoseconds(t.timestamp_subsec_nanos() as i64);
        let from = hour(first);
        let to = hour(last) + chrono::Duration::hours(1);
        let total = (to - from).num_milliseconds() as f64;
        let at = |t: DateTime<FixedOffset>| ((t - from).num_milliseconds() as f64 / total).clamp(0.0, 1.0);
        let mut hours = vec![];
        let mut h = from;
        while h <= to {
            hours.push((at(h), hm(h)));
            h += chrono::Duration::hours(1);
        }
        let by_app = self.by_app();
        let colours = self.colours();
        let segments = self
            .stretches()
            .map(|s| Segment { from: at(s.start), to: at(s.end), colour: colours.get(self.key_of(s, by_app)).copied().unwrap_or(Colour::Grey), key: s.key() })
            .collect();
        Some(DayBar { from, to, hours, segments })
    }

    /// Every recording, each with only the stretches with a read that matches the search (`found`: those reads) and
    /// the notes that match; no gaps. Empty: the whole day, as it is.
    pub fn search(&self, query: &str) -> Day {
        let words = words(query);
        if words.is_empty() {
            return self.clone();
        }
        let blocks = self
            .blocks
            .iter()
            .map(|b| {
                let items: Vec<Item> = b
                    .items
                    .iter()
                    .filter_map(|i| match i {
                        Item::Stretch(s) => {
                            let found: Vec<Moment> = s.reads.iter().filter(|m| m.has(&words)).cloned().collect();
                            (!found.is_empty()).then(|| Item::Stretch(Stretch { found: Some(found), ..s.clone() }))
                        }
                        Item::Note(m) => m.has(&words).then(|| i.clone()),
                        Item::Gap(_) => None,
                    })
                    .collect();
                Block { items, ..b.without_items() }
            })
            .collect();
        Day { date: self.date, blocks }
    }
}

impl Block {
    fn without_items(&self) -> Block {
        Block { session: self.session.clone(), start: self.start, end: self.end, live: self.live, items: vec![] }
    }
}

/// The search's words, as search compares them.
fn words(query: &str) -> Vec<String> {
    searchable(query).split_whitespace().map(String::from).collect()
}

fn hm(t: DateTime<FixedOffset>) -> String {
    t.format("%H:%M").to_string()
}

/// "10:02 – 11:30"; within one minute: "10:02".
fn span(a: DateTime<FixedOffset>, b: DateTime<FixedOffset>) -> String {
    let (x, y) = (hm(a), hm(b));
    if x == y {
        x
    } else {
        format!("{x} – {y}")
    }
}

/// Lower case, without accents, full-width letters as plain ones: what search compares.
pub fn searchable(s: &str) -> String {
    s.nfkd().filter(|c| !is_combining_mark(*c)).collect::<String>().to_lowercase()
}

/// The first `FOLD_LINES` lines that aren't blank (trimmed; longer than `FOLD_WIDTH` cut with "…"), and whether
/// anything was left out. The Mac's `Timeline.fold`.
pub fn fold(text: &str) -> (String, bool) {
    let mut kept: Vec<String> = vec![];
    let mut more = false;
    for raw in text.split(['\n', '\r', '\u{2028}', '\u{2029}', '\u{85}', '\u{0B}', '\u{0C}']) {
        let line = raw.trim_matches([' ', '\t']);
        if line.is_empty() {
            continue;
        }
        if kept.len() == FOLD_LINES {
            more = true;
            break;
        }
        if line.chars().count() > FOLD_WIDTH {
            kept.push(cut(line, FOLD_WIDTH - 1));
            more = true;
        } else {
            kept.push(line.to_string());
        }
    }
    (kept.join("\n"), more)
}

/// The first line of 20 or more characters (letters as seen, trimmed), cut at 120 with "…"; None if no line is
/// that long. The Mac's `Timeline.peek`.
fn peek(text: &str) -> Option<String> {
    let line = text.split(['\n', '\r', '\u{2028}', '\u{2029}', '\u{85}', '\u{0B}', '\u{0C}']).map(str::trim).find(|l| l.graphemes(true).count() >= PEEK_MIN)?;
    match line.grapheme_indices(true).nth(PEEK_MAX) {
        Some((i, _)) => Some(format!("{}…", &line[..i])),
        None => Some(line.to_string()),
    }
}

/// At most `n` characters, then "…" when cut.
fn cut(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// A session id ("20261019-100103", local time) as a time in `tz`.
pub fn session_start<Tz: TimeZone>(id: &str, tz: &Tz) -> Option<DateTime<FixedOffset>> {
    if !is_session_id(id) {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(id, "%Y%m%d-%H%M%S").ok()?;
    tz.from_local_datetime(&naive).earliest().map(|t| t.fixed_offset())
}

/// "YYYYMMDD-HHMMSS" and nothing else (never a path).
pub fn is_session_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 15 && b.iter().enumerate().all(|(i, c)| if i == 8 { *c == b'-' } else { c.is_ascii_digit() })
}

/// Midnight to midnight of `date` in `tz`.
fn day_bounds<Tz: TimeZone>(date: NaiveDate, tz: &Tz) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let at = |d: NaiveDate| tz.from_local_datetime(&d.and_time(NaiveTime::MIN)).earliest().map(|t| t.with_timezone(&Utc));
    Some((at(date)?, at(date.succ_opt()?)?))
}

/// What state.json says is recording now (its session id).
pub fn recording_session(paths: &Paths) -> Option<String> {
    let data = std::fs::read(paths.state()).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&data).ok()?;
    if v.get("recording")?.as_bool()? {
        v.get("session_id")?.as_str().filter(|s| !s.is_empty()).map(String::from)
    } else {
        None
    }
}

/// The day in local time.
pub fn load_day(paths: &Paths, date: NaiveDate) -> Day {
    let projects = Projects::load(&paths.projects());
    load_day_in(&paths.sessions(), recording_session(paths).as_deref(), date, &Local, &projects)
}

/// The day in `tz`: every recording that started that day or the day before and has something on it (or
/// started that day), with that day's reads (in stretches), notes and gaps. `live`: the session being recorded now.
pub fn load_day_in<Tz: TimeZone>(sessions: &Path, live: Option<&str>, date: NaiveDate, tz: &Tz, projects: &[Project]) -> Day {
    let mut day = Day { date, blocks: vec![] };
    let Some((from, to)) = day_bounds(date, tz) else { return day };
    let Ok(dir) = std::fs::read_dir(sessions) else { return day };
    let before = date.pred_opt().unwrap_or(date);
    let mut ids: Vec<String> = dir
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|id| session_start(id, tz).is_some_and(|s| s.date_naive() == date || s.date_naive() == before))
        .collect();
    ids.sort();
    let local = |t: DateTime<Utc>| t.with_timezone(tz).fixed_offset();
    for id in ids {
        let Some(begun) = session_start(&id, tz) else { continue };
        let started_today = begun.date_naive() == date;
        let folder = sessions.join(&id);
        let fixes = Assignments::load(&folder.join("assign.json"));

        let text = std::fs::read_to_string(folder.join("events.jsonl")).unwrap_or_default();
        let mut parts = vec![];
        let mut times: Vec<DateTime<Utc>> = vec![];
        for (index, line) in text.split('\n').enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(serde_json::Value::Object(obj)) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            let s = |k: &str| obj.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let Some(t) = parse_iso(&s("t")) else { continue };
            if t < from || t >= to {
                continue;
            }
            let end = parse_iso(&s("end")).unwrap_or(t).max(t);
            let kind = s("kind");
            let is_note = kind == "voice" || kind == "note";
            // The same order as Home and the Claude side: a correction, the recorder's choice, then clues.
            let mut project = s("project");
            if let Some(fixed) = Assignments::at(&fixes, t) {
                project = fixed.to_string();
            } else if project.is_empty() && !(is_note && s("app").is_empty()) {
                if let Some(p) = Projects::detect(projects, &s("title"), &s("where"), &s("app")) {
                    project = p.id.clone();
                }
            }
            let m = Moment {
                session: id.clone(),
                index,
                line: line.to_string(),
                t_raw: s("t"),
                t: local(t),
                end: local(end),
                app: s("app"),
                bundle: s("bundle"),
                title: s("title"),
                location: s("where"),
                project,
                kind: if is_note { kind } else { String::new() },
                src: s("src"),
                focus: s("focus"),
                text: s("text"),
            };
            times.extend([t, end]);
            parts.push(Part::Moment(m));
        }
        // The gaps that overlap the day, cut to it.
        let gaps = std::fs::read_to_string(folder.join("gaps.jsonl")).unwrap_or_default();
        for line in gaps.split('\n').filter(|l| !l.trim().is_empty()) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            let s = |k: &str| v.get(k).and_then(|x| x.as_str());
            let (Some(t), Some(why)) = (s("t").and_then(parse_iso), s("why")) else { continue };
            let end = s("end").and_then(parse_iso).unwrap_or(t).max(t);
            if end < from || t >= to {
                continue;
            }
            let (t, end) = (t.max(from), end.min(to));
            times.extend([t, end]);
            parts.push(Part::Gap(Gap { session: id.clone(), t: local(t), end: local(end), why: why.to_string() }));
        }
        if parts.is_empty() && !started_today {
            continue;
        }
        parts.sort_by_key(Part::t); // stable: file order within the same second
        let start = times.iter().min().copied().map(local).unwrap_or(begun);
        let mut end = times.iter().max().copied().map(local).unwrap_or(start);
        if times.is_empty() {
            let ended = std::fs::read(folder.join("session.json"))
                .ok()
                .and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok())
                .and_then(|v| v.get("ended_at").and_then(|x| x.as_str()).and_then(parse_iso));
            if let Some(ended) = ended {
                end = local(ended.min(to)).max(start);
            }
        }
        day.blocks.push(Block {
            session: id.clone(),
            start,
            end,
            live: live == Some(id.as_str()),
            items: group(parts),
        });
    }
    day.blocks.sort_by_key(|b| b.start);
    day
}

/// The days that have a recording (from the folders' names), newest first.
pub fn days_with_recordings(sessions: &Path) -> Vec<NaiveDate> {
    let mut days: Vec<NaiveDate> = std::fs::read_dir(sessions)
        .map(|d| d.flatten().filter_map(|e| session_start(&e.file_name().to_string_lossy(), &Local).map(|t| t.date_naive())).collect())
        .unwrap_or_default();
    days.sort();
    days.dedup();
    days.reverse();
    days
}

// MARK: delete

/// Why a delete didn't happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// That recording is still going: stop first.
    Recording,
    /// It's not there (any more), or the name isn't a recording's.
    NotFound,
    Io(String),
}

impl Refused {
    /// (title, body) for a message.
    pub fn message(&self) -> (String, String) {
        match self {
            Refused::Recording => (STOP_FIRST.into(), STOP_FIRST_BODY.into()),
            Refused::NotFound => (ALREADY_GONE.into(), ALREADY_GONE_BODY.into()),
            Refused::Io(e) => (CANT_DELETE.into(), e.clone()),
        }
    }
}

/// Remove one read (or note): the first line of events.jsonl that is exactly its line (and has its `t`), with its
/// newline; the file is written back whole (temp file + rename). Where it was is tried first. Not while recording it.
pub fn delete_read(paths: &Paths, m: &Moment) -> Result<(), Refused> {
    delete_reads(paths, &m.session, std::slice::from_ref(m))
}

/// Remove a whole stretch: all its reads' lines, or none if one of them isn't there any more.
pub fn delete_stretch(paths: &Paths, s: &Stretch) -> Result<(), Refused> {
    delete_reads(paths, &s.session, &s.reads)
}

fn delete_reads(paths: &Paths, session: &str, reads: &[Moment]) -> Result<(), Refused> {
    if !is_session_id(session) || reads.is_empty() || reads.iter().any(|m| m.session != session) {
        return Err(Refused::NotFound);
    }
    if recording_session(paths).as_deref() == Some(session) {
        return Err(Refused::Recording);
    }
    let file = paths.session_dir(session).join("events.jsonl");
    let text = std::fs::read_to_string(&file).map_err(|_| Refused::NotFound)?;
    let lines: Vec<&str> = text.split('\n').collect();
    let mut gone = vec![false; lines.len()];
    for m in reads {
        let has_t = serde_json::from_str::<serde_json::Value>(&m.line).ok().and_then(|v| v.get("t").and_then(|x| x.as_str()).map(|x| x == m.t_raw)) == Some(true);
        if !has_t {
            return Err(Refused::NotFound);
        }
        let free = |i: usize| lines[i] == m.line && !gone[i];
        let at = if m.index < lines.len() && free(m.index) { Some(m.index) } else { (0..lines.len()).find(|i| free(*i)) };
        let Some(at) = at else { return Err(Refused::NotFound) };
        gone[at] = true;
    }
    let kept: Vec<&str> = lines.iter().zip(&gone).filter(|(_, g)| !**g).map(|(l, _)| *l).collect();
    write_atomically(&file, &kept.join("\n")).map_err(|e| Refused::Io(e.to_string()))
}

/// Remove a whole recording (its folder). Not while recording it.
pub fn delete_session(paths: &Paths, session: &str) -> Result<(), Refused> {
    if !is_session_id(session) {
        return Err(Refused::NotFound);
    }
    if recording_session(paths).as_deref() == Some(session) {
        return Err(Refused::Recording);
    }
    let dir = paths.session_dir(session);
    if !dir.is_dir() {
        return Err(Refused::NotFound);
    }
    std::fs::remove_dir_all(&dir).map_err(|e| Refused::Io(e.to_string()))
}

// MARK: choosing the day

/// The day control's date: "Thursday 23 October"; the year too when it isn't this year ("Monday 20 October 2025").
pub fn day_title(date: NaiveDate, today: NaiveDate) -> String {
    use chrono::Datelike;
    date.format(if date.year() == today.year() { "%A %-d %B" } else { "%A %-d %B %Y" }).to_string()
}

/// ‹: the day before.
pub fn day_before(date: NaiveDate) -> NaiveDate {
    date.pred_opt().unwrap_or(date)
}

/// ›: the day after; None from today on (no future days).
pub fn day_after(date: NaiveDate, today: NaiveDate) -> Option<NaiveDate> {
    date.succ_opt().filter(|d| *d <= today)
}

/// The calendar's title: "October 2026".
pub fn month_title(year: i32, month: u32) -> String {
    NaiveDate::from_ymd_opt(year, month, 1).map(|d| d.format("%B %Y").to_string()).unwrap_or_default()
}

/// The calendar's weekday heads, Monday first.
pub const WEEKDAYS: [&str; 7] = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

/// A month as the calendar shows it: its weeks, Monday to Sunday; None outside the month.
pub fn month_weeks(year: i32, month: u32) -> Vec<[Option<NaiveDate>; 7]> {
    use chrono::Datelike;
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else { return vec![] };
    let mut weeks = vec![];
    let mut week = [None; 7];
    let mut d = first;
    while d.month() == month {
        let col = d.weekday().num_days_from_monday() as usize;
        week[col] = Some(d);
        if col == 6 {
            weeks.push(week);
            week = [None; 7];
        }
        match d.succ_opt() {
            Some(n) => d = n,
            None => break,
        }
    }
    if week.iter().any(Option::is_some) {
        weeks.push(week);
    }
    weeks
}

/// The days of a month that have a recording (1–31, in order): the calendar's dots.
pub fn recorded_in_month(recorded: &[NaiveDate], year: i32, month: u32) -> Vec<u32> {
    use chrono::Datelike;
    let mut days: Vec<u32> = recorded.iter().filter(|d| d.year() == year && d.month() == month).map(|d| d.day()).collect();
    days.sort_unstable();
    days.dedup();
    days
}
