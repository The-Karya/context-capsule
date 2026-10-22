// context-capsule — core: the Timeline — everything recorded on one day, raw: a block per recording (session),
// its moments (screen text, "Looked at" windows, voice and typed notes) and the stretches that weren't read
// (gaps.jsonl), in time order; search; deleting a moment or a whole recording. The words are here too, so
// Windows and Linux say the same as the Mac (SPEC.md, "Timeline"; `Timeline` in capture/Sources/Core.swift and
// TimelineView.swift).
//! Nothing here leaves the computer. Times are shown in local time; session ids are local, events UTC.

use crate::capsules::project_name;
use crate::gaps::Why;
use crate::json::write_atomically;
use crate::paths::Paths;
use crate::projects::{Project, Projects};
use crate::stats::{parse_iso, Assignments, WorkStats};
use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use std::collections::HashMap;
use std::path::Path;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

// MARK: words

pub const TIMELINE: &str = "Timeline";
pub const SEE_TODAYS_TIMELINE: &str = "See today's timeline";
pub const TODAY: &str = "Today";
pub const YESTERDAY: &str = "Yesterday";
pub const PICK_A_DAY: &str = "Pick a day";
pub const SEARCH_DAY: &str = "Search this day";
pub const SHOW_ALL: &str = "Show all";
pub const SHOW_LESS: &str = "Show less";
pub const SHOW_MORE: &str = "Show more";
pub const LOOKED_AT: &str = "Looked at";
pub const LOOKED_AT_HELP: &str = "The mouse was over it on another screen. You weren't typing in it.";
pub const RECORDING_NOW: &str = "Recording now";
pub const MAKE_FROM_THIS: &str = "Make a capsule from this";
pub const MAKE_FROM_THIS_HELP: &str = "Your AI makes a capsule from this recording.";
pub const DELETE: &str = "Delete";
pub const DELETE_MOMENT_MENU: &str = "Delete this moment…";
pub const DELETE_BLOCK_MENU: &str = "Delete this recording…";
pub const CANCEL: &str = "Cancel";
pub const LOADING: &str = "Reading…";
pub const NOTHING_NEW: &str = "Nothing new on screen";
pub const NOTHING_IN_IT: &str = "Nothing was recorded.";
pub const NO_MATCHES: &str = "Nothing on this day matches.";
pub const NOTHING_ON_DAY: &str = "Nothing recorded on this day.";
pub const DELETE_MOMENT: &str = "Delete this moment? It can't be undone.";
pub const DELETE_BLOCK: &str = "Delete this recording? It can't be undone.";
pub const STOP_FIRST: &str = "Stop first";
pub const STOP_FIRST_BODY: &str = "This recording is still going. Stop it, then delete.";
pub const ALREADY_GONE: &str = "Already gone";
pub const ALREADY_GONE_BODY: &str = "It isn't there anymore. The page is up to date now.";
pub const CANT_DELETE: &str = "Couldn't delete";

/// The line at the top of the page. `this_computer`: "this PC" / "this computer".
pub fn intro(this_computer: &str) -> String {
    format!("Everything recorded on this day, as it was read. It stays on {this_computer}.")
}

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

/// Under "Delete this moment? It can't be undone."
pub fn delete_moment_detail(this_computer: &str) -> String {
    format!("It's removed from {this_computer}. Capsules already made stay as they are.")
}

/// Under "Delete this recording? It can't be undone."
pub fn delete_block_detail(b: &Block, this_computer: &str) -> String {
    format!(
        "Everything recorded from {} to {} is removed from {this_computer}. Capsules already made stay as they are.",
        hm(b.start),
        hm(b.end)
    )
}

/// What "Make a capsule from this" asks Your AI (it uses get_session_activity).
pub fn session_prompt(session: &str) -> String {
    format!("Use Context Capsule to make a capsule from my session {session}.")
}

/// How many moments, notes and gaps the page shows before "Show more" (and how many more each time).
pub const PAGE: usize = 150;
/// Moment text is folded to this many lines, with Show all.
pub const FOLD_LINES: usize = 3;
/// …each at most this many characters.
pub const FOLD_WIDTH: usize = 300;
/// Show all shows at most this much (a moment can hold 100,000 characters).
pub const SHOW_ALL_CHARS: usize = 40_000;
/// "Off" between recordings a minute or more apart.
const OFF_MIN_SECONDS: i64 = 60;

