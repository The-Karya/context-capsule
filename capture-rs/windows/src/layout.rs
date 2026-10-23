// context-capsule — windows: the Context Capsule window's layout — what goes where for Home, Timeline, Capsules, Projects,
// Settings (with Connect your AI and Your AI) and Help, as a flat list of shapes, words and controls in device-independent pixels. Pure (no Windows
// calls): text is measured through `Measure`, so it runs and is tested on any OS. `main_window` draws it.
//! The words and numbers come from core's `window` and `timeline` modules, the same as the Mac's window. Raw
//! recordings show only on the Timeline (everywhere else: time per project, counts and capsules).

use crate::ai::{self, AiState};
use crate::setup::{Done, Step};
use context_capsule_core::anyai::{self, AiClient};
use context_capsule_core::capsules::{self, CapsuleSummary};
use context_capsule_core::share_check::{self, Kind};
use context_capsule_core::settings::{self, Appearance};
use context_capsule_core::shortcuts::{self as keys, Which};
use context_capsule_core::timeline::{self as tl, Block, Colour, Day, Item, Moment, MomentKey, Stretch, StretchKey};
use context_capsule_core::window::{self as w, HomeNumbers, Keys, Section};
use context_capsule_core::{Config, Project, Status};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
    pub fn moved(&self, dy: f64) -> Rect {
        Rect { y: self.y + dy, ..*self }
    }
    #[cfg(test)]
    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
}

/// Text styles. Sizes in DIPs (see `Font::size`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Font {
    /// The greeting: large and light.
    Display,
    /// The page title.
    Title,
    /// Big numbers in the stat strip.
    Stat,
    /// The status card's line.
    Subtitle,
    Strong,
    Body,
    Small,
    SmallStrong,
    /// Segoe Fluent Icons / MDL2 Assets.
    Icon,
    IconSmall,
}

impl Font {
    /// (size in DIPs, weight, display face).
    pub fn spec(self) -> (f64, i32, bool) {
        match self {
            Font::Display => (34.0, 300, true),
            Font::Title => (26.0, 600, true),
            Font::Stat => (24.0, 600, true),
            Font::Subtitle => (18.0, 600, true),
            Font::Strong => (14.0, 600, false),
            Font::Body => (14.0, 400, false),
            Font::Small => (12.0, 400, false),
            Font::SmallStrong => (12.0, 600, false),
            Font::Icon => (16.0, 400, false),
            Font::IconSmall => (12.0, 400, false),
        }
    }
    pub fn is_icon(self) -> bool {
        matches!(self, Font::Icon | Font::IconSmall)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// A plain Windows 11 button.
    Standard,
    /// The accent-colored button (one main action per card).
    Accent,
    /// No border until hovered (links, icon buttons).
    Subtle,
    /// A sidebar item.
    Nav { selected: bool },
    /// A full-width list row ("Working on").
    Row { selected: bool },
    /// A button that opens a list of choices (a chevron on the right).
    Dropdown,
    /// A full-width row with a check box on the left ("Check before sharing").
    Check { on: bool },
    /// A day in the Timeline's calendar: centered, highlighted when it's the day shown.
    Cell { selected: bool },
}

/// The text boxes the window shows (real Windows edit controls, placed where the layout says).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Name,
    ProjectName,
    ProjectClues,
    /// Capsules: filters by title and "In short" as you type.
    Search,
    /// Timeline: filters the day's moments as you type.
    TimelineSearch,
}

impl Field {
    pub const ALL: [Field; 5] = [Field::Name, Field::ProjectName, Field::ProjectClues, Field::Search, Field::TimelineSearch];
}

/// What happens once "Check before sharing" says Share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareHow {
    CopyFile,
    CopyText,
    SaveCopy,
}

/// "Check before sharing": a capsule about to be shared, what was found in it, and which are checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareView {
    pub path: PathBuf,
    pub how: ShareHow,
    /// The capsule's text (never changed: Share uses a copy).
    pub text: String,
    pub items: Vec<(Kind, String)>,
    pub checked: Vec<bool>,
}

impl ShareView {
    /// None: nothing found, share straight away.
    pub fn new(path: PathBuf, how: ShareHow, text: String) -> Option<ShareView> {
        let items = share_check::find(&text).items();
        if items.is_empty() {
            return None;
        }
        let checked = vec![true; items.len()];
        Some(ShareView { path, how, text, items, checked })
    }

    /// The text to share: the checked items replaced by `[removed]`.
    pub fn shared_text(&self) -> String {
        let chosen: Vec<String> = self.items.iter().zip(&self.checked).filter(|(_, on)| **on).map(|((_, s), _)| s.clone()).collect();
        share_check::remove(&self.text, &chosen)
    }
}

/// What a button does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Nav(Section),
    ToggleRecord,
    Voice,
    AddScreen,
    /// None: find by itself.
    Pick(Option<String>),
    NewProject,
    Make,
    SeeAll,
    Open(PathBuf),
    /// Copy file, Copy text, Save a copy…, Show in folder.
    More(PathBuf),
    Edit(String),
    Delete(String),
    SaveEdit(String),
    CancelEdit,
    Setup(Step),
    Remind,
    RemindAt,
    Keep,
    Sounds,
    /// Settings → Every day → When you start: pick a screen or window, or all my screens.
    ShareScreens,
    Language,
    Appearance,
    RemoveApp(String),
    AddApp,
    /// Settings → Updates.
    AutoUpdate,
    CheckUpdates,
    /// Capsules: a capsule someone sent (a file dialog), and your AI searching them.
    OpenCapsule,
    Ask,
    /// Any AI: Copy my day, Save from clipboard (Make follows Your AI, and is Copy my day for a website).
    CopyMyDay,
    SaveClipboard,
    /// Settings → Connect your AI: Connect an app; its ⋯ menu (Disconnect); the "More" fold.
    AiConnect(String),
    AiMenu(String),
    AiMore,
    /// Settings → Your AI: the choice.
    YourAi,
    /// Check before sharing: one item's check box, Share, Cancel.
    ShareToggle(usize),
    ShareGo,
    ShareCancel,
    /// Settings → Shortcuts: record new keys for one, or put back the defaults.
    Shortcut(Which),
    ResetShortcuts,
    /// Timeline: the day control (‹, the date's calendar, ›, Today; the calendar's month and its days).
    DayBefore,
    DayAfter,
    DayToday,
    Calendar,
    CalendarMonth(i32),
    PickDay(chrono::NaiveDate),
    /// Timeline: open or close a recording; a day bar segment (opens its recording, scrolls to the stretch); Show
    /// what was read and more of it; Show all / Show less; delete a read or note, a stretch or a whole recording
    /// (after a confirm); make a capsule from a recording with Your AI.
    ToggleBlock(String),
    OpenStretch(StretchKey),
    ShowReads(StretchKey),
    MoreReads(StretchKey),
    ShowAll(MomentKey),
    DeleteRead(MomentKey),
    DeleteStretch(StretchKey),
    DeleteBlock(String),
    MakeFrom(String),
    /// Home → "See today's timeline".
    SeeTimeline,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// A rounded panel.
    Card(Rect),
    /// A thin line.
    Divider(Rect),
    /// `lines`: 1 = one line, cut with "…"; 0 = wrap as needed; n = wrap, at most n lines.
    Text { r: Rect, text: String, font: Font, ink: Ink, align: Align, lines: u8 },
    Button { r: Rect, action: Action, label: String, icon: Option<&'static str>, style: Style, enabled: bool, check: bool },
    Toggle { r: Rect, action: Action, on: bool },
    /// One project's share of time.
    Bar { r: Rect, fraction: f64, muted: bool },
    /// The status dot: red and filled while contextualizing, else an outline.
    Dot { r: Rect, on: bool },
    /// A key on the keyboard ("Alt", "C").
    Key { r: Rect, text: String },
    /// The small status pill (top right).
    Pill { r: Rect, text: String, live: bool },
    Field { r: Rect, field: Field },
    /// A round app badge with its first letter (Timeline).
    Badge { r: Rect, text: String },
    /// A speech bubble (a voice or typed note on the Timeline).
    Bubble(Rect),
    /// A rounded patch of a project's colour (the Timeline's day bar, its dots and time bars); None: the empty track.
    Swatch { r: Rect, colour: Option<Colour> },
}

impl Node {
    /// Buttons, switches and text boxes take clicks and keyboard focus.
    pub fn is_control(&self) -> bool {
        matches!(self, Node::Button { enabled: true, .. } | Node::Toggle { .. } | Node::Field { .. })
    }
    pub fn rect(&self) -> Rect {
        match self {
            Node::Card(r) | Node::Divider(r) | Node::Bubble(r) => *r,
            Node::Text { r, .. }
            | Node::Button { r, .. }
            | Node::Toggle { r, .. }
            | Node::Bar { r, .. }
            | Node::Dot { r, .. }
            | Node::Key { r, .. }
            | Node::Pill { r, .. }
            | Node::Badge { r, .. }
            | Node::Swatch { r, .. }
            | Node::Field { r, .. } => *r,
        }
    }
    pub fn action(&self) -> Option<&Action> {
        match self {
            Node::Button { action, enabled: true, .. } | Node::Toggle { action, .. } => Some(action),
            _ => None,
        }
    }
}

/// Text sizes, from the platform (GDI) or a fake in tests. Answers in DIPs.
pub trait Measure {
    /// (width, height) of `text` in `font`, wrapped at `width` when `lines` != 1 (at most `lines` lines, 0 = any).
    fn text(&self, text: &str, font: Font, width: f64, lines: u8) -> (f64, f64);
}

/// Icons (Segoe Fluent Icons; the same code points in Segoe MDL2 Assets).
pub mod glyph {
    pub const HOME: &str = "\u{E80F}";
    pub const CAPSULE: &str = "\u{E8A5}";
    pub const FOLDER: &str = "\u{E8B7}";
    pub const SETTINGS: &str = "\u{E713}";
    pub const HELP: &str = "\u{E897}";
    pub const CHECK: &str = "\u{E73E}";
    pub const ADD: &str = "\u{E710}";
    pub const MORE: &str = "\u{E712}";
    pub const CHEVRON: &str = "\u{E70D}";
    pub const CHEVRON_RIGHT: &str = "\u{E76C}";
    pub const CHEVRON_LEFT: &str = "\u{E76B}";
    pub const MIC: &str = "\u{E720}";
    pub const LINK: &str = "\u{E71B}";
    pub const POWER: &str = "\u{E7E8}";
    pub const HISTORY: &str = "\u{E81C}";
    pub const HIDE: &str = "\u{ED1A}";
    pub const KEYBOARD: &str = "\u{E765}";
}

pub fn section_icon(s: Section) -> &'static str {
    match s {
        Section::Home => glyph::HOME,
        Section::Timeline => glyph::HISTORY,
        Section::Capsules => glyph::CAPSULE,
        Section::Projects => glyph::FOLDER,
        Section::Settings => glyph::SETTINGS,
        Section::Help => glyph::HELP,
    }
}

pub fn step_icon(s: Step) -> &'static str {
    match s {
        Step::Voice => glyph::MIC,
        Step::Claude => glyph::LINK,
        Step::Login => glyph::POWER,
    }
}

/// What the window knows from the files (loaded off the UI thread).
#[derive(Clone, Debug, Default)]
pub struct Data {
    pub home: HomeNumbers,
    pub capsules: Vec<CapsuleSummary>,
    pub projects: Vec<Project>,
    pub config: Config,
    pub appearance: Appearance,
}

/// Everything one layout needs.
pub struct View<'a> {
    pub section: Section,
    pub status: &'a Status,
    pub data: &'a Data,
    pub now: SystemTime,
    pub hour: u32,
    /// "Tuesday, 20 October", as the user's clock writes dates.
    pub date: &'a str,
    pub today: chrono::NaiveDate,
    /// The project being edited on Projects.
    pub editing: Option<&'a str>,
    pub setup: Done,
    pub several_screens: bool,
    /// Built with voice notes.
    pub voice: bool,
    /// "Update automatically" is on; None: no Updates section.
    pub auto_update: Option<bool>,
    pub keys: &'a Keys,
    /// What's typed in Capsules' search box.
    pub search: &'a str,
    /// "Check before sharing", shown instead of the page while a capsule is about to be shared.
    pub share: Option<&'a ShareView>,
    /// Settings → Shortcuts: the one waiting for new keys, and a short line under one row (why it was refused).
    pub recording_keys: Option<Which>,
    pub keys_note: Option<(Which, &'a str)>,
    /// The AI apps and Your AI (Settings, Home's Make today's capsules, Capsules, Help).
    pub ai: &'a AiState,
    /// The Timeline's day and what's open on it.
    pub timeline: &'a TimelineView,
}

/// One day read for the Timeline (off the UI thread): the day asked for, what's in it, and the days with recordings.
#[derive(Clone, Debug)]
pub struct TimelineData {
    pub date: chrono::NaiveDate,
    pub day: Day,
    pub recorded: Vec<chrono::NaiveDate>,
}

/// The Timeline page's state: the day shown (None while it's read), its search, and what's open on it — kept
/// when Today is read again while recording.
#[derive(Clone, Debug)]
pub struct TimelineView {
    pub date: chrono::NaiveDate,
    day: Option<Day>,
    /// The day filtered by `search`, while searching (kept, so a big day isn't searched again on every repaint).
    shown: Option<Day>,
    pub search: String,
    /// Recordings opened or closed by hand (open = the default, flipped).
    pub toggled: HashSet<String>,
    /// Stretches with Show what was read open, and how many of their reads show.
    pub reads_open: HashSet<StretchKey>,
    pub reads_limit: HashMap<StretchKey, usize>,
    /// Reads opened with Show all.
    pub expanded: HashSet<MomentKey>,
    pub recorded: Vec<chrono::NaiveDate>,
    /// The month calendar under the date, while it's open: (year, month).
    pub calendar: Option<(i32, u32)>,
}

/// For testing: `--show <page> [YYYY-MM-DD] [<recording id>]` opens the window at launch on that page (the
/// Timeline on that day, with that recording open), like the Mac's `--show-…` flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowArg {
    pub section: Section,
    pub date: Option<chrono::NaiveDate>,
    pub open: Option<String>,
}

pub fn show_arg(args: &[String]) -> Option<ShowArg> {
    let i = args.iter().position(|a| a == "--show")?;
    let page = args.get(i + 1)?;
    let section = Section::ALL.into_iter().find(|s| page.eq_ignore_ascii_case(s.title()))?;
    let mut show = ShowArg { section, date: None, open: None };
    for a in &args[i + 2..] {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(a, "%Y-%m-%d") {
            show.date = Some(d);
        } else if a.len() == 15 && chrono::NaiveDateTime::parse_from_str(a, "%Y%m%d-%H%M%S").is_ok() {
            show.open = Some(a.clone());
        } else {
            break;
        }
    }
    Some(show)
}

impl TimelineView {
    pub fn new(date: chrono::NaiveDate) -> TimelineView {
        TimelineView {
            date,
            day: None,
            shown: None,
            search: String::new(),
            toggled: HashSet::new(),
            reads_open: HashSet::new(),
            reads_limit: HashMap::new(),
            expanded: HashSet::new(),
            recorded: vec![],
            calendar: None,
        }
    }

    /// Another day: nothing shown until it's read.
    pub fn go_to(&mut self, date: chrono::NaiveDate) {
        self.calendar = None;
        if date != self.date {
            self.date = date;
            self.day = None;
            self.shown = None;
            self.toggled.clear();
            self.reads_open.clear();
            self.reads_limit.clear();
            self.expanded.clear();
        }
    }

    /// The day was read (ignored if another day was picked meanwhile).
    pub fn set(&mut self, data: TimelineData) {
        self.recorded = data.recorded;
        if data.date == self.date {
            self.shown = self.searching().then(|| data.day.search(&self.search));
            self.day = Some(data.day);
        }
    }

    pub fn set_search(&mut self, search: &str) {
        if search != self.search {
            self.search = search.to_string();
            self.shown = if self.searching() { self.day.as_ref().map(|d| d.search(search)) } else { None };
        }
    }

    pub fn searching(&self) -> bool {
        self.search.split_whitespace().next().is_some()
    }

    /// What the recordings list shows (the search applied); None while reading.
    pub fn shown(&self) -> Option<&Day> {
        if self.searching() {
            self.shown.as_ref()
        } else {
            self.day.as_ref()
        }
    }

    /// The whole day (the header, the day bar and Where your time went; deleting).
    pub fn day(&self) -> Option<&Day> {
        self.day.as_ref()
    }

    /// A recording is open: by default the one still recording (while searching: those with a match), unless
    /// opened or closed by hand.
    pub fn is_open(&self, b: &Block) -> bool {
        let default = if self.searching() { !b.items.is_empty() } else { b.live };
        default != self.toggled.contains(&b.session)
    }

