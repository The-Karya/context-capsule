// context-capsule — core: is the front window in what the user picked, which window is in front,
// why a sample was skipped, and what part of a frame to OCR (crop + private masks).
//! Normalized rects here have their origin at the **top left** (images are top-left in memory).
//! The Swift recorder uses Vision's bottom-left origin instead; only the y of normalized rects differs.

use crate::geometry::Rect;
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickStyle {
    Display,
    Window,
}

/// What the user shared. `rect` is its place on screen in global points / pixels (top-left origin).
/// `window_ids` are known when the picker says which windows were picked; else match by frame.
#[derive(Clone, Debug, PartialEq)]
pub struct PickedContent {
    pub style: PickStyle,
    pub rect: Rect,
    pub window_ids: Option<Vec<u64>>,
    /// Which shared screen or window this is, when several are shared (one capture each).
    pub source: u32,
}

impl PickedContent {
    pub fn display(rect: Rect) -> PickedContent {
        PickedContent { style: PickStyle::Display, rect, window_ids: None, source: 0 }
    }
    pub fn window(rect: Rect, window_ids: Option<Vec<u64>>) -> PickedContent {
        PickedContent { style: PickStyle::Window, rect, window_ids, source: 0 }
    }
    pub fn with_source(mut self, source: u32) -> PickedContent {
        self.source = source;
        self
    }
}

/// One on-screen window (front to back in a list). Set by the platform:
/// `layer` 0 = a normal app window (anything else: panels, menus, docks, always-on-top helpers);
/// `excluded`: its app is never recorded; `notification`: a notification banner or toast (it shows
/// message text from any app, so it is never read); `name`: the title, when the OS gives it out.
#[derive(Clone, Debug, PartialEq)]
pub struct ListedWindow {
    pub id: u64,
    pub pid: u32,
    pub owner: String,
    pub layer: i32,
    pub alpha: f64,
    pub frame: Rect,
    pub name: Option<String>,
    pub excluded: bool,
    pub notification: bool,
}

impl Default for ListedWindow {
    fn default() -> Self {
        ListedWindow {
            id: 0,
            pid: 0,
            owner: String::new(),
            layer: 0,
            alpha: 1.0,
            frame: Rect::ZERO,
            name: None,
            excluded: false,
            notification: false,
        }
    }
}

impl ListedWindow {
    pub fn new(id: u64, pid: u32, frame: Rect) -> ListedWindow {
        ListedWindow { id, pid, frame, ..Default::default() }
    }
}

/// Why a sample wrote nothing. Shown in the menu and state.json so the user knows what is going on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Skip {
    PrivateApp,
    NoWindow,
    NotPicked,
    Overlay,
    PrivateWindow,
    WaitingFrame,
    UnknownDisplay,
}

impl Skip {
    pub const ALL: [Skip; 7] = [
        Skip::PrivateApp,
        Skip::NoWindow,
        Skip::NotPicked,
        Skip::Overlay,
        Skip::PrivateWindow,
        Skip::WaitingFrame,
        Skip::UnknownDisplay,
    ];

    /// The state.json value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Skip::PrivateApp => "private_app",         // the front app is never recorded
            Skip::NoWindow => "no_window",             // the front app has no normal window on screen
            Skip::NotPicked => "not_picked",           // the front window is not in what the user picked
            Skip::Overlay => "overlay",                // a window overview shows every window
            Skip::PrivateWindow => "private_window",   // a never-record window covers most of the front window
            Skip::WaitingFrame => "waiting_frame",     // no frame yet since the last app switch
            Skip::UnknownDisplay => "unknown_display", // cannot tell where the picked display is
        }
    }

    /// Plain words for the menu.
    pub fn message(&self) -> &'static str {
        match self {
            Skip::PrivateApp => "a private app is in front (not recorded)",
            Skip::NoWindow => "the front app has no window",
            Skip::NotPicked => "the front window is not in what you picked",
            Skip::Overlay => "the window overview is open",
            Skip::PrivateWindow => "a private app covers the window",
            Skip::WaitingFrame => "waiting for the screen to update",
            Skip::UnknownDisplay => "cannot find the picked screen",
        }
    }

    pub fn index(&self) -> usize {
        Skip::ALL.iter().position(|s| s == self).unwrap_or(0)
    }
}