// MARK: the day

/// One events.jsonl line, as the timeline shows it.
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
    /// "" for a screen moment, "voice" or "note".
    pub kind: String,
    pub src: String,
    pub focus: String,
    pub text: String,
}

/// Identifies a moment on the page (Show all, Delete).
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
        let words: Vec<String> = searchable(query).split_whitespace().map(String::from).collect();
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
    /// The title, else the app, else "A window".
    pub fn heading(&self) -> String {
        for s in [&self.title, &self.app] {
            if !s.trim().is_empty() {
                return s.trim().to_string();
            }
        }
        "A window".into()
    }
    /// The line under the title: the app (when the title is shown), where (address or file), the project's name.
    pub fn details(&self, projects: &[Project]) -> String {
        let mut parts = vec![];
        if !self.title.trim().is_empty() && !self.app.trim().is_empty() {
            parts.push(self.app.trim().to_string());
        }
        if !self.location.is_empty() {
            parts.push(self.location.clone());
        }
        if !self.project.is_empty() {
            parts.push(project_name(projects, &self.project));
        }
        parts.join(" · ")
    }
    /// The text to show: a screen moment's first lines (`all`: all of it, up to `SHOW_ALL_CHARS`); a note's words.
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

/// Most items are moments: not boxed.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Moment(Moment),
    Gap(Gap),
}

impl Item {
    pub fn t(&self) -> DateTime<FixedOffset> {
        match self {
            Item::Moment(m) => m.t,
            Item::Gap(g) => g.t,
        }
    }
}

/// One recording (session), as far as it is on the day.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub session: String,
    /// From its first moment or gap that day to its last (else when it started).
    pub start: DateTime<FixedOffset>,
    pub end: DateTime<FixedOffset>,
    /// Still recording.
    pub live: bool,
    /// The project it was mostly about (time in windows in use).
    pub project: Option<String>,
    /// Screen moments (notes not counted), and notes.
    pub moments: usize,
    pub notes: usize,
    /// This day's moments and gaps, in time order.
    pub items: Vec<Item>,
}

impl Block {
    /// "10:02 – 11:30 · 12 moments · Website".
    pub fn line(&self, projects: &[Project]) -> String {
        let mut parts = vec![span(self.start, self.end), format!("{} moment{}", self.moments, if self.moments == 1 { "" } else { "s" })];
        if let Some(p) = &self.project {
            parts.push(project_name(projects, p));
        }
        parts.join(" · ")
    }

    pub fn moments(&self) -> impl Iterator<Item = &Moment> {
        self.items.iter().filter_map(|i| if let Item::Moment(m) = i { Some(m) } else { None })
    }
}

/// One day: its recordings in time order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Day {
    pub date: NaiveDate,
    pub blocks: Vec<Block>,
}

/// What the page shows, top to bottom: a recording's header, its moments and gaps, and "Off" between recordings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row<'a> {
    Block(&'a Block),
    Item(&'a Block, &'a Item),
    /// "Off  11:30 – 13:05": no recording for a minute or more.
    Off(String),
}

impl Day {
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    pub fn find(&self, key: &MomentKey) -> Option<&Moment> {
        self.blocks.iter().flat_map(|b| b.moments()).find(|m| m.session == key.0 && m.index == key.1)
    }

    pub fn block(&self, session: &str) -> Option<&Block> {
        self.blocks.iter().find(|b| b.session == session)
    }

    /// Only the moments that match the search (no gaps); recordings with none are left out. Empty: the whole day.
    pub fn search(&self, query: &str) -> Day {
        if query.split_whitespace().next().is_none() {
            return self.clone();
        }
        let blocks = self
            .blocks
            .iter()
            .filter_map(|b| {
                let items: Vec<Item> = b.items.iter().filter(|i| matches!(i, Item::Moment(m) if m.matches(query))).cloned().collect();
                (!items.is_empty()).then(|| Block { items, ..b.clone() })
            })
            .collect();
        Day { date: self.date, blocks }
    }

