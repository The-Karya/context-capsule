// context-capsule — core: only what's new on screen — remembers the lines already saved for each window in a
// recording, so each sample saves only new lines. No size limit needed; long documents are kept whole as you scroll.

use crate::exclusion::trim_spaces;
use crate::text::lines;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct NewLines {
    seen: HashMap<String, HashSet<String>>,
    /// Least recently used first.
    order: Vec<String>,
}

impl NewLines {
    /// Windows remembered at once; the least recently used is forgotten first.
    pub const MAX_WINDOWS: usize = 64;

    pub fn new() -> NewLines {
        NewLines::default()
    }

    /// The lines of `text` not saved before for `window`, in their order (blank lines dropped, repeats once).
    pub fn filter(&mut self, text: &str, window: &str) -> String {
        let known = self.seen.entry(window.to_string()).or_default();
        let mut out: Vec<String> = vec![];
        for raw in lines(text) {
            let line = trim_spaces(&raw);
            if line.is_empty() || !known.insert(line.to_string()) {
                continue;
            }
            out.push(line.to_string());
        }
        self.order.retain(|w| w != window);
        self.order.push(window.to_string());
        if self.order.len() > Self::MAX_WINDOWS {
            let oldest = self.order.remove(0);
            self.seen.remove(&oldest);
        }
        out.join("\n")
    }

    /// A new recording starts fresh.
    pub fn reset(&mut self) {
        self.seen.clear();
        self.order.clear();
    }

    /// One window: its app (bundle id when known), title and where.
    pub fn key(app: &str, bundle: &str, title: &str, location: &str) -> String {
        let who = if bundle.is_empty() { app } else { bundle };
        [who, title, location].join("\u{1F}")
    }
}