    /// Open or close a recording.
    pub fn toggle(&mut self, session: &str) {
        if !self.toggled.remove(session) {
            self.toggled.insert(session.to_string());
        }
    }

    /// The day bar: open the stretch's recording.
    pub fn open_stretch(&mut self, key: &StretchKey) {
        let closed = self.shown().and_then(|d| d.block(&key.0)).is_some_and(|b| !self.is_open(b));
        if closed {
            self.toggle(&key.0);
        }
    }

    /// The calendar's month, one back or on (never past this month).
    pub fn calendar_month(&mut self, delta: i32, today: chrono::NaiveDate) {
        use chrono::Datelike;
        let Some((y, m)) = self.calendar else { return };
        let n = y * 12 + m as i32 - 1 + delta;
        if n < today.year() * 12 + today.month() as i32 {
            self.calendar = Some((n.div_euclid(12), n.rem_euclid(12) as u32 + 1));
        }
    }
}

pub struct Layout {
    /// Fixed on the left.
    pub sidebar: Vec<Node>,
    /// Fixed at the top of the content (the page title and the status pill).
    pub header: Vec<Node>,
    /// Scrolls; y = 0 is just under the header.
    pub content: Vec<Node>,
    /// How tall the content is, with its bottom margin.
    pub height: f64,
    /// The content column, for the centered notice.
    pub column: Rect,
}

pub const SIDEBAR: f64 = 220.0;
pub const HEADER: f64 = 72.0;
const PAD: f64 = 32.0;
const GAP: f64 = 24.0;
const CAPTION_GAP: f64 = 8.0;
const CARD_PAD: f64 = 16.0;
const ROW: f64 = 48.0;
const BUTTON_H: f64 = 32.0;
const SIDE_GAP: f64 = 24.0;

/// Builds nodes top to bottom in one column.
struct Col<'m> {
    nodes: Vec<Node>,
    x: f64,
    w: f64,
    y: f64,
    m: &'m dyn Measure,
}