pub struct WindowList;

impl WindowList {
    /// Smaller than this (points) is a strip, badge or helper window, not a real window.
    pub const MIN_SIDE: f64 = 40.0;

    pub fn is_normal(w: &ListedWindow) -> bool {
        w.layer == 0 && w.alpha > 0.01 && w.frame.width >= Self::MIN_SIDE && w.frame.height >= Self::MIN_SIDE
    }

    /// The app's front window. Invisible (alpha 0) and tiny windows are skipped, and a strip that sits
    /// inside a bigger window of the same app (Chrome and Electron tab bars) gives way to that window.
    pub fn front(pid: u32, list: &[ListedWindow]) -> Option<&ListedWindow> {
        let mut mine = list.iter().filter(|w| w.pid == pid && Self::is_normal(w));
        let first = mine.next()?;
        let t = Membership::TOLERANCE;
        let holder = mine.find(|w| {
            w.frame.inset_by(-t, -t).contains_rect(&first.frame) && w.frame.area() > first.frame.area()
        });
        Some(holder.unwrap_or(first))
    }

    /// The top normal window that is not the OS's own UI (taskbar, Start, menus…): what the user works
    /// in while a system surface is in front. A private app's window is NOT skipped: if it is on top,
    /// it is what the user is looking at, and the sample is then skipped as private.
    pub fn top_regular<'a>(list: &'a [ListedWindow], is_system: &dyn Fn(&ListedWindow) -> bool) -> Option<&'a ListedWindow> {
        list.iter().find(|w| Self::is_normal(w) && !is_system(w))
    }

    /// Never to be read: windows of never-record apps, and notification banners.
    pub fn is_private(w: &ListedWindow) -> bool {
        w.excluded || w.notification
    }
}

/// Frames come only when the screen changes, so right after an app switch the latest frame can still
/// show the previous app. Only use a frame that arrived after the last switch.
pub struct Freshness;

impl Freshness {
    pub const SETTLE: Duration = Duration::from_millis(100);

    pub fn ok(frame_at: Option<SystemTime>, changed_at: Option<SystemTime>) -> bool {
        let Some(frame_at) = frame_at else { return false };
        let Some(changed_at) = changed_at else { return true };
        frame_at >= changed_at + Self::SETTLE
    }
}

/// What to OCR: `region` and `masks` are normalized to the captured image, origin top left.
/// Masks are painted black before OCR.
#[derive(Clone, Debug, PartialEq)]
pub enum OcrDecision {
    Read { region: Rect, masks: Vec<Rect> },
    Skip(Skip),
}

pub struct Membership;

impl Membership {
    /// Points of slack when matching a window frame to the picked rect (shadows, rounding).
    pub const TOLERANCE: f64 = 8.0;
    /// More than this share of the window hidden by private windows: skip the sample.
    pub const MAX_MASKED: f64 = 0.5;

    /// Is the front window part of what the user picked? If not, the sample writes nothing.
    pub fn contains(pick: &PickedContent, w: &ListedWindow) -> bool {
        match pick.style {
            PickStyle::Window => match &pick.window_ids {
                Some(ids) => ids.contains(&w.id),
                None => Self::same_frame(&pick.rect, &w.frame), // ids unknown: match by place and size
            },
            PickStyle::Display => {
                if pick.rect.is_empty() || w.frame.is_empty() {
                    return false;
                }
                pick.rect.contains_point(w.frame.mid_x(), w.frame.mid_y())
            }
        }
    }