    /// The page's rows, at most `limit` moments and gaps (the rest after "Show more"). Also: whether some were left out.
    pub fn rows(&self, limit: usize) -> (Vec<Row<'_>>, bool) {
        let mut rows = vec![];
        let mut shown = 0;
        for (i, b) in self.blocks.iter().enumerate() {
            if shown >= limit {
                return (rows, true);
            }
            if i > 0 {
                let before = &self.blocks[i - 1];
                if (b.start - before.end).num_seconds() >= OFF_MIN_SECONDS {
                    rows.push(Row::Off(format!("Off  {}", span(before.end, b.start))));
                }
            }
            rows.push(Row::Block(b));
            for item in &b.items {
                if shown >= limit {
                    return (rows, true);
                }
                rows.push(Row::Item(b, item));
                shown += 1;
            }
        }
        (rows, false)
    }
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
/// started that day), with that day's moments and gaps. `live`: the session being recorded now.
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
        let mut items = vec![];
        let mut times: Vec<DateTime<Utc>> = vec![];
        let mut time: HashMap<String, f64> = HashMap::new();
        let (mut moments, mut notes) = (0, 0);
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
            if m.is_note() {
                notes += 1;
            } else {
                moments += 1;
                if !m.looked_at() && !m.project.is_empty() {
                    *time.entry(m.project.clone()).or_default() += ((end - t).num_milliseconds() as f64 / 1000.0).max(WorkStats::MIN_SECONDS);
                }
            }
            times.extend([t, end]);
            items.push(Item::Moment(m));
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
            items.push(Item::Gap(Gap { session: id.clone(), t: local(t), end: local(end), why: why.to_string() }));
        }
        if items.is_empty() && !started_today {
            continue;
        }
        items.sort_by_key(|i| i.t()); // stable: file order within the same second
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
        let mut by: Vec<(String, f64)> = time.into_iter().collect();
        by.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        day.blocks.push(Block {
            session: id.clone(),
            start,
            end,
            live: live == Some(id.as_str()),
            project: by.first().map(|p| p.0.clone()),
            moments,
            notes,
            items,
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

/// Remove one moment: the first line of events.jsonl that is exactly `line` (and has this `t`), with its newline;
/// the file is written back whole (temp file + rename). `index` (where it was) is tried first. Not while recording it.
pub fn delete_moment(paths: &Paths, session: &str, index: usize, t: &str, line: &str) -> Result<(), Refused> {
    if !is_session_id(session) {
        return Err(Refused::NotFound);
    }
    if recording_session(paths).as_deref() == Some(session) {
        return Err(Refused::Recording);
    }
    let has_t = serde_json::from_str::<serde_json::Value>(line).ok().and_then(|v| v.get("t").and_then(|x| x.as_str()).map(|x| x == t)) == Some(true);
    if !has_t {
        return Err(Refused::NotFound);
    }
    let file = paths.session_dir(session).join("events.jsonl");
    let text = std::fs::read_to_string(&file).map_err(|_| Refused::NotFound)?;
    let lines: Vec<&str> = text.split('\n').collect();
    let at = if lines.get(index) == Some(&line) { Some(index) } else { lines.iter().position(|l| *l == line) };
    let Some(at) = at else { return Err(Refused::NotFound) };
    let kept: Vec<&str> = lines.iter().enumerate().filter(|(i, _)| *i != at).map(|(_, l)| *l).collect();
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

/// The day's label for the picker: "Today", "Yesterday", else "19 Oct".
pub fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    crate::capsules::day_label(&date.format("%Y-%m-%d").to_string(), today)
}

/// The days the picker offers: Today, Yesterday, then every other day with a recording (newest first, at most `max`).
pub fn picker_days(recorded: &[NaiveDate], today: NaiveDate, max: usize) -> Vec<NaiveDate> {
    let mut out = vec![today];
    if let Some(y) = today.pred_opt() {
        out.push(y);
    }
    for d in recorded {
        if out.len() >= max {
            break;
        }
        if !out.contains(d) && *d < today {
            out.push(*d);
        }
    }
    out
}
