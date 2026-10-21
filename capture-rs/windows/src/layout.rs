// context-capsule — windows: the Context Capsule window's layout — what goes where for Home, Capsules, Projects,
// Settings and Help, as a flat list of shapes, words and controls in device-independent pixels. Pure (no Windows
// calls): text is measured through `Measure`, so it runs and is tested on any OS. `main_window` draws it.
//! The words and numbers come from core's `window` module, the same as the Mac's window. Raw recordings are
//! never shown (only time per project, counts and capsules).

use crate::setup::{Done, Step};
use context_capsule_core::capsules::{self, CapsuleSummary};
use context_capsule_core::share_check::{self, Kind};
use context_capsule_core::settings::{self, Appearance};
use context_capsule_core::window::{self as w, HomeNumbers, Keys, Section};
use context_capsule_core::{Config, Project, Status};
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
}

/// The text boxes the window shows (real Windows edit controls, placed where the layout says).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    Name,
    ProjectName,
    ProjectClues,
    /// Capsules: filters by title and "In short" as you type.
    Search,
}

impl Field {
    pub const ALL: [Field; 4] = [Field::Name, Field::ProjectName, Field::ProjectClues, Field::Search];
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
    /// Capsules: a capsule someone sent (a file dialog), and Claude searching them.
    OpenCapsule,
    AskClaude,
    /// Check before sharing: one item's check box, Share, Cancel.
    ShareToggle(usize),
    ShareGo,
    ShareCancel,
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
}