    /// The shared content that holds the front window, when several screens or windows are shared. The
    /// window with keyboard focus decides — not the mouse: people type on one screen with the pointer on another.
    pub fn pick<'a>(w: &ListedWindow, picks: &'a [PickedContent]) -> Option<&'a PickedContent> {
        picks.iter().find(|p| Self::contains(p, w))
    }

    /// Is a newly picked screen or window already shared? The system pickers can't hide those, so the
    /// platform refuses it. A display or window whose place or id is unknown is never refused.
    pub fn already_shared(new: &PickedContent, shared: &[PickedContent]) -> bool {
        shared.iter().any(|old| {
            if old.style != new.style {
                return false;
            }
            match new.style {
                PickStyle::Display => !new.rect.is_empty() && Self::same_frame(&old.rect, &new.rect),
                PickStyle::Window => match (&new.window_ids, &old.window_ids) {
                    (Some(a), Some(b)) => !a.is_empty() && a.iter().all(|id| b.contains(id)),
                    _ => Self::same_frame(&old.rect, &new.rect),
                },
            }
        })
    }

    pub fn same_frame(a: &Rect, b: &Rect) -> bool {
        if a.is_empty() || b.is_empty() {
            return false;
        }
        let t = Self::TOLERANCE;
        (a.min_x() - b.min_x()).abs() <= t
            && (a.min_y() - b.min_y()).abs() <= t
            && (a.width - b.width).abs() <= t
            && (a.height - b.height).abs() <= t
    }

    /// The part of the captured image to OCR, normalized, origin top left. A picked window is read
    /// whole; a display is cropped to the front window. None when the window is not on it at all.
    pub fn ocr_region(pick: &PickedContent, window: &Rect) -> Option<Rect> {
        if pick.style == PickStyle::Window {
            return Some(Rect::UNIT);
        }
        let c = pick.rect;
        if c.is_empty() {
            return None;
        }
        let i = c.intersection(window)?;
        if i.width < 1.0 || i.height < 1.0 {
            return None;
        }
        Some(Self::normalized(&i, &c))
    }

    /// `r` (global, top-left origin) as a part of `c`, normalized with the origin at the top left.
    pub fn normalized(r: &Rect, c: &Rect) -> Rect {
        let n = Rect::new(
            (r.min_x() - c.min_x()) / c.width,
            (r.min_y() - c.min_y()) / c.height,
            r.width / c.width,
            r.height / c.height,
        );
        n.intersection(&Rect::UNIT).unwrap_or(Rect::ZERO)
    }

    /// What to OCR for this sample. For a picked display: crop to the front window, and black out any
    /// never-record window above it (floating or overlapping); skip if they hide most of it.
    /// A picked window is captured on its own (other apps are not in the image), so no masks.
    /// `list` is the whole on-screen window list, front to back; `front` is one of its entries.
    pub fn ocr_decision(pick: &PickedContent, front: &ListedWindow, list: &[ListedWindow]) -> OcrDecision {
        if pick.style == PickStyle::Display && pick.rect.is_empty() {
            return OcrDecision::Skip(Skip::UnknownDisplay);
        }
        let Some(region) = Self::ocr_region(pick, &front.frame) else {
            return OcrDecision::Skip(Skip::NotPicked);
        };
        if pick.style != PickStyle::Display {
            return OcrDecision::Read { region, masks: vec![] };
        }
        let Some(crop) = pick.rect.intersection(&front.frame) else {
            return OcrDecision::Skip(Skip::NotPicked);
        };
        let mut masks = vec![];
        let mut hidden = 0.0;
        for w in list.iter().take_while(|w| w.id != front.id) {
            if !WindowList::is_private(w) || w.alpha <= 0.01 {
                continue;
            }
            let Some(i) = w.frame.intersection(&crop) else { continue };
            if i.width < 1.0 || i.height < 1.0 {
                continue;
            }
            masks.push(Self::normalized(&i, &pick.rect));
            hidden += i.width * i.height;
        }
        if hidden > Self::MAX_MASKED * crop.width * crop.height {
            return OcrDecision::Skip(Skip::PrivateWindow);
        }
        OcrDecision::Read { region, masks }
    }
}