impl<'m> Col<'m> {
    fn new(x: f64, w: f64, y: f64, m: &'m dyn Measure) -> Col<'m> {
        Col { nodes: vec![], x, w, y, m }
    }

    fn text_h(&self, text: &str, font: Font, w: f64, lines: u8) -> f64 {
        self.m.text(text, font, w, lines).1
    }

    fn text_w(&self, text: &str, font: Font) -> f64 {
        self.m.text(text, font, 10_000.0, 1).0
    }

    /// A paragraph across the column.
    fn para(&mut self, text: &str, font: Font, ink: Ink) {
        let h = self.text_h(text, font, self.w, 0);
        self.nodes.push(Node::Text { r: Rect::new(self.x, self.y, self.w, h), text: text.into(), font, ink, align: Align::Left, lines: 0 });
        self.y += h;
    }

    /// A small grey caption above a card.
    fn caption(&mut self, text: &str) {
        let h = self.text_h(text, Font::SmallStrong, self.w, 1);
        self.nodes.push(Node::Text {
            r: Rect::new(self.x + 4.0, self.y, self.w - 4.0, h),
            text: text.into(),
            font: Font::SmallStrong,
            ink: Ink::Secondary,
            align: Align::Left,
            lines: 1,
        });
        self.y += h + CAPTION_GAP;
    }

    /// A caption with a small link on the right ("See all").
    fn caption_with(&mut self, text: &str, link: Option<(&str, Action)>) {
        if let Some((label, action)) = link {
            let lw = self.text_w(label, Font::Small) + 16.0;
            let h = self.text_h(text, Font::SmallStrong, self.w, 1);
            self.nodes.push(Node::Button {
                r: Rect::new(self.x + self.w - lw, self.y - 6.0, lw, h + 10.0),
                action,
                label: label.into(),
                icon: None,
                style: Style::Subtle,
                enabled: true,
                check: false,
            });
        }
        self.caption(text);
    }

    fn gap(&mut self, h: f64) {
        self.y += h;
    }

    /// A card holding rows; `rows` builds them from the card's inner left edge and width, starting at y, and
    /// returns the bottom.
    fn card(&mut self, rows: impl FnOnce(&mut Vec<Node>, f64, f64, f64, &dyn Measure) -> f64) {
        let top = self.y;
        let at = self.nodes.len();
        self.nodes.push(Node::Card(Rect::default()));
        let mut inner = vec![];
        let bottom = rows(&mut inner, self.x, self.w, top, self.m);
        self.nodes[at] = Node::Card(Rect::new(self.x, top, self.w, bottom - top));
        self.nodes.extend(inner);
        self.y = bottom;
    }

    fn button(&mut self, x: f64, label: &str, action: Action, style: Style) -> f64 {
        let w = button_width(self.m, label, style);
        self.nodes.push(Node::Button { r: Rect::new(x, self.y, w, BUTTON_H), action, label: label.into(), icon: None, style, enabled: true, check: false });
        w
    }
}

pub fn button_width(m: &dyn Measure, label: &str, style: Style) -> f64 {
    let extra = if style == Style::Dropdown { 44.0 } else { 24.0 };
    (m.text(label, Font::Body, 10_000.0, 1).0 + extra).max(if style == Style::Subtle { 0.0 } else { 80.0 })
}

/// One row inside a card: a label on the left (and an optional second line), and something on the right
/// placed by `right(x_right, row_top, row_h) -> width used`.
#[allow(clippy::too_many_arguments)]
fn row(
    out: &mut Vec<Node>,
    m: &dyn Measure,
    x: f64,
    w: f64,
    y: f64,
    icon: Option<&'static str>,
    label: &str,
    help: Option<&str>,
    right_w: f64,
) -> (f64, f64) {
    let left = x + CARD_PAD + if icon.is_some() { 36.0 } else { 0.0 };
    let text_w = (x + w - CARD_PAD - right_w - 12.0 - left).max(40.0);
    let lh = m.text(label, Font::Body, text_w, 1).1;
    let hh = help.map_or(0.0, |h| m.text(h, Font::Small, text_w, 2).1);
    let h = (lh + hh + if help.is_some() { 2.0 } else { 0.0 } + 24.0).max(ROW);
    let mut ty = y + (h - lh - hh - if help.is_some() { 2.0 } else { 0.0 }) / 2.0;
    if let Some(i) = icon {
        out.push(Node::Text { r: Rect::new(x + CARD_PAD, y, 24.0, h), text: i.into(), font: Font::Icon, ink: Ink::Secondary, align: Align::Center, lines: 1 });
    }
    out.push(Node::Text { r: Rect::new(left, ty, text_w, lh), text: label.into(), font: Font::Body, ink: Ink::Primary, align: Align::Left, lines: 1 });
    if let Some(help) = help {
        ty += lh + 2.0;
        out.push(Node::Text { r: Rect::new(left, ty, text_w, hh), text: help.into(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 2 });
    }
    (y + h, h)
}

fn divider(out: &mut Vec<Node>, x: f64, w: f64, y: f64) {
    out.push(Node::Divider(Rect::new(x + CARD_PAD, y, w - 2.0 * CARD_PAD, 1.0)));
}

#[allow(clippy::too_many_arguments)]
fn right_button(out: &mut Vec<Node>, m: &dyn Measure, x: f64, w: f64, top: f64, h: f64, label: &str, action: Action, style: Style) {
    let bw = button_width(m, label, style);
    out.push(Node::Button {
        r: Rect::new(x + w - CARD_PAD - bw, top + (h - BUTTON_H) / 2.0, bw, BUTTON_H),
        action,
        label: label.into(),
        icon: None,
        style,
        enabled: true,
        check: false,
    });
}

const TOGGLE_W: f64 = 40.0;
const TOGGLE_H: f64 = 20.0;

fn right_toggle(out: &mut Vec<Node>, x: f64, w: f64, top: f64, h: f64, on: bool, action: Action) {
    out.push(Node::Toggle { r: Rect::new(x + w - CARD_PAD - TOGGLE_W, top + (h - TOGGLE_H) / 2.0, TOGGLE_W, TOGGLE_H), action, on });
}

/// The whole window at `width` × `height` DIPs.
pub fn layout(v: &View, width: f64, height: f64, m: &dyn Measure) -> Layout {
    let sidebar = sidebar(v.section, height, m);
    let area = (width - SIDEBAR).max(0.0);
    let avail = (area - 2.0 * PAD).max(200.0);
    let (cw, wide) = w::content_width(avail);
    let x0 = SIDEBAR + PAD + (avail - cw) / 2.0;

    // The page title, and the pill on the right.
    let mut header = vec![];
    let pill = w::status_pill(v.status, v.now, v.data.home.today.seconds);
    let pw = m.text(&pill, Font::Small, 1000.0, 1).0 + 24.0;
    let ph = 26.0;
    header.push(Node::Pill { r: Rect::new(x0 + cw - pw, (HEADER - ph) / 2.0 + 4.0, pw, ph), text: pill, live: v.status.recording });
    // Home's greeting is its title.
    let title = if v.share.is_some() { w::CHECK_TITLE } else { v.section.title() };
    let th = m.text(title, Font::Title, cw - pw - 16.0, 1).1;
    if v.section != Section::Home || v.share.is_some() {
        header.push(Node::Text {
            r: Rect::new(x0, (HEADER - th) / 2.0 + 4.0, cw - pw - 16.0, th),
            text: title.into(),
            font: Font::Title,
            ink: Ink::Primary,
            align: Align::Left,
            lines: 1,
        });
    }

    let mut col = Col::new(x0, cw, 8.0, m);
    match (v.share, v.section) {
        (Some(share), _) => share_page(&mut col, share),
        (None, Section::Home) => home(&mut col, v, wide),
        (None, Section::Timeline) => timeline_page(&mut col, v),
        (None, Section::Capsules) => capsules_page(&mut col, v),
        (None, Section::Projects) => projects_page(&mut col, v),
        (None, Section::Settings) => settings_page(&mut col, v),
        (None, Section::Help) => help_page(&mut col, v),
    }
    let height = col.y + PAD;
    Layout { sidebar, header, content: col.nodes, height, column: Rect::new(x0, 0.0, cw, 0.0) }
}

fn sidebar(section: Section, _height: f64, m: &dyn Measure) -> Vec<Node> {
    let mut out = vec![];
    let (x, w) = (8.0, SIDEBAR - 16.0);
    let mut y = 16.0;
    let mut group = "";
    for s in Section::ALL {
        if s.group() != group {
            group = s.group();
            if y > 16.0 {
                y += 12.0;
            }
            let h = m.text(group, Font::SmallStrong, w, 1).1;
            out.push(Node::Text { r: Rect::new(x + 12.0, y, w - 12.0, h), text: group.into(), font: Font::SmallStrong, ink: Ink::Secondary, align: Align::Left, lines: 1 });
            y += h + 6.0;
        }
        out.push(Node::Button {
            r: Rect::new(x, y, w, 36.0),
            action: Action::Nav(s),
            label: s.title().into(),
            icon: Some(section_icon(s)),
            style: Style::Nav { selected: s == section },
            enabled: true,
            check: false,
        });
        y += 40.0;
    }
    out
}

// MARK: Home

fn home(col: &mut Col, v: &View, wide: bool) {
    let greeting = w::greeting(v.hour, &v.data.home.first_name);
    let h = col.text_h(&greeting, Font::Display, col.w, 0);
    col.nodes.push(Node::Text { r: Rect::new(col.x, col.y, col.w, h), text: greeting, font: Font::Display, ink: Ink::Primary, align: Align::Left, lines: 0 });
    col.y += h + 2.0;
    col.para(v.date, Font::Body, Ink::Secondary);
    col.gap(20.0);

    // The stat strip: three big numbers in one panel.
    let stats = w::stats(&v.data.home);
    col.card(|out, x, w, top, m| {
        let cell = w / 3.0;
        let lh = m.text("Today", Font::Small, cell, 1).1;
        let vh = m.text("0 m", Font::Stat, cell, 1).1;
        for (i, s) in stats.iter().enumerate() {
            let cx = x + cell * i as f64 + CARD_PAD;
            let iw = cell - 2.0 * CARD_PAD;
            let mut y = top + 14.0;
            out.push(Node::Text { r: Rect::new(cx, y, iw, lh), text: s.label.into(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
            y += lh + 2.0;
            out.push(Node::Text { r: Rect::new(cx, y, iw, vh), text: s.value.clone(), font: Font::Stat, ink: Ink::Primary, align: Align::Left, lines: 1 });
            y += vh;
            out.push(Node::Text { r: Rect::new(cx, y, iw, lh), text: s.detail.clone(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
            if i > 0 {
                out.push(Node::Divider(Rect::new(x + cell * i as f64, top + 14.0, 1.0, lh * 2.0 + vh + 2.0)));
            }
        }
        top + 14.0 + lh * 2.0 + vh + 2.0 + 14.0
    });
    col.gap(GAP);

    let (x0, total, top) = (col.x, col.w, col.y);
    let main_w = if wide { total - w::SIDE_WIDTH - SIDE_GAP } else { total };
    col.w = main_w;
    status_card(col, v);
    col.gap(GAP);
    col.caption_with(w::TODAY, Some((tl::SEE_TODAYS_TIMELINE, Action::SeeTimeline)));
    today_card(col, v);
    col.gap(GAP);
    recent(col, v);
    let main_bottom = col.y;
    if wide {
        col.x = x0 + main_w + SIDE_GAP;
        col.w = w::SIDE_WIDTH;
        col.y = top;
    } else {
        col.gap(GAP);
    }
    side(col, v);
    col.y = col.y.max(main_bottom);
    col.x = x0;
    col.w = total;
}

fn status_card(col: &mut Col, v: &View) {
    let s = v.status;
    let title = w::status_title(s, v.now);
    let detail = w::status_detail(s, v.keys, v.data.config.share_screens);
    let toggle = w::toggle_button(s);
    let now_line = w::now_line(s);
    let several = v.several_screens;
    col.card(|out, x, w, top, m| {
        let pad = 18.0;
        let bw = button_width(m, toggle, Style::Accent);
        let th = m.text(&title, Font::Subtitle, w, 1).1;
        let line_h = th.max(BUTTON_H);
        let tx = x + pad + 20.0;
        out.push(Node::Dot { r: Rect::new(x + pad, top + pad + (line_h - 10.0) / 2.0, 10.0, 10.0), on: s.recording });
        out.push(Node::Text {
            r: Rect::new(tx, top + pad + (line_h - th) / 2.0, (w - pad * 2.0 - 20.0 - bw - 12.0).max(40.0), th),
            text: title,
            font: Font::Subtitle,
            ink: Ink::Primary,
            align: Align::Left,
            lines: 1,
        });
        out.push(Node::Button {
            r: Rect::new(x + w - pad - bw, top + pad + (line_h - BUTTON_H) / 2.0, bw, BUTTON_H),
            action: Action::ToggleRecord,
            label: toggle.into(),
            icon: None,
            style: Style::Accent,
            enabled: true,
            check: false,
        });
        let mut y = top + pad + line_h + 8.0;
        let inner = w - 2.0 * pad;
        let dh = m.text(&detail, Font::Body, inner, 0).1;
        out.push(Node::Text { r: Rect::new(x + pad, y, inner, dh), text: detail, font: Font::Body, ink: Ink::Secondary, align: Align::Left, lines: 0 });
        y += dh;
        if let Some(line) = now_line {
            let nh = m.text(&line, Font::Small, inner, 1).1;
            out.push(Node::Text { r: Rect::new(x + pad, y + 2.0, inner, nh), text: line, font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
            y += nh + 2.0;
        }
        if s.recording {
            y += 12.0;
            let mut bx = x + pad;
            let voice = w::voice_button(s);
            let vw = button_width(m, voice, Style::Standard);
            out.push(Node::Button { r: Rect::new(bx, y, vw, BUTTON_H), action: Action::Voice, label: voice.into(), icon: None, style: Style::Standard, enabled: true, check: false });
            bx += vw + 8.0;
            if several {
                let aw = button_width(m, w::ADD_SCREEN, Style::Standard);
                out.push(Node::Button {
                    r: Rect::new(bx, y, aw, BUTTON_H),
                    action: Action::AddScreen,
                    label: w::ADD_SCREEN.into(),
                    icon: None,
                    style: Style::Standard,
                    enabled: s.can_add_screen(),
                    check: false,
                });
            }
            y += BUTTON_H;
        }
        y + pad
    });
}

fn today_card(col: &mut Col, v: &View) {
    let bars = w::bars(&v.data.home.today, &v.data.projects);
    col.card(|out, x, w, top, m| {
        if bars.is_empty() {
            let h = m.text(w::TODAY_EMPTY, Font::Body, w - 2.0 * CARD_PAD, 0).1;
            out.push(Node::Text { r: Rect::new(x + CARD_PAD, top + CARD_PAD, w - 2.0 * CARD_PAD, h), text: w::TODAY_EMPTY.into(), font: Font::Body, ink: Ink::Secondary, align: Align::Left, lines: 0 });
            return top + CARD_PAD * 2.0 + h;
        }
        let name_w = ((w - 2.0 * CARD_PAD) * 0.32).clamp(80.0, 170.0);
        let time_w = 72.0;
        let mut y = top + CARD_PAD;
        for b in bars {
            let lh = m.text(&b.name, Font::Body, name_w, 1).1.max(20.0);
            let ink = if b.muted { Ink::Secondary } else { Ink::Primary };
            out.push(Node::Text { r: Rect::new(x + CARD_PAD, y, name_w, lh), text: b.name.clone(), font: Font::Body, ink, align: Align::Left, lines: 1 });
            let bx = x + CARD_PAD + name_w + 12.0;
            let bw = (w - 2.0 * CARD_PAD - name_w - 12.0 - time_w - 12.0).max(20.0);
            out.push(Node::Bar { r: Rect::new(bx, y + (lh - 8.0) / 2.0, bw, 8.0), fraction: b.fraction, muted: b.muted });
            out.push(Node::Text { r: Rect::new(x + w - CARD_PAD - time_w, y, time_w, lh), text: b.time.clone(), font: Font::Small, ink: Ink::Secondary, align: Align::Right, lines: 1 });
            y += lh + 10.0;
        }
        y - 10.0 + CARD_PAD
    });
}

fn recent(col: &mut Col, v: &View) {
    let list = &v.data.home.recent;
    let link = (!list.is_empty()).then_some((w::SEE_ALL, Action::SeeAll));
    col.caption_with(w::RECENT_CAPSULES, link);
    if list.is_empty() {
        col.card(|out, x, w, top, m| {
            let h = m.text(w::NONE_YET, Font::Body, w, 1).1;
            out.push(Node::Text { r: Rect::new(x + CARD_PAD, top + CARD_PAD, w - 2.0 * CARD_PAD, h), text: w::NONE_YET.into(), font: Font::Body, ink: Ink::Secondary, align: Align::Left, lines: 1 });
            top + 2.0 * CARD_PAD + h
        });
    } else {
        capsule_card(col, list, v.today);
    }
}

fn side(col: &mut Col, v: &View) {
    // Shortcuts, with keycaps.
    col.caption(w::SHORTCUTS);
    let shortcuts = w::home_shortcuts(v.keys);
    col.card(|out, x, w, top, m| {
        let mut y = top + 12.0;
        for (keys, what) in shortcuts {
            let kx = keycaps(out, m, x + CARD_PAD, y, &keys);
            let th = m.text(what, Font::Body, w, 1).1;
            out.push(Node::Text { r: Rect::new(kx + 8.0, y + (24.0 - th) / 2.0, (x + w - CARD_PAD - kx - 8.0).max(20.0), th), text: what.into(), font: Font::Body, ink: Ink::Primary, align: Align::Left, lines: 1 });
            y += 32.0;
        }
        y - 8.0 + 12.0
    });
    col.gap(GAP);

    // Working on: a list, the chosen one checked.
    col.caption(w::WORKING_ON);
    let chips = w::chips(v.status);
    col.card(|out, x, w, top, _m| {
        let mut y = top + 6.0;
        for c in chips {
            out.push(Node::Button {
                r: Rect::new(x + 6.0, y, w - 12.0, 34.0),
                action: Action::Pick(c.id.clone()),
                label: c.label.clone(),
                icon: None,
                style: Style::Row { selected: c.selected },
                enabled: true,
                check: c.selected,
            });
            y += 36.0;
        }
        out.push(Node::Button {
            r: Rect::new(x + 6.0, y, w - 12.0, 34.0),
            action: Action::NewProject,
            label: w::NEW_PROJECT.into(),
            icon: Some(glyph::ADD),
            style: Style::Row { selected: false },
            enabled: true,
            check: false,
        });
        y + 34.0 + 6.0
    });
    col.gap(GAP);

    // Make today's capsules, the way Your AI works: Copy my day and Save from clipboard for a website or any other
    // AI, else one button (Claude or another app opens with the request).
    let ai = v.ai;
    let kind = ai.kind();
    let hint = w::make_hint(&kind, &ai.name(), ai.day_line.as_deref());
    let your = kind.home_line(&ai.clients);
    col.card(|out, x, w, top, m| {
        let inner = w - 2.0 * CARD_PAD;
        let mut y = top + CARD_PAD;
        let th = m.text(w::MAKE_TITLE, Font::Strong, inner, 0).1;
        out.push(Node::Text { r: Rect::new(x + CARD_PAD, y, inner, th), text: w::MAKE_TITLE.into(), font: Font::Strong, ink: Ink::Primary, align: Align::Left, lines: 0 });
        y += th + 2.0;
        let hh = m.text(&hint, Font::Small, inner, 0).1;
        out.push(Node::Text { r: Rect::new(x + CARD_PAD, y, inner, hh), text: hint.clone(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 0 });
        y += hh + 12.0;
        let full = |action: Action, label: &str, style: Style, enabled: bool, y: f64| Node::Button {
            r: Rect::new(x + CARD_PAD, y, inner, BUTTON_H),
            action,
            label: label.into(),
            icon: None,
            style,
            enabled,
            check: false,
        };
        if kind.uses_copy() {
            out.push(full(Action::Make, anyai::COPY_MY_DAY, Style::Accent, !ai.working, y));
            y += BUTTON_H + 8.0;
            out.push(full(Action::SaveClipboard, anyai::SAVE_FROM_CLIPBOARD, Style::Standard, !ai.working, y));
        } else {
            out.push(full(Action::Make, anyai::MAKE_CAPSULES, Style::Accent, true, y));
        }
        y += BUTTON_H + 10.0;
        // "Your AI: Cursor  Change"
        let lw = button_width(m, anyai::CHANGE, Style::Subtle);
        let yh = m.text(&your, Font::Small, inner, 1).1;
        let tw = m.text(&your, Font::Small, (inner - lw - 4.0).max(40.0), 1).0;
        out.push(Node::Text { r: Rect::new(x + CARD_PAD, y + 4.0, tw, yh), text: your.clone(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
        out.push(Node::Button {
            r: Rect::new(x + CARD_PAD + tw + 4.0, y, lw, yh + 8.0),
            action: Action::Nav(Section::Settings),
            label: anyai::CHANGE.into(),
            icon: None,
            style: Style::Subtle,
            enabled: true,
            check: false,
        });
        y + yh + 8.0 + CARD_PAD - 4.0
    });
}

/// "Alt+C" as two keys; returns the right edge.
fn keycaps(out: &mut Vec<Node>, m: &dyn Measure, x: f64, y: f64, keys: &str) -> f64 {
    let mut kx = x;
    for part in keys.split('+') {
        let kw = (m.text(part, Font::SmallStrong, 200.0, 1).0 + 14.0).max(26.0);
        out.push(Node::Key { r: Rect::new(kx, y, kw, 24.0), text: part.into() });
        kx += kw + 4.0;
    }
    kx - 4.0
}

// MARK: Capsules

/// Rows of capsules in one card: icon, name, "In short", the day, Open and ⋯.
fn capsule_card(col: &mut Col, list: &[CapsuleSummary], today: chrono::NaiveDate) {
    col.card(|out, x, w, top, m| {
        let mut y = top;
        for (i, c) in list.iter().enumerate() {
            if i > 0 {
                divider(out, x, w, y);
            }
            let open_w = button_width(m, w::OPEN, Style::Standard);
            let more_w = 36.0;
            let when = c.when(today);
            let when_w = if when.is_empty() { 0.0 } else { m.text(&when, Font::Small, 200.0, 1).0 + 12.0 };
            let left = x + CARD_PAD + 32.0;
            let text_w = (x + w - CARD_PAD - open_w - more_w - 8.0 - 12.0 - when_w - left).max(60.0);
            let label = c.label(today);
            let label = if c.draft { format!("{label}  ·  {}", w::DRAFT) } else { label };
            let lh = m.text(&label, Font::Strong, text_w, 1).1;
            let sh = if c.in_short.is_empty() { 0.0 } else { m.text(&c.in_short, Font::Small, text_w, 2).1 + 2.0 };
            let h = (lh + sh + 24.0).max(56.0);
            let ty = y + (h - lh - sh) / 2.0;
            let icon = if c.kind == "project" { glyph::FOLDER } else { glyph::CAPSULE };
            out.push(Node::Text { r: Rect::new(x + CARD_PAD, y, 20.0, h), text: icon.into(), font: Font::Icon, ink: Ink::Secondary, align: Align::Center, lines: 1 });
            out.push(Node::Text { r: Rect::new(left, ty, text_w, lh), text: label, font: Font::Strong, ink: Ink::Primary, align: Align::Left, lines: 1 });
            if sh > 0.0 {
                out.push(Node::Text { r: Rect::new(left, ty + lh + 2.0, text_w, sh - 2.0), text: c.in_short.clone(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 2 });
            }
            let mut rx = x + w - CARD_PAD - more_w;
            out.push(Node::Button {
                r: Rect::new(rx, y + (h - BUTTON_H) / 2.0, more_w, BUTTON_H),
                action: Action::More(c.path.clone()),
                label: String::new(),
                icon: Some(glyph::MORE),
                style: Style::Subtle,
                enabled: true,
                check: false,
            });
            rx -= open_w + 4.0;
            out.push(Node::Button {
                r: Rect::new(rx, y + (h - BUTTON_H) / 2.0, open_w, BUTTON_H),
                action: Action::Open(c.path.clone()),
                label: w::OPEN.into(),
                icon: None,
                style: Style::Standard,
                enabled: true,
                check: false,
            });
            if !when.is_empty() {
                let wh = m.text(&when, Font::Small, when_w, 1).1;
                out.push(Node::Text { r: Rect::new(rx - when_w - 4.0, y + (h - wh) / 2.0, when_w, wh), text: when, font: Font::Small, ink: Ink::Secondary, align: Align::Right, lines: 1 });
            }
            y += h;
        }
        y
    });
}

fn capsules_page(col: &mut Col, v: &View) {
    let mw = col.button(col.x, w::MAKE_TODAYS, Action::Make, Style::Standard);
    col.button(col.x + mw + 8.0, w::OPEN_CAPSULE, Action::OpenCapsule, Style::Standard);
    col.gap(BUTTON_H + 16.0);

    // Search as you type, or ask your AI (it reads inside every capsule).
    let asking = !v.search.trim().is_empty();
    let ask = anyai::ask_button(&v.ai.kind());
    let aw = button_width(col.m, ask, Style::Accent);
    col.nodes.push(Node::Field { r: Rect::new(col.x, col.y, (col.w - aw - 8.0).max(80.0), BUTTON_H), field: Field::Search });
    col.nodes.push(Node::Button {
        r: Rect::new(col.x + col.w - aw, col.y, aw, BUTTON_H),
        action: Action::Ask,
        label: ask.into(),
        icon: None,
        style: Style::Accent,
        enabled: asking,
        check: false,
    });
    col.gap(BUTTON_H + 12.0);
    copy_row(col, v.ai);
    col.gap(GAP);

    if v.data.capsules.is_empty() {
        col.para(&w::no_capsules(v.keys), Font::Body, Ink::Secondary);
        return;
    }
    let shown = capsules::search(&v.data.capsules, v.search);
    if shown.is_empty() {
        col.para(w::NO_MATCHES, Font::Body, Ink::Secondary);
        return;
    }
    for (i, g) in capsules::groups(&shown, &v.data.projects).iter().enumerate() {
        if i > 0 {
            col.gap(GAP);
        }
        col.caption(&g.title);
        capsule_card(col, &g.items, v.today);
    }
}

/// Capsules: for an AI on the web or any other AI — the line (or what to do next after Copy my day), then Copy my
/// day and Save from clipboard. Too narrow for one row: the line above the buttons.
fn copy_row(col: &mut Col, ai: &AiState) {
    let line = ai.day_line.as_deref().unwrap_or(anyai::ON_THE_WEB);
    let cw = button_width(col.m, anyai::COPY_MY_DAY, Style::Standard);
    let sw = button_width(col.m, anyai::SAVE_FROM_CLIPBOARD, Style::Standard);
    let buttons = cw + 8.0 + sw;
    let room = col.w - buttons - 12.0;
    let mut bx = col.x + col.w - buttons;
    if room >= 160.0 {
        let lh = col.text_h(line, Font::Small, room, 2);
        col.nodes.push(Node::Text { r: Rect::new(col.x, col.y + ((BUTTON_H - lh) / 2.0).max(0.0), room, lh), text: line.into(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 2 });
        col.y += (lh - BUTTON_H).max(0.0) / 2.0;
    } else {
        col.para(line, Font::Small, Ink::Secondary);
        col.gap(8.0);
        bx = col.x;
    }
    for (label, action, w) in [(anyai::COPY_MY_DAY, Action::CopyMyDay, cw), (anyai::SAVE_FROM_CLIPBOARD, Action::SaveClipboard, sw)] {
        col.nodes.push(Node::Button { r: Rect::new(bx, col.y, w, BUTTON_H), action, label: label.into(), icon: None, style: Style::Standard, enabled: !ai.working, check: false });
        bx += w + 8.0;
    }
    col.gap(BUTTON_H);
}

/// "Check before sharing": what was found, by group, each with a check box (all checked), then Cancel and Share.
fn share_page(col: &mut Col, share: &ShareView) {
    col.para(w::CHECK_INTRO, Font::Body, Ink::Secondary);
    col.gap(GAP);
    let mut at = 0;
    for kind in Kind::ALL {
        let rows: Vec<(usize, &str)> =
            share.items.iter().enumerate().filter(|(_, (k, _))| *k == kind).map(|(i, (_, s))| (i, s.as_str())).collect();
        if rows.is_empty() {
            continue;
        }
        if at > 0 {
            col.gap(GAP);
        }
        at += 1;
        col.caption(kind.title());
        col.card(|out, x, w, top, _m| {
            let mut y = top + 6.0;
            for (i, text) in &rows {
                out.push(Node::Button {
                    r: Rect::new(x + 6.0, y, w - 12.0, 34.0),
                    action: Action::ShareToggle(*i),
                    label: (*text).into(),
                    icon: None,
                    style: Style::Check { on: share.checked.get(*i).copied().unwrap_or(false) },
                    enabled: true,
                    check: false,
                });
                y += 36.0;
            }
            y - 2.0 + 6.0
        });
    }
    col.gap(GAP);
    let sw = button_width(col.m, w::SHARE, Style::Accent);
    let cw = button_width(col.m, w::CANCEL, Style::Standard);
    let right = col.x + col.w;
    col.button(right - sw - 8.0 - cw, w::CANCEL, Action::ShareCancel, Style::Standard);
    col.button(right - sw, w::SHARE, Action::ShareGo, Style::Accent);
    col.gap(BUTTON_H);
}

// MARK: Timeline

/// The Timeline, top to bottom (SPEC "Timeline"; the Mac's TimelineView.swift): the day control (‹ date ›, Today)
/// and search, the month calendar while it's open, "1 h 50 min recorded · 3 recordings", the day bar, Where your
/// time went, then the recordings, each folded to one line (open: its stretches, notes and gaps in one card).
fn timeline_page(col: &mut Col, v: &View) {
    let tv = v.timeline;
    day_control(col, v);
    if let Some((year, month)) = tv.calendar {
        calendar(col, v, year, month);
    }
    let (Some(day), Some(shown)) = (tv.day(), tv.shown()) else {
        col.para(tl::LOADING, Font::Body, Ink::Secondary);
        return;
    };
    col.para(&day.summary(), Font::Small, Ink::Secondary);
    col.gap(GAP);
    if day.is_empty() {
        col.para(&tl::empty_line(false, tv.date == v.today, &v.keys.record), Font::Body, Ink::Secondary);
        return;
    }
    if let Some(bar) = day.bar() {
        col.card(|out, x, w, top, m| day_bar(out, m, x, w, top, &bar));
        col.gap(GAP);
        col.caption(tl::WHERE_TIME_WENT);
        let shares = day.time_went(&v.data.projects);
        col.card(|out, x, w, top, m| time_went(out, m, x, w, top, &shares));
        col.gap(GAP);
    }
    col.caption(tl::RECORDINGS);
    if tv.searching() && shown.blocks.iter().all(|b| b.items.is_empty()) {
        col.para(tl::NO_MATCHES, Font::Body, Ink::Secondary);
        col.gap(12.0);
    }
    for b in &shown.blocks {
        let open = tv.is_open(b);
        // Its line from the whole recording (a search leaves only part of it).
        block_header(col, day.block(&b.session).unwrap_or(b), open, &v.data.projects);
        if open {
            if b.items.is_empty() {
                let none = if tv.searching() { tl::NO_MATCHES } else { tl::NOTHING_IN_IT };
                col.para(none, Font::Small, Ink::Secondary);
            } else {
                col.card(|out, x, w, top, m| timeline_items(out, m, x, w, top, b, day, tv, &v.data.projects));
            }
            col.gap(12.0);
        }
        col.gap(4.0);
    }
}

/// "‹  Thursday 23 October  ›" (the date opens the calendar), Today when another day is shown, and the search box.
fn day_control(col: &mut Col, v: &View) {
    let tv = v.timeline;
    let title = tl::day_title(tv.date, v.today);
    let mut x = col.x;
    let icon = |x: f64, glyph: &'static str, action: Action, enabled: bool| Node::Button {
        r: Rect::new(x, col.y, BUTTON_H, BUTTON_H),
        action,
        label: String::new(),
        icon: Some(glyph),
        style: Style::Subtle,
        enabled,
        check: false,
    };
    let back = icon(x, glyph::CHEVRON_LEFT, Action::DayBefore, true);
    col.nodes.push(back);
    x += BUTTON_H + 4.0;
    let dw = button_width(col.m, &title, Style::Dropdown);
    col.nodes.push(Node::Button { r: Rect::new(x, col.y, dw, BUTTON_H), action: Action::Calendar, label: title, icon: None, style: Style::Dropdown, enabled: true, check: false });
    x += dw + 4.0;
    let on = icon(x, glyph::CHEVRON_RIGHT, Action::DayAfter, tl::day_after(tv.date, v.today).is_some());
    col.nodes.push(on);
    x += BUTTON_H + 8.0;
    if tv.date != v.today {
        x += col.button(x, tl::TODAY, Action::DayToday, Style::Standard) + 8.0;
    }
    let room = col.x + col.w - x;
    if room >= 200.0 {
        let fw = room.min(320.0);
        col.nodes.push(Node::Field { r: Rect::new(col.x + col.w - fw, col.y, fw, BUTTON_H), field: Field::TimelineSearch });
        col.gap(BUTTON_H + 8.0);
    } else {
        col.gap(BUTTON_H + 8.0);
        col.nodes.push(Node::Field { r: Rect::new(col.x, col.y, col.w, BUTTON_H), field: Field::TimelineSearch });
        col.gap(BUTTON_H + 8.0);
    }
}

/// Calendar cells.
const CELL_W: f64 = 40.0;
const CELL_H: f64 = 34.0;

/// The month calendar under the date: ‹ October 2026 ›, the weekdays, then its days (a dot on days with a
/// recording; days after today can't be picked). Picking a day closes it.
fn calendar(col: &mut Col, v: &View, year: i32, month: u32) {
    use chrono::Datelike;
    let tv = v.timeline;
    let dots = tl::recorded_in_month(&tv.recorded, year, month);
    let w = 7.0 * CELL_W + 2.0 * CARD_PAD;
    let saved = (col.x, col.w);
    col.w = w.min(col.w);
    col.card(|out, x, _w, top, m| {
        let mut y = top + 12.0;
        let left = x + CARD_PAD;
        let right = left + 7.0 * CELL_W;
        let arrow = |x: f64, y: f64, glyph: &'static str, delta: i32, enabled: bool| Node::Button {
            r: Rect::new(x, y, BUTTON_H, BUTTON_H),
            action: Action::CalendarMonth(delta),
            label: String::new(),
            icon: Some(glyph),
            style: Style::Subtle,
            enabled,
            check: false,
        };
        out.push(arrow(left, y, glyph::CHEVRON_LEFT, -1, true));
        let later = (year, month) < (v.today.year(), v.today.month());
        out.push(arrow(right - BUTTON_H, y, glyph::CHEVRON_RIGHT, 1, later));
        let title = tl::month_title(year, month);
        let th = m.text(&title, Font::Strong, right - left - 2.0 * BUTTON_H, 1).1;
        out.push(Node::Text { r: Rect::new(left + BUTTON_H, y + (BUTTON_H - th) / 2.0, right - left - 2.0 * BUTTON_H, th), text: title, font: Font::Strong, ink: Ink::Primary, align: Align::Center, lines: 1 });
        y += BUTTON_H + 6.0;
        for (i, head) in tl::WEEKDAYS.iter().enumerate() {
            let hh = m.text(head, Font::Small, CELL_W, 1).1;
            out.push(Node::Text { r: Rect::new(left + i as f64 * CELL_W, y, CELL_W, hh), text: (*head).into(), font: Font::Small, ink: Ink::Secondary, align: Align::Center, lines: 1 });
        }
        y += 20.0;
        for week in tl::month_weeks(year, month) {
            for (i, d) in week.iter().enumerate() {
                let Some(d) = d else { continue };
                let r = Rect::new(left + i as f64 * CELL_W + 2.0, y + 1.0, CELL_W - 4.0, CELL_H - 2.0);
                out.push(Node::Button { r, action: Action::PickDay(*d), label: d.day().to_string(), icon: None, style: Style::Cell { selected: *d == tv.date }, enabled: *d <= v.today, check: false });
                if dots.contains(&d.day()) {
                    out.push(Node::Swatch { r: Rect::new(r.x + r.w / 2.0 - 2.0, r.bottom() - 6.0, 4.0, 4.0), colour: Some(Colour::Grey) });
                }
            }
            y += CELL_H;
        }
        y + 10.0
    });
    (col.x, col.w) = saved;
    col.gap(12.0);
}

/// The day bar: the stretches in their colours from the first one's hour to the last one's, hour labels under it.
/// A segment opens its recording.
fn day_bar(out: &mut Vec<Node>, m: &dyn Measure, x: f64, w: f64, top: f64, bar: &tl::DayBar) -> f64 {
    let (left, width) = (x + CARD_PAD, w - 2.0 * CARD_PAD);
    let y = top + CARD_PAD + 4.0;
    let h = 14.0;
    out.push(Node::Swatch { r: Rect::new(left, y, width, h), colour: None });
    let mut taken = left;
    for s in &bar.segments {
        let sx = left + s.from * width;
        let sw = ((s.to - s.from) * width).max(3.0).min(left + width - sx);
        // The click area first (its hover shows around the colour), never over the one before.
        let cx = sx.max(taken);
        if sx + sw - cx >= 1.0 {
            out.push(Node::Button { r: Rect::new(cx, y - 4.0, sx + sw - cx, h + 8.0), action: Action::OpenStretch(s.key.clone()), label: String::new(), icon: None, style: Style::Subtle, enabled: true, check: false });
            taken = sx + sw;
        }
        out.push(Node::Swatch { r: Rect::new(sx, y, sw, h), colour: Some(s.colour) });
    }
    let ly = y + h + 6.0;
    let mut last_right = f64::MIN;
    let mut lh: f64 = 0.0;
    for (at, label) in &bar.hours {
        let (lw, th) = m.text(label, Font::Small, 100.0, 1);
        let lx = (left + at * width - lw / 2.0).clamp(left, left + width - lw);
        if lx < last_right + 8.0 {
            continue;
        }
        out.push(Node::Text { r: Rect::new(lx, ly, lw, th), text: label.clone(), font: Font::Small, ink: Ink::Secondary, align: Align::Center, lines: 1 });
        last_right = lx + lw;
        lh = lh.max(th);
    }
    ly + lh + CARD_PAD
}

/// Where your time went: a row per project (or app): its name, a bar in its colour, the time.
fn time_went(out: &mut Vec<Node>, m: &dyn Measure, x: f64, w: f64, top: f64, shares: &[tl::Share]) -> f64 {
    let name_w = ((w - 2.0 * CARD_PAD) * 0.32).clamp(80.0, 200.0);
    let time_w = 80.0;
    let mut y = top + CARD_PAD;
    for s in shares {
        let lh = m.text(&s.name, Font::Body, name_w, 1).1.max(20.0);
        let ink = if s.colour == Colour::Grey { Ink::Secondary } else { Ink::Primary };
        out.push(Node::Text { r: Rect::new(x + CARD_PAD, y, name_w, lh), text: s.name.clone(), font: Font::Body, ink, align: Align::Left, lines: 1 });
        let bx = x + CARD_PAD + name_w + 12.0;
        let bw = (w - 2.0 * CARD_PAD - name_w - 12.0 - time_w - 12.0).max(20.0);
        let by = y + (lh - 8.0) / 2.0;
        out.push(Node::Swatch { r: Rect::new(bx, by, bw, 8.0), colour: None });
        out.push(Node::Swatch { r: Rect::new(bx, by, (bw * s.share).max(6.0).min(bw), 8.0), colour: Some(s.colour) });
        out.push(Node::Text { r: Rect::new(x + w - CARD_PAD - time_w, y, time_w, lh), text: s.time.clone(), font: Font::Small, ink: Ink::Secondary, align: Align::Right, lines: 1 });
        y += lh + 10.0;
    }
    y - 10.0 + CARD_PAD
}

/// A recording on one line: ˃ "10:00 – 11:40 · 1 h 40 min · Website, Client report" (opens it; a red dot and
/// "Recording now" while it records), then Make a capsule and ⋯ (Delete this recording…) on the right.
fn block_header(col: &mut Col, b: &Block, open: bool, projects: &[Project]) {
    let mw = button_width(col.m, tl::MAKE_CAPSULE, Style::Standard);
    let dw = 36.0;
    let right = col.x + col.w;
    let live_w = if b.live { col.text_w(tl::RECORDING_NOW, Font::Small) + 24.0 } else { 0.0 };
    let row_w = (col.w - mw - dw - 12.0 - live_w).max(60.0);
    col.nodes.push(Node::Button {
        r: Rect::new(col.x, col.y, row_w, BUTTON_H),
        action: Action::ToggleBlock(b.session.clone()),
        label: b.line(projects),
        icon: Some(if open { glyph::CHEVRON } else { glyph::CHEVRON_RIGHT }),
        style: Style::Row { selected: false },
        enabled: true,
        check: false,
    });
    if b.live {
        let x = col.x + row_w + 8.0;
        col.nodes.push(Node::Dot { r: Rect::new(x, col.y + (BUTTON_H - 8.0) / 2.0, 8.0, 8.0), on: true });
        let sh = col.text_h(tl::RECORDING_NOW, Font::Small, live_w, 1);
        col.nodes.push(Node::Text { r: Rect::new(x + 14.0, col.y + (BUTTON_H - sh) / 2.0, live_w - 24.0, sh), text: tl::RECORDING_NOW.into(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
    }
    col.nodes.push(Node::Button { r: Rect::new(right - dw - 4.0 - mw, col.y, mw, BUTTON_H), action: Action::MakeFrom(b.session.clone()), label: tl::MAKE_CAPSULE.into(), icon: None, style: Style::Standard, enabled: true, check: false });
    col.nodes.push(Node::Button { r: Rect::new(right - dw, col.y, dw, BUTTON_H), action: Action::DeleteBlock(b.session.clone()), label: String::new(), icon: Some(glyph::MORE), style: Style::Subtle, enabled: true, check: false });
    col.gap(BUTTON_H + CAPTION_GAP);
}

/// The first letter of an app, for its badge.
pub fn initial(app: &str) -> String {
    app_label(app).chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "·".into())
}

/// An open recording's stretches, notes and gaps, one under the other; returns the bottom. `day`: the whole day
/// (the stretches' colours).
#[allow(clippy::too_many_arguments)]
fn timeline_items(out: &mut Vec<Node>, m: &dyn Measure, x: f64, w: f64, top: f64, b: &Block, day: &Day, tv: &TimelineView, projects: &[Project]) -> f64 {
    let mut y = top;
    let more_w = 32.0;
    let left = x + CARD_PAD + 36.0;
    for (n, item) in b.items.iter().enumerate() {
        if n > 0 {
            divider(out, x, w, y);
        }
        match item {
            // "Not read 10:40 – 10:55: a private app was in front"
            Item::Gap(g) => {
                let line = g.line();
                let inner = x + w - CARD_PAD - left;
                let h = m.text(&line, Font::Small, inner, 2).1;
                out.push(Node::Text { r: Rect::new(x + CARD_PAD, y + 8.0, 24.0, h.max(16.0)), text: glyph::HIDE.into(), font: Font::IconSmall, ink: Ink::Secondary, align: Align::Center, lines: 1 });
                out.push(Node::Text { r: Rect::new(left, y + 8.0, inner, h), text: line, font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 2 });
                y += h + 16.0;
            }
            // A voice or typed note: all its words in a speech bubble, then "You said · 10:14 · in Figma".
            Item::Note(mo) => {
                let bw = (x + w - CARD_PAD - more_w - 8.0 - left).max(80.0);
                let inner = bw - 24.0;
                let words = mo.shown_text(true);
                let words = if words.is_empty() { "…".to_string() } else { words };
                let wh = m.text(&words, Font::Body, inner, 0).1;
                let bh = 8.0 + wh + 8.0;
                let icon = if mo.kind == "voice" { glyph::MIC } else { glyph::KEYBOARD };
                out.push(Node::Text { r: Rect::new(x + CARD_PAD, y + 14.0, 24.0, 24.0), text: icon.into(), font: Font::Icon, ink: Ink::Secondary, align: Align::Center, lines: 1 });
                out.push(Node::Bubble(Rect::new(left, y + 10.0, bw, bh)));
                out.push(Node::Text { r: Rect::new(left + 12.0, y + 18.0, inner, wh), text: words, font: Font::Body, ink: Ink::Primary, align: Align::Left, lines: 0 });
                let caption = mo.note_caption();
                let ch = m.text(&caption, Font::Small, bw - 4.0, 1).1;
                out.push(Node::Text { r: Rect::new(left + 4.0, y + 10.0 + bh + 4.0, bw - 4.0, ch), text: caption, font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
                out.push(more_button(x, w, y + 10.0, Action::DeleteRead(mo.key())));
                y += 10.0 + bh + 4.0 + ch + 10.0;
            }
            Item::Stretch(s) => y = stretch_row(out, m, x, w, y, s, day, tv, projects),
        }
    }
    y
}

/// One stretch: the app's badge, its colour, "App · title" and "10:00 – 10:38 · 38 min"; where · project; the peek;
/// "24 reads" and Show what was read (then the reads); ⋯ (Delete this stretch…).
#[allow(clippy::too_many_arguments)]
fn stretch_row(out: &mut Vec<Node>, m: &dyn Measure, x: f64, w: f64, y: f64, s: &Stretch, day: &Day, tv: &TimelineView, projects: &[Project]) -> f64 {
    let more_w = 32.0;
    let left = x + CARD_PAD + 36.0;
    let inner = (x + w - CARD_PAD - more_w - 8.0 - left).max(60.0);
    let ty0 = y + 10.0;
    out.push(Node::Badge { r: Rect::new(x + CARD_PAD, ty0, 24.0, 24.0), text: initial(&s.app) });
    let time = format!("{} · {}", s.time(), s.length());
    let tw = m.text(&time, Font::Small, inner, 1).0;
    let heading = s.heading();
    let hh = m.text(&heading, Font::Strong, inner, 1).1;
    out.push(Node::Swatch { r: Rect::new(left, ty0 + 2.0 + (hh - 8.0) / 2.0, 8.0, 8.0), colour: Some(day.colour(s)) });
    let room = (inner - 14.0 - tw - 12.0).max(40.0);
    let hw = m.text(&heading, Font::Strong, room, 1).0.min(room);
    out.push(Node::Text { r: Rect::new(left + 14.0, ty0 + 2.0, hw, hh), text: heading, font: Font::Strong, ink: Ink::Primary, align: Align::Left, lines: 1 });
    let th = m.text(&time, Font::Small, tw, 1).1;
    out.push(Node::Text { r: Rect::new(left + inner - tw, ty0 + 2.0 + (hh - th) / 2.0, tw, th), text: time, font: Font::Small, ink: Ink::Secondary, align: Align::Right, lines: 1 });
    let mut ty = ty0 + 2.0 + hh;
    let details: Vec<String> = [s.location.clone(), if s.project.is_empty() { String::new() } else { context_capsule_core::capsules::project_name(projects, &s.project) }]
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
    if !details.is_empty() {
        let line = details.join(" · ");
        let dh = m.text(&line, Font::Small, inner, 1).1;
        out.push(Node::Text { r: Rect::new(left, ty + 2.0, inner, dh), text: line, font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
        ty += dh + 2.0;
    }
    if let Some(peek) = &s.peek {
        let ph = m.text(peek, Font::Small, inner, 2).1;
        out.push(Node::Text { r: Rect::new(left, ty + 4.0, inner, ph), text: peek.clone(), font: Font::Small, ink: Ink::Primary, align: Align::Left, lines: 2 });
        ty += ph + 4.0;
    }
    // "24 reads"  Show what was read
    let reads = s.reads_line();
    let (rw, rh) = m.text(&reads, Font::Small, inner, 1);
    let open = tv.reads_open.contains(&s.key());
    let label = if open { tl::HIDE_READS } else { tl::SHOW_READS };
    let lw = button_width(m, label, Style::Subtle);
    let lh = rh + 8.0;
    out.push(Node::Text { r: Rect::new(left, ty + 4.0 + (lh - rh) / 2.0, rw, rh), text: reads, font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
    out.push(Node::Button { r: Rect::new(left + rw + 8.0, ty + 4.0, lw.min((inner - rw - 8.0).max(0.0)), lh), action: Action::ShowReads(s.key()), label: label.into(), icon: None, style: Style::Subtle, enabled: true, check: false });
    ty += lh + 4.0;
    out.push(more_button(x, w, y + 6.0, Action::DeleteStretch(s.key())));
    if open {
        ty = reads_list(out, m, x, w, ty, s, tv);
    }
    ty.max(y + 44.0) + 10.0
}

/// Show what was read: each read's time, where and "Looked at", its first lines (Show all), ⋯ (Delete this
/// read…); at most `reads_limit`, then Show more.
fn reads_list(out: &mut Vec<Node>, m: &dyn Measure, x: f64, w: f64, top: f64, s: &Stretch, tv: &TimelineView) -> f64 {
    let more_w = 32.0;
    let left = x + CARD_PAD + 52.0;
    let inner = (x + w - CARD_PAD - more_w - 8.0 - left).max(60.0);
    let limit = tv.reads_limit.get(&s.key()).copied().unwrap_or(tl::PAGE);
    let reads = s.shown_reads();
    let mut y = top + 4.0;
    for mo in reads.iter().take(limit) {
        out.push(Node::Divider(Rect::new(left, y, inner, 1.0)));
        y = read_row(out, m, x, w, left, inner, y + 8.0, mo, tv);
    }
    if reads.len() > limit {
        let bw = button_width(m, tl::SHOW_MORE, Style::Standard);
        out.push(Node::Button { r: Rect::new(left, y + 4.0, bw.min(inner), BUTTON_H), action: Action::MoreReads(s.key()), label: tl::SHOW_MORE.into(), icon: None, style: Style::Standard, enabled: true, check: false });
        y += BUTTON_H + 8.0;
    }
    y
}

/// One read: "10:01 – 10:03 · linear.app/pricing" (and Looked at), the text folded to 3 lines with Show all, ⋯.
#[allow(clippy::too_many_arguments)]
fn read_row(out: &mut Vec<Node>, m: &dyn Measure, x: f64, w: f64, left: f64, inner: f64, y: f64, mo: &Moment, tv: &TimelineView) -> f64 {
    let line = if mo.location.is_empty() { mo.time() } else { format!("{} · {}", mo.time(), mo.location) };
    let pill = if mo.looked_at() { m.text(tl::LOOKED_AT, Font::Small, inner, 1).0 + 16.0 } else { 0.0 };
    let room = (inner - if pill > 0.0 { pill + 8.0 } else { 0.0 }).max(40.0);
    let (lw, lh) = m.text(&line, Font::Small, room, 1);
    let lw = lw.min(room);
    out.push(Node::Text { r: Rect::new(left, y, lw, lh), text: line, font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
    if pill > 0.0 {
        out.push(Node::Pill { r: Rect::new(left + lw + 8.0, y + (lh - 20.0) / 2.0, pill, 20.0), text: tl::LOOKED_AT.into(), live: false });
    }
    let mut ty = y + lh.max(if pill > 0.0 { 20.0 } else { 0.0 });
    let open = tv.expanded.contains(&mo.key());
    let text = mo.shown_text(open);
    if text.trim().is_empty() {
        let nh = m.text(tl::NOTHING_NEW, Font::Small, inner, 1).1;
        out.push(Node::Text { r: Rect::new(left, ty + 4.0, inner, nh), text: tl::NOTHING_NEW.into(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
        ty += nh + 4.0;
    } else {
        let lines = if open { 0 } else { tl::FOLD_LINES as u8 * 2 };
        let bh = m.text(&text, Font::Small, inner, lines).1;
        out.push(Node::Text { r: Rect::new(left, ty + 4.0, inner, bh), text, font: Font::Small, ink: Ink::Primary, align: Align::Left, lines });
        ty += bh + 4.0;
    }
    if open || mo.folds() {
        let label = if open { tl::SHOW_LESS } else { tl::SHOW_ALL };
        let bw = button_width(m, label, Style::Subtle);
        let bh = m.text(label, Font::Small, inner, 1).1 + 8.0;
        out.push(Node::Button { r: Rect::new(left - 4.0, ty + 2.0, bw, bh), action: Action::ShowAll(mo.key()), label: label.into(), icon: None, style: Style::Subtle, enabled: true, check: false });
        ty += bh + 2.0;
    }
    out.push(more_button(x, w, y - 6.0, Action::DeleteRead(mo.key())));
    ty.max(y + 26.0) + 8.0
}

/// ⋯ on a row's right (its delete).
fn more_button(x: f64, w: f64, y: f64, action: Action) -> Node {
    Node::Button { r: Rect::new(x + w - CARD_PAD - 32.0, y, 32.0, 32.0), action, label: String::new(), icon: Some(glyph::MORE), style: Style::Subtle, enabled: true, check: false }
}

// MARK: Projects

fn projects_page(col: &mut Col, v: &View) {
    col.para(w::PROJECTS_INTRO, Font::Body, Ink::Secondary);
    col.gap(16.0);
    if v.data.projects.is_empty() {
        col.para(w::NO_PROJECTS, Font::Body, Ink::Secondary);
        return;
    }
    let projects = v.data.projects.clone();
    let editing = v.editing.map(str::to_string);
    col.card(|out, x, w, top, m| {
        let mut y = top;
        for (i, p) in projects.iter().enumerate() {
            if i > 0 {
                divider(out, x, w, y);
            }
            if editing.as_deref() == Some(p.id.as_str()) {
                y = project_editor(out, m, x, w, y, p);
                continue;
            }
            let ew = button_width(m, w::EDIT, Style::Standard);
            let dw = button_width(m, w::DELETE, Style::Standard);
            let text_w = (w - 2.0 * CARD_PAD - ew - dw - 8.0 - 12.0).max(60.0);
            let found = w::found_by(p);
            let nh = m.text(&p.name, Font::Strong, text_w, 1).1;
            let fh = m.text(&found, Font::Small, text_w, 1).1;
            let h = (nh + fh + 2.0 + 24.0).max(ROW);
            let ty = y + (h - nh - fh - 2.0) / 2.0;
            out.push(Node::Text { r: Rect::new(x + CARD_PAD, ty, text_w, nh), text: p.name.clone(), font: Font::Strong, ink: Ink::Primary, align: Align::Left, lines: 1 });
            out.push(Node::Text { r: Rect::new(x + CARD_PAD, ty + nh + 2.0, text_w, fh), text: found, font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
            let by = y + (h - BUTTON_H) / 2.0;
            let dx = x + w - CARD_PAD - dw;
            out.push(Node::Button { r: Rect::new(dx, by, dw, BUTTON_H), action: Action::Delete(p.id.clone()), label: w::DELETE.into(), icon: None, style: Style::Standard, enabled: true, check: false });
            out.push(Node::Button { r: Rect::new(dx - 8.0 - ew, by, ew, BUTTON_H), action: Action::Edit(p.id.clone()), label: w::EDIT.into(), icon: None, style: Style::Standard, enabled: true, check: false });
            y += h;
        }
        y
    });
}

fn project_editor(out: &mut Vec<Node>, m: &dyn Measure, x: f64, w: f64, top: f64, p: &Project) -> f64 {
    let inner = w - 2.0 * CARD_PAD;
    let mut y = top + CARD_PAD;
    for (label, field) in [(w::NAME_LABEL, Field::Name), (w::CLUES_LABEL, Field::ProjectClues)] {
        let field = if field == Field::Name { Field::ProjectName } else { field };
        let lh = m.text(label, Font::Small, inner, 0).1;
        out.push(Node::Text { r: Rect::new(x + CARD_PAD, y, inner, lh), text: label.into(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 0 });
        y += lh + 4.0;
        out.push(Node::Field { r: Rect::new(x + CARD_PAD, y, inner, BUTTON_H), field });
        y += BUTTON_H + 12.0;
    }
    let sw = button_width(m, w::SAVE, Style::Accent);
    let cw = button_width(m, w::CANCEL, Style::Standard);
    let sx = x + w - CARD_PAD - sw;
    out.push(Node::Button { r: Rect::new(sx, y, sw, BUTTON_H), action: Action::SaveEdit(p.id.clone()), label: w::SAVE.into(), icon: None, style: Style::Accent, enabled: true, check: false });
    out.push(Node::Button { r: Rect::new(sx - 8.0 - cw, y, cw, BUTTON_H), action: Action::CancelEdit, label: w::CANCEL.into(), icon: None, style: Style::Standard, enabled: true, check: false });
    y + BUTTON_H + CARD_PAD
}

// MARK: Settings

/// Settings → Updates: "Update automatically", the version, and "Check for updates".
fn updates_section(col: &mut Col, v: &View) {
    use context_capsule_core::update::{AUTO_LABEL, CHECK_LABEL};
    let Some(auto) = v.auto_update else { return };
    col.gap(GAP);
    col.caption(w::UPDATES);
    let version = w::version_line(env!("CARGO_PKG_VERSION"));
    col.card(|out, x, w, top, m| {
        let (y, h) = row(out, m, x, w, top, None, AUTO_LABEL, None, TOGGLE_W);
        right_toggle(out, x, w, top, h, auto, Action::AutoUpdate);
        divider(out, x, w, y);
        let bw = button_width(m, CHECK_LABEL, Style::Standard);
        let (bottom, h) = row(out, m, x, w, y, None, &version, None, bw);
        right_button(out, m, x, w, y, h, CHECK_LABEL, Action::CheckUpdates, Style::Standard);
        bottom
    });
}

fn settings_page(col: &mut Col, v: &View) {
    let c = &v.data.config;

    col.caption(w::YOU);
    col.card(|out, x, w, top, m| {
        let fw = (w * 0.45).clamp(140.0, 260.0);
        let (bottom, h) = row(out, m, x, w, top, None, w::YOUR_NAME, None, fw);
        out.push(Node::Field { r: Rect::new(x + w - CARD_PAD - fw, top + (h - BUTTON_H) / 2.0, fw, BUTTON_H), field: Field::Name });
        bottom
    });
    col.gap(6.0);
    col.x += 4.0;
    col.para(w::NAME_HELP, Font::Small, Ink::Secondary);
    col.x -= 4.0;
    col.gap(GAP);

    col.caption(w::SET_UP);
    let done = v.setup;
    let ai = v.ai;
    col.card(|out, x, w, top, m| {
        let mut y = top;
        for (i, step) in Step::ALL.iter().enumerate() {
            if i > 0 {
                divider(out, x, w, y);
            }
            if *step == Step::Claude {
                y = connect_rows(out, m, x, w, y, done.claude || ai.any_connected(), ai);
                continue;
            }
            let r = step.row();
            let right = if done.get(*step) { 24.0 } else { button_width(m, r.button, Style::Standard) };
            let (bottom, h) = row(out, m, x, w, y, Some(step_icon(*step)), r.title, Some(r.help), right);
            if done.get(*step) {
                out.push(Node::Text { r: Rect::new(x + w - CARD_PAD - 24.0, y, 24.0, h), text: glyph::CHECK.into(), font: Font::Icon, ink: Ink::Secondary, align: Align::Center, lines: 1 });
            } else {
                right_button(out, m, x, w, y, h, r.button, Action::Setup(*step), Style::Standard);
            }
            y = bottom;
        }
        y
    });
    col.gap(GAP);

    // Your AI: what Make today's capsules opens.
    col.caption(anyai::YOUR_AI);
    let label = ai.choice_label();
    col.card(|out, x, w, top, m| {
        let bw = button_width(m, &label, Style::Dropdown).min((w * 0.5).max(120.0));
        let (bottom, h) = row(out, m, x, w, top, None, anyai::MAKE_WITH, None, bw);
        out.push(Node::Button {
            r: Rect::new(x + w - CARD_PAD - bw, top + (h - BUTTON_H) / 2.0, bw, BUTTON_H),
            action: Action::YourAi,
            label: label.clone(),
            icon: None,
            style: Style::Dropdown,
            enabled: true,
            check: false,
        });
        bottom
    });
    col.gap(6.0);
    col.x += 4.0;
    col.para(&anyai::settings_line(&ai.kind(), &ai.name()), Font::Small, Ink::Secondary);
    col.x -= 4.0;
    col.gap(GAP);

    col.caption(w::EVERY_DAY);
    let remind_at = c.remind_at.clone();
    let keep = c.keep_days;
    let sounds = c.sounds;
    let share = c.share_screens.label();
    col.card(|out, x, w, top, m| {
        let bw = button_width(m, share, Style::Dropdown);
        let (mut y, h) = row(out, m, x, w, top, None, w::WHEN_YOU_START, None, bw);
        right_button(out, m, x, w, top, h, share, Action::ShareScreens, Style::Dropdown);
        divider(out, x, w, y);
        let (bottom, h) = row(out, m, x, w, y, None, w::REMIND, None, TOGGLE_W);
        right_toggle(out, x, w, y, h, remind_at.is_some(), Action::Remind);
        y = bottom;
        if let Some(at) = &remind_at {
            divider(out, x, w, y);
            let label = settings::normalize_time(at);
            let bw = button_width(m, &label, Style::Dropdown);
            let (bottom, h) = row(out, m, x, w, y, None, w::REMIND_AT, None, bw);
            right_button(out, m, x, w, y, h, &label, Action::RemindAt, Style::Dropdown);
            y = bottom;
        }
        divider(out, x, w, y);
        let keep_label = settings::keep_options(keep).into_iter().find(|(d, _)| *d == keep).map(|(_, s)| s).unwrap_or_default();
        let bw = button_width(m, &keep_label, Style::Dropdown);
        let (bottom, h) = row(out, m, x, w, y, None, w::KEEP, None, bw);
        right_button(out, m, x, w, y, h, &keep_label, Action::Keep, Style::Dropdown);
        y = bottom;
        divider(out, x, w, y);
        let (bottom, h) = row(out, m, x, w, y, None, w::SOUNDS, None, TOGGLE_W);
        right_toggle(out, x, w, y, h, sounds, Action::Sounds);
        bottom
    });
    col.gap(6.0);
    col.x += 4.0;
    col.para(w::KEPT, Font::Small, Ink::Secondary);
    col.x -= 4.0;

    shortcuts_section(col, v);

    if v.voice {
        col.gap(GAP);
        col.caption(w::VOICE_NOTES);
        let options = settings::language_options(c.voice_locale.as_deref());
        let label = options[settings::language_index(c.voice_locale.as_deref())].1.clone();
        col.card(|out, x, w, top, m| {
            let bw = button_width(m, &label, Style::Dropdown);
            let (bottom, h) = row(out, m, x, w, top, None, w::LANGUAGE, None, bw);
            right_button(out, m, x, w, top, h, &label, Action::Language, Style::Dropdown);
            bottom
        });
    }

    col.gap(GAP);
    col.caption(w::APPEARANCE);
    let look = v.data.appearance.label();
    col.card(|out, x, w, top, m| {
        let bw = button_width(m, look, Style::Dropdown);
        let (bottom, h) = row(out, m, x, w, top, None, LIGHT_OR_DARK, None, bw);
        right_button(out, m, x, w, top, h, look, Action::Appearance, Style::Dropdown);
        bottom
    });

    col.gap(GAP);
    col.caption(w::NEVER_RECORD);
    let apps = c.exclude_apps.clone();
    col.card(|out, x, w, top, m| {
        let inner = w - 2.0 * CARD_PAD;
        let hh = m.text(w::NEVER_RECORD_HELP, Font::Small, inner, 0).1;
        out.push(Node::Text { r: Rect::new(x + CARD_PAD, top + 12.0, inner, hh), text: w::NEVER_RECORD_HELP.into(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 0 });
        let mut y = top + 12.0 + hh + 8.0;
        for app in &apps {
            divider(out, x, w, y);
            let bw = button_width(m, w::REMOVE, Style::Standard);
            let (bottom, h) = row(out, m, x, w, y, None, &app_label(app), None, bw);
            right_button(out, m, x, w, y, h, w::REMOVE, Action::RemoveApp(app.clone()), Style::Standard);
            y = bottom;
        }
        divider(out, x, w, y);
        let bw = button_width(m, w::CHOOSE, Style::Standard);
        let (bottom, h) = row(out, m, x, w, y, None, w::ADD_APP, None, bw);
        right_button(out, m, x, w, y, h, w::CHOOSE, Action::AddApp, Style::Standard);
        bottom
    });

    updates_section(col, v);
}

/// Settings → Set up → Connect your AI: the header row (✓ when an AI is connected), one row per installed app
/// (Connect, or ✓ and a ⋯ menu with Disconnect; a word while it works), the apps not installed under "More", a
/// note after connecting, and the line about any other AI. Returns the bottom.
fn connect_rows(out: &mut Vec<Node>, m: &dyn Measure, x: f64, w: f64, top: f64, done: bool, ai: &AiState) -> f64 {
    let r = Step::Claude.row();
    let (mut y, h) = row(out, m, x, w, top, Some(step_icon(Step::Claude)), r.title, Some(r.help), if done { 24.0 } else { 0.0 });
    if done {
        out.push(Node::Text { r: Rect::new(x + w - CARD_PAD - 24.0, top, 24.0, h), text: glyph::CHECK.into(), font: Font::Icon, ink: Ink::Secondary, align: Align::Center, lines: 1 });
    }
    let left = x + CARD_PAD + 36.0;
    let right = x + w - CARD_PAD;
    let sub_divider = |out: &mut Vec<Node>, y: f64| out.push(Node::Divider(Rect::new(left, y, (right - left).max(0.0), 1.0)));
    const SUB: f64 = 40.0;
    for c in ai.installed() {
        sub_divider(out, y);
        app_row(out, m, left, right, y, SUB, c, ai);
        y += SUB;
    }
    let missing = ai.missing();
    if !missing.is_empty() {
        sub_divider(out, y);
        out.push(Node::Button {
            r: Rect::new(left - 8.0, y + 2.0, (right - left + 8.0).max(40.0), 32.0),
            action: Action::AiMore,
            label: anyai::MORE.into(),
            icon: Some(if ai.more { glyph::CHEVRON } else { glyph::CHEVRON_RIGHT }),
            style: Style::Row { selected: false },
            enabled: true,
            check: false,
        });
        y += 36.0;
        if ai.more {
            let nw = m.text(anyai::NOT_INSTALLED, Font::Small, 400.0, 1).0;
            for c in missing {
                let lh = m.text(&c.name, Font::Small, (right - left - nw - 12.0).max(20.0), 1).1;
                out.push(Node::Text { r: Rect::new(left, y + (28.0 - lh) / 2.0, (right - left - nw - 12.0).max(20.0), lh), text: c.name.clone(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 1 });
                out.push(Node::Text { r: Rect::new(right - nw, y + (28.0 - lh) / 2.0, nw, lh), text: anyai::NOT_INSTALLED.into(), font: Font::Small, ink: Ink::Secondary, align: Align::Right, lines: 1 });
                y += 28.0;
            }
        }
    }
    let lw = (right - left).max(40.0);
    if let Some(note) = &ai.note {
        let nh = m.text(note, Font::Small, lw, 0).1;
        out.push(Node::Text { r: Rect::new(left, y + 6.0, lw, nh), text: note.clone(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 0 });
        y += nh + 6.0;
    }
    let ah = m.text(anyai::ANY_OTHER_LINE, Font::Small, lw, 0).1;
    out.push(Node::Text { r: Rect::new(left, y + 6.0, lw, ah), text: anyai::ANY_OTHER_LINE.into(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 0 });
    y + 6.0 + ah + 12.0
}

/// One AI app under Connect your AI.
#[allow(clippy::too_many_arguments)]
fn app_row(out: &mut Vec<Node>, m: &dyn Measure, left: f64, right: f64, y: f64, h: f64, c: &AiClient, ai: &AiState) {
    let by = y + (h - BUTTON_H) / 2.0;
    let used = if ai.busy.as_deref() == Some(c.id.as_str()) {
        let word = if c.connected { ai::DISCONNECTING } else { ai::CONNECTING };
        let ww = m.text(word, Font::Small, 300.0, 1).0;
        let wh = m.text(word, Font::Small, ww, 1).1;
        out.push(Node::Text { r: Rect::new(right - ww, y + (h - wh) / 2.0, ww, wh), text: word.into(), font: Font::Small, ink: Ink::Secondary, align: Align::Right, lines: 1 });
        ww
    } else if c.connected {
        out.push(Node::Button { r: Rect::new(right - 36.0, by, 36.0, BUTTON_H), action: Action::AiMenu(c.id.clone()), label: String::new(), icon: Some(glyph::MORE), style: Style::Subtle, enabled: true, check: false });
        out.push(Node::Text { r: Rect::new(right - 36.0 - 4.0 - 24.0, y, 24.0, h), text: glyph::CHECK.into(), font: Font::IconSmall, ink: Ink::Secondary, align: Align::Center, lines: 1 });
        36.0 + 4.0 + 24.0
    } else {
        let bw = button_width(m, anyai::CONNECT, Style::Standard);
        out.push(Node::Button { r: Rect::new(right - bw, by, bw, BUTTON_H), action: Action::AiConnect(c.id.clone()), label: anyai::CONNECT.into(), icon: None, style: Style::Standard, enabled: ai.busy.is_none(), check: false });
        bw
    };
    let tw = (right - left - used - 12.0).max(20.0);
    let th = m.text(&c.name, Font::Body, tw, 1).1;
    out.push(Node::Text { r: Rect::new(left, y + (h - th) / 2.0, tw, th), text: c.name.clone(), font: Font::Body, ink: Ink::Primary, align: Align::Left, lines: 1 });
}

/// Settings → Shortcuts: "Start or stop" and "Speak a note", each a button showing its keys (click, then press
/// the new keys; Esc cancels), a line under a row when keys were refused, and Reset.
fn shortcuts_section(col: &mut Col, v: &View) {
    col.gap(GAP);
    col.caption(keys::SHORTCUTS);
    let current = [(Which::Record, v.keys.record.clone()), (Which::Voice, v.keys.voice.clone())];
    let (recording, note) = (v.recording_keys, v.keys_note);
    col.card(|out, x, w, top, m| {
        let mut y = top;
        for (which, label) in &current {
            let waiting = recording == Some(*which);
            let shown = if waiting { keys::PRESS_KEYS.to_string() } else { label.clone() };
            let help = if waiting { Some(keys::PRESS_HELP) } else { note.filter(|(n, _)| n == which).map(|(_, t)| t) };
            let bw = button_width(m, &shown, Style::Standard).max(120.0);
            let (bottom, h) = row(out, m, x, w, y, None, which.title(), help, bw);
            out.push(Node::Button {
                r: Rect::new(x + w - CARD_PAD - bw, y + (h - BUTTON_H) / 2.0, bw, BUTTON_H),
                action: Action::Shortcut(*which),
                label: shown,
                icon: None,
                style: if waiting { Style::Accent } else { Style::Standard },
                enabled: true,
                check: false,
            });
            divider(out, x, w, bottom);
            y = bottom;
        }
        let bw = button_width(m, keys::RESET, Style::Standard);
        let (bottom, h) = row(out, m, x, w, y, None, keys::SHORTCUTS_HELP, None, bw);
        right_button(out, m, x, w, y, h, keys::RESET, Action::ResetShortcuts, Style::Standard);
        bottom
    });
}

/// The row label next to System / Light / Dark.
pub const LIGHT_OR_DARK: &str = "Light or dark";

/// "Slack.exe" → "Slack"
pub fn app_label(app: &str) -> String {
    let t = app.trim();
    match t.len().checked_sub(4) {
        Some(cut) if t.is_char_boundary(cut) && t[cut..].eq_ignore_ascii_case(".exe") => t[..cut].to_string(),
        _ => t.to_string(),
    }
}

// MARK: Help

fn help_page(col: &mut Col, v: &View) {
    col.caption(w::SHORTCUTS);
    let lines = w::shortcuts(v.keys, &v.ai.kind());
    col.card(|out, x, w, top, m| {
        let mut y = top;
        for (i, (what, keys)) in lines.iter().enumerate() {
            if i > 0 {
                divider(out, x, w, y);
            }
            let kw: f64 = if keys.contains('+') { keys.split('+').map(|k| (m.text(k, Font::SmallStrong, 200.0, 1).0 + 14.0).max(26.0) + 4.0).sum::<f64>() } else { m.text(keys, Font::Body, 200.0, 1).0 };
            let (bottom, h) = row(out, m, x, w, y, None, what, None, kw);
            if keys.contains('+') {
                keycaps(out, m, x + w - CARD_PAD - kw + 4.0, y + (h - 24.0) / 2.0, keys);
            } else {
                let th = m.text(keys, Font::Body, kw, 1).1;
                out.push(Node::Text { r: Rect::new(x + w - CARD_PAD - kw, y + (h - th) / 2.0, kw, th), text: keys.clone(), font: Font::Body, ink: Ink::Secondary, align: Align::Right, lines: 1 });
            }
            y = bottom;
        }
        y
    });
    col.gap(GAP);
    for (title, paras) in [(w::HOW_IT_WORKS, w::HOW.map(String::from).to_vec()), (w::PRIVACY, w::privacy(v.keys).to_vec())] {
        col.caption(title);
        col.card(|out, x, w, top, m| {
            let inner = w - 2.0 * CARD_PAD;
            let mut y = top + 14.0;
            for p in &paras {
                let h = m.text(p, Font::Body, inner, 0).1;
                out.push(Node::Text { r: Rect::new(x + CARD_PAD, y, inner, h), text: p.clone(), font: Font::Body, ink: Ink::Primary, align: Align::Left, lines: 0 });
                y += h + 8.0;
            }
            y - 8.0 + 14.0
        });
        col.gap(GAP);
    }
    col.y -= GAP;
}

/// A Windows long-date pattern without its year: "dddd, d MMMM yyyy" → "dddd, d MMMM",
/// "dddd, MMMM d, yyyy" → "dddd, MMMM d", "yyyy年M月d日" → "M月d日".
pub fn without_year(pattern: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == 'y' {
            while chars.peek() == Some(&'y') {
                chars.next();
            }
            if matches!(chars.peek(), Some('年') | Some('년')) {
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    out.trim_matches(|c: char| c == ' ' || c == ',' || c == '.' || c == '/' || c == '-').replace(" ,", ",").replace("  ", " ")
}

/// The controls in focus order (Tab), as indexes into `nodes`.
#[cfg(test)]
pub fn controls(nodes: &[Node]) -> Vec<usize> {
    nodes.iter().enumerate().filter(|(_, n)| n.is_control()).map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_flag() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(show_arg(&a(&["cc.exe"])), None);
        assert_eq!(show_arg(&a(&["cc.exe", "--show", "nowhere"])), None);
        assert_eq!(show_arg(&a(&["cc.exe", "--show", "help"])), Some(ShowArg { section: Section::Help, date: None, open: None }));
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 19);
        assert_eq!(
            show_arg(&a(&["cc.exe", "--show", "Timeline", "2026-10-19", "20261019-100103"])),
            Some(ShowArg { section: Section::Timeline, date: day, open: Some("20261019-100103".into()) })
        );
        assert_eq!(show_arg(&a(&["cc.exe", "--show", "timeline", "--login", "2026-10-19"])).unwrap().date, None);
    }
    use context_capsule_core::window::Keys;
    use context_capsule_core::Totals;

    /// 7 DIPs a character, 18 a line (wrapping by width).
    struct Fake;
    impl Measure for Fake {
        fn text(&self, text: &str, font: Font, width: f64, lines: u8) -> (f64, f64) {
            let (size, _, _) = font.spec();
            let cw = size * 0.5;
            let line = size * 1.33;
            let natural = text.chars().count() as f64 * cw;
            if lines == 1 || natural <= width {
                return (natural.min(width), line);
            }
            let mut n = (natural / width.max(1.0)).ceil();
            if lines > 0 {
                n = n.min(lines as f64);
            }
            (width, n * line)
        }
    }

    fn data() -> Data {
        let projects = vec![
            Project { id: "web".into(), name: "Website".into(), clues: vec!["acme.com".into()] },
            Project { id: "app".into(), name: "Mobile app".into(), clues: vec![] },
        ];
        let cap = |p: &str, kind: &str, date: &str| CapsuleSummary {
            path: PathBuf::from(format!("C:/c/{p}-{date}.capsule.md")),
            title: format!("{p} {date}"),
            project: p.into(),
            kind: kind.into(),
            date: date.into(),
            created: format!("{date}T18:00:00Z"),
            in_short: "We redesigned the pricing page and moved Teams to $12.".into(),
            ..Default::default()
        };
        let capsules = vec![cap("web", "daily", "2026-10-20"), cap("web", "project", "2026-10-19"), cap("app", "daily", "2026-10-18")];
        let today = Totals { seconds: 2403.0, by_project: vec![("web".into(), 1803.0), ("".into(), 600.0)] };
        Data {
            home: HomeNumbers { first_name: "Saachi".into(), today: today.clone(), week: today, week_capsules: 2, streak: 3, recent: capsules.clone() },
            capsules,
            projects,
            config: Config { exclude_apps: vec!["Slack.exe".into()], ..Default::default() },
            appearance: Appearance::System,
        }
    }

    fn view<'a>(section: Section, status: &'a Status, data: &'a Data) -> View<'a> {
        View {
            section,
            status,
            data,
            now: SystemTime::now(),
            hour: 20,
            date: "Tuesday, 20 October",
            today: chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(),
            editing: None,
            setup: Done { voice: true, claude: false, login: true },
            several_screens: true,
            voice: true,
            auto_update: Some(true),
            keys: Box::leak(Box::new(Keys::windows(&Default::default()))),
            search: "",
            share: None,
            recording_keys: None,
            keys_note: None,
            ai: Box::leak(Box::new(AiState::default())),
            timeline: Box::leak(Box::new(timeline_view(sample_day()))),
        }
    }

    fn timeline_view(day: Day) -> TimelineView {
        let mut tv = TimelineView::new(day.date);
        tv.set(TimelineData { date: day.date, day, recorded: vec![chrono::NaiveDate::from_ymd_opt(2026, 10, 19).unwrap()] });
        tv
    }

    /// A day on the Timeline (UTC), read by core from files: a recording with a stretch on Website (two reads, one
    /// looked at), a gap, a voice note and a stretch in Code; then one still going, in Slack.
    fn sample_day() -> Day {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!("cc-windows-timeline-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let write = |id: &str, file: &str, lines: &[&str]| {
            std::fs::create_dir_all(root.join(id)).unwrap();
            std::fs::write(root.join(id).join(file), lines.iter().map(|l| format!("{l}\n")).collect::<String>()).unwrap();
        };
        let (a, b) = ("20261020-093000", "20261020-140000");
        write(a, "events.jsonl", &[
            r#"{"t":"2026-10-20T09:30:00Z","end":"2026-10-20T09:30:03Z","app":"Google Chrome","title":"Pricing — Linear","where":"linear.app/pricing","project":"web","text":"Pricing\nFree\nTeams $12\nEnterprise plans for larger teams"}"#,
            r#"{"t":"2026-10-20T09:30:03Z","end":"2026-10-20T09:40:00Z","app":"Google Chrome","title":"Pricing — Linear","project":"web","focus":"pointer","text":"Annual −20%"}"#,
            r#"{"t":"2026-10-20T10:00:00Z","end":"2026-10-20T10:00:05Z","kind":"voice","app":"Google Chrome","text":"We keep annual at twenty percent off."}"#,
            r#"{"t":"2026-10-20T10:02:00Z","end":"2026-10-20T10:10:00Z","app":"Code","title":"main.rs","text":""}"#,
        ]);
        write(a, "gaps.jsonl", &[r#"{"t":"2026-10-20T09:45:00Z","end":"2026-10-20T09:55:00Z","why":"private_app"}"#]);
        write(b, "events.jsonl", &[r##"{"t":"2026-10-20T14:00:00Z","end":"2026-10-20T14:02:00Z","app":"Slack","title":"#general","text":"hello"}"##]);
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        let day = tl::load_day_in(&root, Some(b), chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap(), &tz, &[]);
        let _ = std::fs::remove_dir_all(&root);
        day
    }

    fn actions(l: &Layout) -> Vec<Action> {
        l.content.iter().filter_map(|n| n.action().cloned()).collect()
    }

    #[test]
    fn timeline_page() {
        let (status, data) = (Status::default(), data());
        let mut v = view(Section::Timeline, &status, &data);
        let l = layout(&v, 1100.0, 800.0, &Fake);
        let t = texts(&l);
        let (a, b) = ("20261020-093000".to_string(), "20261020-140000".to_string());
        // The header, the day bar, where the time went, the recordings: the live one open, the other folded.
        for word in ["Tuesday 20 October", "20 min recorded · 2 recordings", "09:00", "15:00", "Where your time went", "Website", "Not sorted",
            "Recordings", "09:30 – 10:10 · 40 min · Website", "14:00 – 14:02 · 2 min · Slack", "Recording now", "Slack · #general", "14:00 – 14:02 · 2 min",
            "1 read", "Show what was read"] {
            assert!(t.iter().any(|x| x == word), "{word}: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Today"), "no Today button on today");
        assert!(!t.iter().any(|x| x == "Google Chrome · Pricing — Linear"), "a folded recording shows one line");
        assert_eq!(t.iter().filter(|x| *x == "Make a capsule").count(), 2);
        assert!(l.content.iter().any(|n| matches!(n, Node::Field { field: Field::TimelineSearch, .. })));
        let acts = actions(&l);
        assert!(acts.contains(&Action::DayBefore) && acts.contains(&Action::Calendar) && !acts.contains(&Action::DayAfter), "› off on today");
        let segments = acts.iter().filter(|a| matches!(a, Action::OpenStretch(_))).count();
        assert_eq!(segments, 3, "a click area per stretch on the day bar");
        assert_eq!(acts.iter().filter(|a| matches!(a, Action::DeleteBlock(_))).count(), 2);
        let colours: Vec<Colour> = l.content.iter().filter_map(|n| if let Node::Swatch { colour: Some(c), .. } = n { Some(*c) } else { None }).collect();
        assert!(colours.contains(&Colour::Blue) && colours.contains(&Colour::Grey) && colours.iter().all(|c| matches!(c, Colour::Blue | Colour::Grey)));

        // Open the other recording: its stretches (where · project, the peek, the reads), the gap, the voice note.
        let mut tv = timeline_view(sample_day());
        tv.toggle(&a);
        v.timeline = Box::leak(Box::new(tv.clone()));
        let l = layout(&v, 1100.0, 800.0, &Fake);
        let t = texts(&l);
        for word in ["Google Chrome · Pricing — Linear", "09:30 – 09:40 · 10 min", "linear.app/pricing · Website", "Enterprise plans for larger teams", "2 reads",
            "Not read 09:45 – 09:55: a private app was in front", "We keep annual at twenty percent off.", "You said · 10:00 · in Google Chrome", "Code · main.rs"] {
            assert!(t.iter().any(|x| x == word), "{word}: {t:?}");
        }
        assert!(l.content.iter().any(|n| matches!(n, Node::Bubble(_))));
        let chrome = (a.clone(), 0);
        assert!(actions(&l).contains(&Action::DeleteStretch(chrome.clone())) && actions(&l).contains(&Action::DeleteRead((a.clone(), 2))), "a stretch, a note");
        assert!(!actions(&l).contains(&Action::DeleteRead(chrome.clone())), "reads only after Show what was read");

        // Show what was read: each read's time and where, Looked at, its lines (Show all).
        tv.reads_open.insert(chrome.clone());
        tv.expanded.insert(chrome.clone());
        v.timeline = Box::leak(Box::new(tv.clone()));
        let l = layout(&v, 1100.0, 800.0, &Fake);
        let t = texts(&l);
        for word in ["Hide what was read", "09:30 · linear.app/pricing", "09:30 – 09:40", "Looked at", "Show less", "Pricing\nFree\nTeams $12\nEnterprise plans for larger teams"] {
            assert!(t.iter().any(|x| x == word), "{word}: {t:?}");
        }
        assert!(actions(&l).contains(&Action::DeleteRead((a.clone(), 1))));
        tv.reads_limit.insert(chrome.clone(), 1);
        v.timeline = Box::leak(Box::new(tv.clone()));
        assert!(find(&layout(&v, 1100.0, 800.0, &Fake), "Show more").is_some());

        // Search: every recording listed, those with a match open; "1 of 2 reads match"; the header stays the day's.
        let mut tv = timeline_view(sample_day());
        tv.set_search("ANNUAL");
        v.timeline = Box::leak(Box::new(tv.clone()));
        let t = texts(&layout(&v, 1100.0, 800.0, &Fake));
        for word in ["1 of 2 reads match", "We keep annual at twenty percent off.", "20 min recorded · 2 recordings", "14:00 – 14:02 · 2 min · Slack"] {
            assert!(t.iter().any(|x| x == word), "{word}: {t:?}");
        }
        assert!(!t.iter().any(|x| x.starts_with("Not read") || x == "Code · main.rs" || x == "Slack · #general"), "{t:?}");
        tv.set_search("nothing like it");
        v.timeline = Box::leak(Box::new(tv.clone()));
        assert!(texts(&layout(&v, 1100.0, 800.0, &Fake)).iter().any(|x| x == "Nothing on this day matches."));

        // The day bar opens a recording.
        let mut tv = timeline_view(sample_day());
        tv.open_stretch(&chrome);
        assert!(tv.is_open(tv.shown().unwrap().block(&a).unwrap()));
        tv.open_stretch(&(b.clone(), 0));
        assert!(tv.is_open(tv.shown().unwrap().block(&b).unwrap()), "already open stays open");

        // Reading, an empty day, another day (Today and ›), the calendar.
        v.timeline = Box::leak(Box::new(TimelineView::new(sample_day().date)));
        assert!(texts(&layout(&v, 1100.0, 800.0, &Fake)).iter().any(|x| x == "Reading…"));
        let empty = Day { date: sample_day().date, blocks: vec![] };
        v.timeline = Box::leak(Box::new(timeline_view(empty)));
        let t = texts(&layout(&v, 1100.0, 800.0, &Fake));
        assert!(t.iter().any(|x| x == "Nothing recorded today yet. Start contextualizing (Alt+C) when you begin work.") && t.iter().any(|x| x == "nothing recorded"));
        let mut tv = timeline_view(Day { date: chrono::NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(), blocks: vec![] });
        tv.calendar = Some((2026, 10));
        v.timeline = Box::leak(Box::new(tv.clone()));
        let l = layout(&v, 1100.0, 800.0, &Fake);
        let t = texts(&l);
        assert!(t.iter().any(|x| x == "Monday 19 October") && t.iter().any(|x| x == "Today") && t.iter().any(|x| x == "October 2026") && t.iter().any(|x| x == "Mo"));
        assert!(actions(&l).contains(&Action::DayAfter) && actions(&l).contains(&Action::DayToday));
        let days: Vec<&Node> = l.content.iter().filter(|n| matches!(n, Node::Button { action: Action::PickDay(_), .. })).collect();
        assert_eq!(days.len(), 31);
        assert!(days.iter().any(|n| matches!(n, Node::Button { label, style: Style::Cell { selected: true }, .. } if label == "19")));
        assert!(days.iter().any(|n| matches!(n, Node::Button { label, enabled: false, .. } if label == "21")), "no future days");
        assert!(!actions(&l).contains(&Action::CalendarMonth(1)), "not past this month");
        assert_eq!(l.content.iter().filter(|n| matches!(n, Node::Swatch { colour: Some(Colour::Grey), r } if r.w == 4.0)).count(), 1, "a dot on the 19th");
        tv.calendar_month(-1, v.today);
        assert_eq!(tv.calendar, Some((2026, 9)));
        tv.calendar_month(1, v.today);
        tv.calendar_month(1, v.today);
        assert_eq!(tv.calendar, Some((2026, 10)));
        // Another day picked: nothing until it's read, nothing left open.
        tv.toggle("x");
        tv.go_to(chrono::NaiveDate::from_ymd_opt(2026, 10, 18).unwrap());
        assert!(tv.shown().is_none() && tv.toggled.is_empty() && tv.calendar.is_none());
    }

    /// Claude (connected), Cursor (not), Zed (not installed); Your AI = `choice`.
    fn ai_state(choice: &str) -> AiState {
        let client = |id: &str, name: &str, installed: bool, connected: bool| AiClient { id: id.into(), name: name.into(), installed, connected, how: "config".into() };
        let mut s = AiState::default();
        s.set_clients(vec![AiClient::claude(true, true), client("cursor", "Cursor", true, false), client("zed", "Zed", false, false)], Some(choice));
        s
    }

    #[test]
    fn capsules_search_ask_and_open() {
        let (status, data) = (Status::default(), data());
        let mut v = view(Section::Capsules, &status, &data);
        let l = layout(&v, 1000.0, 700.0, &Fake);
        let ask = |l: &Layout| l.content.iter().find_map(|n| if let Node::Button { action: Action::Ask, enabled, .. } = n { Some(*enabled) } else { None });
        assert_eq!(ask(&l), Some(false), "Ask: off while the box is empty");
        assert!(find(&l, "Ask your AI").is_some() && find(&l, "Using an AI on the web, or any other AI?").is_some());
        assert!(find(&l, "Copy my day").is_some() && find(&l, "Save from clipboard").is_some());
        assert!(find(&l, "Open a capsule…").is_some());
        assert!(l.content.iter().any(|n| matches!(n, Node::Field { field: Field::Search, .. })));
        assert_eq!(texts(&l).iter().filter(|x| *x == "Open").count(), 3);
        v.search = "APP 10-18";
        let l = layout(&v, 1000.0, 700.0, &Fake);
        assert_eq!(ask(&l), Some(true));
        let t = texts(&l);
        assert_eq!(t.iter().filter(|x| *x == "Open").count(), 1, "only the matching capsule: {t:?}");
        assert!(t.iter().any(|x| x == "Mobile app") && !t.iter().any(|x| x == "Website"), "by title, any case");
        v.search = "nothing like this";
        assert!(texts(&layout(&v, 1000.0, 700.0, &Fake)).iter().any(|x| x == w::NO_MATCHES));
    }

    #[test]
    fn check_before_sharing() {
        let (status, data) = (Status::default(), data());
        let text = "---\ncapsule: 1\ntitle: T\n---\n\n## In short\nMet Ana at ana@acme.com about $12,000.\n\n## People, links and files\n- Ana Ruiz — client\n";
        let mut share = ShareView::new(PathBuf::from("C:/c/t.capsule.md"), ShareHow::CopyText, text.into()).unwrap();
        assert!(ShareView::new(PathBuf::from("x"), ShareHow::CopyFile, "Nothing private.".into()).is_none(), "nothing found: straight through");
        let mut v = view(Section::Capsules, &status, &data);
        v.share = Some(&share);
        let l = layout(&v, 1000.0, 700.0, &Fake);
        let t = texts(&l);
        for word in ["Check before sharing", "People", "Emails", "Money", "Ana Ruiz", "ana@acme.com", "$12,000", "Share", "Cancel"] {
            assert!(t.iter().any(|x| x == word), "{word}: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Make today's capsules"), "instead of the page");
        let checks: Vec<bool> = l.content.iter().filter_map(|n| if let Node::Button { style: Style::Check { on }, .. } = n { Some(*on) } else { None }).collect();
        assert_eq!(checks, [true, true, true], "all checked");
        assert_eq!(share.shared_text(), "---\ncapsule: 1\ntitle: T\n---\n\n## In short\nMet Ana at [removed] about [removed].\n\n## People, links and files\n- [removed] — client\n");
        share.checked[2] = false;
        assert!(share.shared_text().contains("$12,000") && !share.shared_text().contains("ana@"));
        let rects: Vec<Rect> = controls(&l.content).iter().map(|i| l.content[*i].rect()).collect();
        for (i, a) in rects.iter().enumerate() {
            assert!(rects[i + 1..].iter().all(|b| !a.overlaps(b)), "controls don't overlap");
        }
    }

    fn texts(l: &Layout) -> Vec<String> {
        l.sidebar.iter().chain(&l.header).chain(&l.content).filter_map(|n| match n {
            Node::Text { text, .. } | Node::Pill { text, .. } | Node::Key { text, .. } => Some(text.clone()),
            Node::Button { label, .. } => Some(label.clone()),
            _ => None,
        }).collect()
    }

    fn find<'a>(l: &'a Layout, label: &str) -> Option<&'a Node> {
        l.content.iter().chain(&l.header).chain(&l.sidebar).find(|n| matches!(n, Node::Text { text, .. } if text == label) || matches!(n, Node::Button { label: b, .. } if b == label))
    }

    #[test]
    fn home_has_everything() {
        let (status, data) = (Status::default(), data());
        let l = layout(&view(Section::Home, &status, &data), 1200.0, 800.0, &Fake);
        let t = texts(&l);
        for word in ["Your work", "Context Capsule", "Home", "Good evening, Saachi.", "Tuesday, 20 October", "Today 40 m", "Not contextualizing",
            "Start contextualizing", "Website", "Not sorted yet", "Recent capsules", "See all", "Shortcuts", "Working on", "Find by itself",
            "New project", "Make today's capsules", "Copy my day", "Save from clipboard", "Your AI: any AI", "Change", "Streak", "3 days", "in a row", "2 capsules", "contextualized", "See today's timeline"] {
            assert!(t.iter().any(|x| x == word), "{word} on Home: {t:?}");
        }
        assert_eq!(t.iter().filter(|x| *x == "Open").count(), 3, "three recent capsules");
        assert!(!l.content.iter().any(|n| n.action() == Some(&Action::Voice)), "Speak a note: only while contextualizing");
    }

    #[test]
    fn wide_has_a_side_column_narrow_stacks() {
        let (status, data) = (Status::default(), data());
        let wide = layout(&view(Section::Home, &status, &data), 1300.0, 800.0, &Fake);
        let narrow = layout(&view(Section::Home, &status, &data), 700.0, 800.0, &Fake);
        let (ws, wt) = (find(&wide, "Shortcuts").unwrap().rect(), find(&wide, "Not contextualizing").unwrap().rect());
        assert!(ws.x > wt.x + 200.0 && ws.y < wt.y + 100.0, "wide: beside the status card");
        let (ns, nr) = (find(&narrow, "Shortcuts").unwrap().rect(), find(&narrow, "Recent capsules").unwrap().rect());
        assert!(ns.y > nr.y && (ns.x - nr.x).abs() < 1.0, "narrow: below, same left edge");
        assert!(narrow.height > wide.height, "stacked is taller");
    }

    #[test]
    fn content_is_centered_and_capped() {
        let (status, data) = (Status::default(), data());
        let l = layout(&view(Section::Help, &status, &data), 2400.0, 900.0, &Fake);
        let cards: Vec<Rect> = l.content.iter().filter_map(|n| if let Node::Card(r) = n { Some(*r) } else { None }).collect();
        assert!(!cards.is_empty());
        for c in &cards {
            assert!(c.w <= w::MAX_CONTENT_WIDE + 0.1, "capped");
            let mid = c.x + c.w / 2.0;
            assert!((mid - (SIDEBAR + (2400.0 - SIDEBAR) / 2.0)).abs() < 1.0, "centered");
        }
    }

    #[test]
    fn nothing_spills_out_of_its_card() {
        let mut status = Status { recording: true, started_at: Some(SystemTime::now()), sources: 2, events: 4, ..Default::default() };
        let data = data();
        status.projects = data.projects.clone();
        for section in Section::ALL {
            for width in [640.0, 900.0, 1500.0] {
                let mut v = view(section, &status, &data);
                v.editing = Some("app");
                let l = layout(&v, width, 700.0, &Fake);
                let cards: Vec<Rect> = l.content.iter().filter_map(|n| if let Node::Card(r) = n { Some(*r) } else { None }).collect();
                for n in &l.content {
                    if matches!(n, Node::Card(_)) {
                        continue;
                    }
                    let r = n.rect();
                    assert!(r.w >= 0.0 && r.h >= 0.0, "{section:?} {width}: {n:?}");
                    assert!(r.x >= SIDEBAR && r.right() <= width + 0.5, "{section:?} {width}: inside the window: {n:?}");
                    if let Some(card) = cards.iter().find(|c| c.contains(r.x + 0.1, r.y + 0.1)) {
                        assert!(r.right() <= card.right() + 0.5 && r.bottom() <= card.bottom() + 0.5, "{section:?} {width}: in its card: {n:?}");
                    }
                }
                assert!(l.height > 0.0);
            }
        }
    }

    #[test]
    fn controls_do_not_overlap() {
        let status = Status { recording: true, started_at: Some(SystemTime::now()), sources: 1, ..Default::default() };
        let data = data();
        for section in Section::ALL {
            for width in [640.0, 1000.0, 1600.0] {
                let l = layout(&view(section, &status, &data), width, 700.0, &Fake);
                let rects: Vec<Rect> = controls(&l.content).iter().map(|i| l.content[*i].rect()).collect();
                for (i, a) in rects.iter().enumerate() {
                    for b in &rects[i + 1..] {
                        assert!(!a.overlaps(b), "{section:?} at {width}: {a:?} and {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn recording_card_and_buttons() {
        let data = data();
        let status = Status { recording: true, started_at: Some(SystemTime::now()), sources: 1, events: 1, ..Default::default() };
        let l = layout(&view(Section::Home, &status, &data), 1000.0, 700.0, &Fake);
        let t = texts(&l);
        assert!(t.iter().any(|x| x.starts_with("Contextualizing  0:0")), "{t:?}");
        assert!(t.iter().any(|x| x == "1 moment saved") && t.iter().any(|x| x == "Stop") && t.iter().any(|x| x == "Speak a note"));
        assert!(t.iter().any(|x| x.starts_with("● 0:0")), "the pill shows the time");
        let add = l.content.iter().find(|n| matches!(n, Node::Button { action: Action::AddScreen, .. })).unwrap();
        assert!(matches!(add, Node::Button { enabled: true, .. }));
    }

    #[test]
    fn other_pages() {
        let (status, data) = (Status::default(), data());
        let t = texts(&layout(&view(Section::Capsules, &status, &data), 1000.0, 700.0, &Fake));
        for word in ["Make today's capsules", "Mobile app", "Website", "Whole project", "Today", "Open"] {
            assert!(t.iter().any(|x| x == word), "{word} on Capsules: {t:?}");
        }
        let mut v = view(Section::Projects, &status, &data);
        let t = texts(&layout(&v, 1000.0, 700.0, &Fake));
        assert!(t.iter().any(|x| x == "Found by: acme.com") && t.iter().any(|x| x == "Not found automatically yet"));
        v.editing = Some("web");
        let l = layout(&v, 1000.0, 700.0, &Fake);
        let fields: Vec<Field> = l.content.iter().filter_map(|n| if let Node::Field { field, .. } = n { Some(*field) } else { None }).collect();
        assert_eq!(fields, [Field::ProjectName, Field::ProjectClues]);
        let t = texts(&layout(&view(Section::Settings, &status, &data), 1000.0, 700.0, &Fake));
        for word in ["You", "Your name", "Set up", "Voice notes", "Connect your AI", "Any other AI (ChatGPT website, Gemini, Copilot…): use Copy my day.", "Your AI", "Make today's capsules with", "Any other AI", "Open at login", "Every day", "When you start", "Pick a screen or window", "Remind me to make capsules",
            "18:00", "30 days", "Sounds", "Capsules are never deleted.", "Language", "Same as my computer", "Appearance", "System", "Never record", "Slack", "Remove", "Choose…", "Updates", "Update automatically", "Check for updates"] {
            assert!(t.iter().any(|x| x == word), "{word} in Settings: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Allow" || x == "Turn On" || x == "Connect"), "done rows show a check instead; no apps listed yet");
        for word in ["Shortcuts", "Start or stop", "Speak a note", "Alt+C", "Alt+V", "Reset", "Click a shortcut, then press the new keys."] {
            assert!(t.iter().any(|x| x == word), "{word} in Settings → Shortcuts: {t:?}");
        }
        // Waiting for new keys, a refusal under the other row, and your own shortcut shown everywhere.
        let mine = context_capsule_core::Hotkeys::DEFAULT.with(Which::Record, context_capsule_core::Shortcut::parse("Ctrl+Shift+R").unwrap());
        let keys = Keys::windows(&mine);
        let mut v = view(Section::Settings, &status, &data);
        v.keys = &keys;
        v.recording_keys = Some(Which::Voice);
        v.keys_note = Some((Which::Record, "Another app uses these keys"));
        let l = layout(&v, 1000.0, 700.0, &Fake);
        let t = texts(&l);
        for word in ["Ctrl+Shift+R", "Press keys…", "Press the new keys. Esc cancels.", "Another app uses these keys"] {
            assert!(t.iter().any(|x| x == word), "{word}: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Alt+V"), "the one waiting shows Press keys…");
        let buttons: Vec<&Action> = l.content.iter().filter_map(|n| n.action()).filter(|a| matches!(a, Action::Shortcut(_) | Action::ResetShortcuts)).collect();
        assert_eq!(buttons, [&Action::Shortcut(Which::Record), &Action::Shortcut(Which::Voice), &Action::ResetShortcuts]);
        v.section = Section::Help;
        assert!(texts(&layout(&v, 1000.0, 700.0, &Fake)).iter().any(|x| x == "Shift"), "Help shows the current keys");
        v.section = Section::Home;
        let home = texts(&layout(&v, 1000.0, 700.0, &Fake));
        assert!(home.iter().any(|x| x == "R") && home.iter().any(|x| x.starts_with("Ctrl+Shift+R to start")), "{home:?}");
        let t = texts(&layout(&view(Section::Help, &status, &data), 1000.0, 700.0, &Fake));
        for word in ["Start or stop contextualizing", "Alt", "C", "V", "Copy my day", "How it works", "Privacy"] {
            assert!(t.iter().any(|x| x == word), "{word} in Help: {t:?}");
        }
    }

    #[test]
    fn any_ai() {
        let (status, data) = (Status::default(), data());
        let mut ai = ai_state("cursor");
        let mut v = view(Section::Settings, &status, &data);
        v.ai = &ai;
        let l = layout(&v, 1000.0, 900.0, &Fake);
        let t = texts(&l);
        for word in ["Connect your AI", "Claude", "Cursor", "Connect", "More", "Make today's capsules with", "Cursor (not connected)", "Make today's capsules opens Cursor with the request."] {
            assert!(t.iter().any(|x| x == word), "{word} in Settings: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Zed" || x == "Not installed"), "not installed: folded under More");
        let actions: Vec<&Action> = l.content.iter().filter_map(|n| n.action()).collect();
        assert!(actions.contains(&&Action::AiMenu("claude".into())) && actions.contains(&&Action::AiConnect("cursor".into())) && actions.contains(&&Action::YourAi));
        assert!(find(&l, glyph::CHECK).is_some(), "the header and Claude show a check");

        ai.more = true;
        ai.busy = Some("cursor".into());
        ai.note = Some("Connected to Cursor. Restart Cursor to use it.".into());
        let mut v = view(Section::Settings, &status, &data);
        v.ai = &ai;
        let t = texts(&layout(&v, 1000.0, 900.0, &Fake));
        for word in ["Zed", "Not installed", "Connecting…", "Connected to Cursor. Restart Cursor to use it."] {
            assert!(t.iter().any(|x| x == word), "{word}: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Connect"), "busy: no Connect button");

        // Home follows Your AI.
        let cursor = ai_state("cursor");
        let mut v = view(Section::Home, &status, &data);
        v.ai = &cursor;
        let t = texts(&layout(&v, 1200.0, 800.0, &Fake));
        assert!(t.iter().any(|x| x == "Make capsules") && t.iter().any(|x| x == "Your AI: Cursor") && t.iter().any(|x| x == "Cursor opens. Paste the request in a new chat."), "{t:?}");
        assert!(!t.iter().any(|x| x == "Copy my day"));
        let claude = ai_state("claude");
        v.ai = &claude;
        let t = texts(&layout(&v, 1200.0, 800.0, &Fake));
        assert!(t.iter().any(|x| x == "Claude opens with the request typed in. Press Enter."));
        let mut web = ai_state("chatgpt-web");
        web.day_line = Some(anyai::PASTE_LINE.into());
        web.working = true;
        v.ai = &web;
        let l = layout(&v, 1200.0, 800.0, &Fake);
        let t = texts(&l);
        assert!(t.iter().any(|x| x == anyai::PASTE_LINE) && t.iter().any(|x| x == "Your AI: ChatGPT"), "{t:?}");
        let off: Vec<&str> = l.content.iter().filter_map(|n| match n { Node::Button { label, enabled: false, .. } => Some(label.as_str()), _ => None }).collect();
        assert_eq!(off, ["Copy my day", "Save from clipboard"], "off while working");

        // Capsules: Ask Claude only when Your AI is Claude; the line after Copy my day.
        let mut v = view(Section::Capsules, &status, &data);
        v.ai = &claude;
        assert!(texts(&layout(&v, 1000.0, 700.0, &Fake)).iter().any(|x| x == "Ask Claude"));
        v.ai = &web;
        let t = texts(&layout(&v, 1000.0, 700.0, &Fake));
        assert!(t.iter().any(|x| x == "Ask your AI") && t.iter().any(|x| x == anyai::PASTE_LINE));

        // Help: the last shortcut follows Your AI.
        let mut v = view(Section::Help, &status, &data);
        v.ai = &claude;
        assert!(texts(&layout(&v, 1000.0, 700.0, &Fake)).iter().any(|x| x == "in your AI"));

        // Nothing overlaps or spills, at every width.
        for section in [Section::Settings, Section::Home, Section::Capsules] {
            for width in [640.0, 1000.0, 1600.0] {
                let mut v = view(section, &status, &data);
                v.ai = &ai;
                let l = layout(&v, width, 700.0, &Fake);
                let rects: Vec<Rect> = controls(&l.content).iter().map(|i| l.content[*i].rect()).collect();
                for (i, a) in rects.iter().enumerate() {
                    assert!(rects[i + 1..].iter().all(|b| !a.overlaps(b)), "{section:?} at {width}: {a:?}");
                    assert!(a.x >= SIDEBAR && a.right() <= width + 0.5, "{section:?} at {width}: inside: {a:?}");
                }
            }
        }
    }

    /// An approximate picture of the layout (SVG, Windows 11 colors, a generic font), to check spacing on a Mac
    /// or Linux: `cargo test -p context-capsule-windows preview -- --ignored`, files in the temp folder.
    #[test]
    #[ignore]
    fn preview() {
        fn esc(s: &str) -> String {
            s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        }
        fn svg(l: &Layout, width: f64, height: f64, dark: bool) -> String {
            // The window's own palette (crate::palette), so the picture matches what Windows draws.
            let t = crate::palette::Theme::new(dark, None);
            let hex = |c: u32| format!("#{:06x}", c & 0xFF_FFFF);
            let (bg, card, border, text, text2, accent, track) =
                (hex(t.bg), hex(t.card), hex(t.card_border), hex(t.text), hex(t.text2), hex(t.accent), hex(t.track));
            let (selected, on_accent, field, red) = (hex(t.subtle_selected), hex(t.on_accent), hex(t.field), hex(t.red));
            let mut out = format!("<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' font-family='Segoe UI, Helvetica, Arial'><rect width='100%' height='100%' fill='{bg}'/>");
            let all = l.sidebar.iter().map(|n| (n, 0.0)).chain(l.header.iter().map(|n| (n, 0.0))).chain(l.content.iter().map(|n| (n, HEADER)));
            for (n, dy) in all {
                let r = n.rect().moved(dy);
                match n {
                    Node::Card(_) => out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='8' fill='{card}' stroke='{border}'/>", r.x, r.y, r.w, r.h),
                    Node::Divider(_) => out += &format!("<rect x='{}' y='{}' width='{}' height='1' fill='{border}'/>", r.x, r.y, r.w.max(1.0)),
                    Node::Text { text: t, font, ink, align, .. } => {
                        if font.is_icon() {
                            continue;
                        }
                        let (size, weight, _) = font.spec();
                        let (x, anchor) = match align { Align::Left => (r.x, "start"), Align::Center => (r.x + r.w / 2.0, "middle"), Align::Right => (r.right(), "end") };
                        let color = if *ink == Ink::Primary { &text } else { &text2 };
                        out += &format!("<text x='{x}' y='{}' font-size='{size}' font-weight='{weight}' fill='{color}' text-anchor='{anchor}'>{}</text>", r.y + size, esc(t));
                    }
                    Node::Button { label, style, .. } => {
                        let (fill, ink) = match style {
                            Style::Accent | Style::Cell { selected: true } => (accent.clone(), on_accent.clone()),
                            Style::Cell { .. } => ("none".into(), text.clone()),
                            Style::Nav { selected: true } | Style::Row { selected: true } => (selected.clone(), text.clone()),
                            Style::Nav { .. } | Style::Row { .. } => ("none".into(), text.clone()),
                            Style::Subtle => ("none".into(), accent.clone()),
                            _ => (card.clone(), text.clone()),
                        };
                        let stroke = if matches!(style, Style::Standard | Style::Dropdown) { border.clone() } else { "none".into() };
                        out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='4' fill='{fill}' stroke='{stroke}'/>", r.x, r.y, r.w, r.h);
                        let (x, anchor) = if matches!(style, Style::Nav { .. }) { (r.x + 44.0, "start") } else if matches!(style, Style::Row { .. } | Style::Dropdown) { (r.x + 12.0, "start") } else { (r.x + r.w / 2.0, "middle") };
                        if label.is_empty() && matches!(style, Style::Subtle) && r.h < 30.0 {
                            continue; // a day bar segment's click area
                        }
                        let label = if label.is_empty() { "⋯".to_string() } else { label.clone() };
                        out += &format!("<text x='{x}' y='{}' font-size='14' fill='{ink}' text-anchor='{anchor}'>{}</text>", r.y + r.h / 2.0 + 5.0, esc(&label));
                    }
                    Node::Toggle { on, .. } => out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='10' fill='{}' stroke='{text2}'/>", r.x, r.y, r.w, r.h, if *on { &accent } else { "none" }),
                    Node::Bar { fraction, muted, .. } => {
                        out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='4' fill='{track}'/>", r.x, r.y, r.w, r.h);
                        out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='4' fill='{}'/>", r.x, r.y, (r.w * fraction).max(6.0), r.h, if *muted { &text2 } else { &accent });
                    }
                    Node::Dot { on, .. } => out += &format!("<circle cx='{}' cy='{}' r='5' fill='{}' stroke='{text2}'/>", r.x + 5.0, r.y + 5.0, if *on { red.as_str() } else { "none" }),
                    Node::Key { text: t, .. } => out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='4' fill='{bg}' stroke='{border}'/><text x='{}' y='{}' font-size='12' font-weight='600' fill='{text}' text-anchor='middle'>{}</text>", r.x, r.y, r.w, r.h, r.x + r.w / 2.0, r.y + 16.0, esc(t)),
                    Node::Pill { text: t, .. } => out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='13' fill='{card}' stroke='{border}'/><text x='{}' y='{}' font-size='12' fill='{text2}' text-anchor='middle'>{}</text>", r.x, r.y, r.w, r.h, r.x + r.w / 2.0, r.y + 17.0, esc(t)),
                    Node::Field { .. } => out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='4' fill='{}' stroke='{border}'/><text x='{}' y='{}' font-size='14' fill='{text}'>Saachi Badal</text>", r.x, r.y, r.w, r.h, field, r.x + 10.0, r.y + 21.0),
                    Node::Badge { text: t, .. } => out += &format!("<circle cx='{}' cy='{}' r='12' fill='{track}'/><text x='{}' y='{}' font-size='12' font-weight='600' fill='{text2}' text-anchor='middle'>{}</text>", r.x + 12.0, r.y + 12.0, r.x + 12.0, r.y + 16.0, esc(t)),
                    Node::Swatch { colour, .. } => {
                        let fill = colour.map_or(track.clone(), |c| hex(t.colour(c)));
                        out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='{}' fill='{fill}'/>", r.x, r.y, r.w, r.h, r.h.min(r.w) / 2.0);
                    }
                    Node::Bubble(_) => out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='12' fill='{selected}'/>", r.x, r.y, r.w, r.h),
                }
            }
            out + "</svg>"
        }
        let mut status = Status { recording: true, started_at: Some(SystemTime::now() - std::time::Duration::from_secs(754)), events: 42, sources: 2, ..Default::default() };
        let data = data();
        status.projects = data.projects.clone();
        status.pinned = Some("web".into());
        for (name, section, width, height, dark) in [
            ("home", Section::Home, 1200.0, 860.0, false),
            ("home-dark", Section::Home, 1200.0, 860.0, true),
            ("home-narrow", Section::Home, 700.0, 1500.0, false),
            ("settings", Section::Settings, 1000.0, 1200.0, false),
            ("capsules", Section::Capsules, 1000.0, 700.0, true),
            ("timeline", Section::Timeline, 1000.0, 900.0, false),
        ] {
            // Your AI: Cursor (Claude connected, Zed under More); narrow Home: a website, after Copy my day.
            let mut ai = ai_state(if name == "home-narrow" { "chatgpt-web" } else { "cursor" });
            ai.more = true;
            if name == "home-narrow" {
                ai.day_line = Some(anyai::PASTE_LINE.into());
            }
            let mut v = view(section, &status, &data);
            v.ai = &ai;
            let l = layout(&v, width, height, &Fake);
            let path = std::env::temp_dir().join(format!("cc-windows-{name}.svg"));
            std::fs::write(&path, svg(&l, width, height.max(l.height + HEADER), dark)).unwrap();
            eprintln!("{}", path.display());
        }
    }

    #[test]
    fn small_words() {
        assert_eq!(app_label("Slack.exe"), "Slack");
        assert_eq!(app_label("KeePassXC"), "KeePassXC");
        assert_eq!(app_label(".exe"), "");
        assert_eq!(without_year("dddd, d MMMM yyyy"), "dddd, d MMMM");
        assert_eq!(without_year("dddd, MMMM d, yyyy"), "dddd, MMMM d");
        assert_eq!(without_year("yyyy年M月d日"), "M月d日");
        assert_eq!(without_year("dddd d. MMMM yyyy"), "dddd d. MMMM");
        assert_eq!(section_icon(Section::Help), glyph::HELP);
        assert_eq!(section_icon(Section::Timeline), glyph::HISTORY);
        assert_eq!((initial("chrome.exe").as_str(), initial(" ").as_str(), initial("éclair").as_str()), ("C", "·", "É"));
    }
}
