// context-capsule — core: reading where you look (the mouse) as well as where you type.
//! Each sample reads the window in use; when the mouse moved or scrolled over a window on another shared
//! screen in the last 30 s, that window is read too (its events say `"focus":"pointer"`).
//! A port of `PointerRule` in `capture/Sources/Core.swift`.

use crate::membership::{ListedWindow, PickedContent, WindowList};
use std::time::{Duration, SystemTime};

pub struct PointerRule;

impl PointerRule {
    /// How long after the mouse last moved or scrolled its screen still counts as "being looked at".
    pub const WINDOW: Duration = Duration::from_secs(30);
    /// Moves smaller than this (pixels) between samples are jitter, not activity.
    pub const MIN_MOVE: f64 = 2.0;

    /// The top normal window under a point (global, top-left origin), private ones included (they're
    /// skipped later).
    pub fn window_at(x: f64, y: f64, list: &[ListedWindow]) -> Option<&ListedWindow> {
        list.iter().find(|w| WindowList::is_normal(w) && w.frame.contains_point(x, y))
    }

    /// Did the mouse move between two samples?
    pub fn moved(last: Option<(f64, f64)>, now: Option<(f64, f64)>) -> bool {
        match (last, now) {
            (Some((ax, ay)), Some((bx, by))) => (ax - bx).hypot(ay - by) > Self::MIN_MOVE,
            _ => false,
        }
    }

    /// Read the window under the mouse too? Only when it's on a shared screen other than the one in use
    /// (or the window in use isn't on a shared screen), it isn't the window in use, and the mouse moved
    /// or scrolled there recently: a mouse just parked on another screen doesn't keep it being read.
    pub fn should_read(
        pointer: Option<&ListedWindow>,
        pointer_pick: Option<&PickedContent>,
        front: Option<&ListedWindow>,
        front_pick: Option<&PickedContent>,
        last_activity: Option<SystemTime>,
        now: SystemTime,
    ) -> bool {
        let (Some(pointer), Some(pointer_pick), Some(last)) = (pointer, pointer_pick, last_activity) else { return false };
        // A time ahead of `now` (clock change) counts as just now.
        if now.duration_since(last).is_ok_and(|d| d > Self::WINDOW) {
            return false;
        }
        if front.is_some_and(|f| f.id == pointer.id) {
            return false;
        }
        match front_pick {
            None => true,
            Some(front_pick) => pointer_pick.source != front_pick.source,
        }
    }
}