impl Node {
    /// Buttons, switches and text boxes take clicks and keyboard focus.
    pub fn is_control(&self) -> bool {
        matches!(self, Node::Button { enabled: true, .. } | Node::Toggle { .. } | Node::Field { .. })
    }
    pub fn rect(&self) -> Rect {
        match self {
            Node::Card(r) | Node::Divider(r) => *r,
            Node::Text { r, .. }
            | Node::Button { r, .. }
            | Node::Toggle { r, .. }
            | Node::Bar { r, .. }
            | Node::Dot { r, .. }
            | Node::Key { r, .. }
            | Node::Pill { r, .. }
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
    pub const MIC: &str = "\u{E720}";
    pub const LINK: &str = "\u{E71B}";
    pub const POWER: &str = "\u{E7E8}";
}

pub fn section_icon(s: Section) -> &'static str {
    match s {
        Section::Home => glyph::HOME,
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
    col.caption(w::TODAY);
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
            let kx = keycaps(out, m, x + CARD_PAD, y, keys);
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

    // Make today's capsules, with the main button.
    let hint = w::make_hint(v.keys);
    col.card(|out, x, w, top, m| {
        let inner = w - 2.0 * CARD_PAD;
        let mut y = top + CARD_PAD;
        let th = m.text(w::MAKE_TITLE, Font::Strong, inner, 0).1;
        out.push(Node::Text { r: Rect::new(x + CARD_PAD, y, inner, th), text: w::MAKE_TITLE.into(), font: Font::Strong, ink: Ink::Primary, align: Align::Left, lines: 0 });
        y += th + 2.0;
        let hh = m.text(&hint, Font::Small, inner, 0).1;
        out.push(Node::Text { r: Rect::new(x + CARD_PAD, y, inner, hh), text: hint.clone(), font: Font::Small, ink: Ink::Secondary, align: Align::Left, lines: 0 });
        y += hh + 12.0;
        out.push(Node::Button { r: Rect::new(x + CARD_PAD, y, inner, BUTTON_H), action: Action::Make, label: w::MAKE_BUTTON.into(), icon: None, style: Style::Accent, enabled: true, check: false });
        y + BUTTON_H + CARD_PAD
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

    // Search as you type, or ask Claude (it reads inside every capsule).
    let asking = !v.search.trim().is_empty();
    let aw = button_width(col.m, w::ASK_CLAUDE, Style::Accent);
    col.nodes.push(Node::Field { r: Rect::new(col.x, col.y, (col.w - aw - 8.0).max(80.0), BUTTON_H), field: Field::Search });
    col.nodes.push(Node::Button {
        r: Rect::new(col.x + col.w - aw, col.y, aw, BUTTON_H),
        action: Action::AskClaude,
        label: w::ASK_CLAUDE.into(),
        icon: None,
        style: Style::Accent,
        enabled: asking,
        check: false,
    });
    col.gap(BUTTON_H + GAP);

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
    col.card(|out, x, w, top, m| {
        let mut y = top;
        for (i, step) in Step::ALL.iter().enumerate() {
            if i > 0 {
                divider(out, x, w, y);
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
    let lines = w::shortcuts(v.keys);
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
                out.push(Node::Text { r: Rect::new(x + w - CARD_PAD - kw, y + (h - th) / 2.0, kw, th), text: (*keys).into(), font: Font::Body, ink: Ink::Secondary, align: Align::Right, lines: 1 });
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
    use context_capsule_core::window::WINDOWS_KEYS;
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
            keys: &WINDOWS_KEYS,
            search: "",
            share: None,
        }
    }

    #[test]
    fn capsules_search_ask_and_open() {
        let (status, data) = (Status::default(), data());
        let mut v = view(Section::Capsules, &status, &data);
        let l = layout(&v, 1000.0, 700.0, &Fake);
        let ask = |l: &Layout| l.content.iter().find_map(|n| if let Node::Button { action: Action::AskClaude, enabled, .. } = n { Some(*enabled) } else { None });
        assert_eq!(ask(&l), Some(false), "Ask Claude: off while the box is empty");
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
            "New project", "Make today's capsules", "Make", "Streak", "3 days", "in a row", "2 capsules", "contextualized"] {
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
        for word in ["You", "Your name", "Set up", "Voice notes", "Connect Claude", "Connect", "Open at login", "Every day", "When you start", "Pick a screen or window", "Remind me to make capsules",
            "18:00", "30 days", "Sounds", "Capsules are never deleted.", "Language", "Same as my computer", "Appearance", "System", "Never record", "Slack", "Remove", "Choose…", "Updates", "Update automatically", "Check for updates"] {
            assert!(t.iter().any(|x| x == word), "{word} in Settings: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "Allow" || x == "Turn On"), "done rows show a check instead");
        let t = texts(&layout(&view(Section::Help, &status, &data), 1000.0, 700.0, &Fake));
        for word in ["Start or stop contextualizing", "Alt", "C", "V", "in Claude", "How it works", "Privacy"] {
            assert!(t.iter().any(|x| x == word), "{word} in Help: {t:?}");
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
            let c = |light: &str, darkc: &str| if dark { darkc.to_string() } else { light.to_string() };
            let (bg, card, border, text, text2, accent, track) = (
                c("#f3f3f3", "#202020"), c("#fbfbfb", "#2b2b2b"), c("#e5e5e5", "#1c1c1c"), c("#1b1b1b", "#ffffff"),
                c("#5d5d5d", "#c8c8c8"), c("#005fb8", "#60cdff"), c("#e6e6e6", "#3d3d3d"),
            );
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
                            Style::Accent => (accent.clone(), c("#ffffff", "#000000")),
                            Style::Nav { selected: true } | Style::Row { selected: true } => (c("#e6e6e6", "#333333"), text.clone()),
                            Style::Nav { .. } | Style::Row { .. } => ("none".into(), text.clone()),
                            Style::Subtle => ("none".into(), accent.clone()),
                            _ => (card.clone(), text.clone()),
                        };
                        let stroke = if matches!(style, Style::Standard | Style::Dropdown) { border.clone() } else { "none".into() };
                        out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='4' fill='{fill}' stroke='{stroke}'/>", r.x, r.y, r.w, r.h);
                        let (x, anchor) = if matches!(style, Style::Nav { .. }) { (r.x + 44.0, "start") } else if matches!(style, Style::Row { .. } | Style::Dropdown) { (r.x + 12.0, "start") } else { (r.x + r.w / 2.0, "middle") };
                        let label = if label.is_empty() { "⋯".to_string() } else { label.clone() };
                        out += &format!("<text x='{x}' y='{}' font-size='14' fill='{ink}' text-anchor='{anchor}'>{}</text>", r.y + r.h / 2.0 + 5.0, esc(&label));
                    }
                    Node::Toggle { on, .. } => out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='10' fill='{}' stroke='{text2}'/>", r.x, r.y, r.w, r.h, if *on { &accent } else { "none" }),
                    Node::Bar { fraction, muted, .. } => {
                        out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='4' fill='{track}'/>", r.x, r.y, r.w, r.h);
                        out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='4' fill='{}'/>", r.x, r.y, (r.w * fraction).max(6.0), r.h, if *muted { &text2 } else { &accent });
                    }
                    Node::Dot { on, .. } => out += &format!("<circle cx='{}' cy='{}' r='5' fill='{}' stroke='{text2}'/>", r.x + 5.0, r.y + 5.0, if *on { "#c42b1c" } else { "none" }),
                    Node::Key { text: t, .. } => out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='4' fill='{bg}' stroke='{border}'/><text x='{}' y='{}' font-size='12' font-weight='600' fill='{text}' text-anchor='middle'>{}</text>", r.x, r.y, r.w, r.h, r.x + r.w / 2.0, r.y + 16.0, esc(t)),
                    Node::Pill { text: t, .. } => out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='13' fill='{card}' stroke='{border}'/><text x='{}' y='{}' font-size='12' fill='{text2}' text-anchor='middle'>{}</text>", r.x, r.y, r.w, r.h, r.x + r.w / 2.0, r.y + 17.0, esc(t)),
                    Node::Field { .. } => out += &format!("<rect x='{}' y='{}' width='{}' height='{}' rx='4' fill='{}' stroke='{border}'/><text x='{}' y='{}' font-size='14' fill='{text}'>Saachi Badal</text>", r.x, r.y, r.w, r.h, c("#ffffff", "#2d2d2d"), r.x + 10.0, r.y + 21.0),
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
        ] {
            let l = layout(&view(section, &status, &data), width, height, &Fake);
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
    }
}
