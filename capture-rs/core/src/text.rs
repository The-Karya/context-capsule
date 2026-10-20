// context-capsule — core: text from the accessibility tree (UIA / AT-SPI), and whether to use it or OCR.
//! Platforms walk the focused window depth-first, parents before children, and map each element's
//! role to a [`NodeKind`]: text that carries a value (static text, edit, document, combo box) is
//! `Value`; headings, links, list items and cells are `Label`; password fields are `Secure` (never
//! read, nor anything inside them); everything else is `Other`.

use std::collections::HashSet;
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NodeKind {
    Value,
    Label,
    Secure,
    #[default]
    Other,
}

/// One element of a window's accessibility tree, in document order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextNode {
    pub depth: usize,
    pub kind: NodeKind,
    pub value: String,
    pub title: String,
    pub desc: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSource {
    Ax,
    Ocr,
}

impl TextSource {
    /// The events.jsonl `src` value ("ax" means any accessibility API).
    pub fn as_str(&self) -> &'static str {
        match self {
            TextSource::Ax => "ax",
            TextSource::Ocr => "ocr",
        }
    }
}

pub struct UiText;

impl UiText {
    /// Below this many letters and digits, accessibility text is not useful (canvas apps, video): use OCR.
    pub const MIN_USEFUL: usize = 80;
    /// The walk stops at whichever comes first: this many elements, or the time budget (keeps the PC smooth).
    pub const MAX_NODES: usize = 20_000;
    pub const BUDGET: Duration = Duration::from_millis(400);
    pub const MAX_DEPTH: usize = 60;
    /// Stop collecting past this (the event's safety cap after redaction).
    pub const MAX_CHARS: usize = 100_000;

    /// The text one node contributes, or "" if its kind carries none.
    pub fn text_of(n: &TextNode) -> &str {
        match n.kind {
            NodeKind::Value => first_filled([&n.value, &n.title, &n.desc]),
            NodeKind::Label => first_filled([&n.title, &n.value, &n.desc]),
            NodeKind::Secure | NodeKind::Other => "",
        }
    }

    /// Lines in document order; secure subtrees skipped; repeated lines kept once; runs of spaces
    /// and tabs collapsed. Stops once about `max_chars` characters are collected.
    pub fn assemble(nodes: &[TextNode], max_chars: usize) -> String {
        let mut out: Vec<String> = vec![];
        let mut seen = HashSet::new();
        let mut total = 0;
        let mut secure_depth: Option<usize> = None;
        for n in nodes {
            if let Some(d) = secure_depth {
                if n.depth > d {
                    continue;
                }
                secure_depth = None;
            }
            if n.kind == NodeKind::Secure {
                secure_depth = Some(n.depth);
                continue;
            }
            for raw in lines(Self::text_of(n)) {
                let line = collapse_spaces(&raw);
                if line.is_empty() || seen.contains(&line) {
                    continue;
                }
                seen.insert(line.clone());
                total += line.graphemes(true).count() + 1;
                out.push(line);
                if total >= max_chars {
                    return out.join("\n");
                }
            }
        }
        out.join("\n")
    }

    /// Letters and digits: what makes text worth keeping.
    pub fn useful_count(s: &str) -> usize {
        s.graphemes(true)
            .filter(|g| g.chars().next().is_some_and(|c| c.is_alphabetic() || c.is_numeric()))
            .count()
    }

    /// Accessibility when it is allowed and gave enough text; otherwise OCR of the picked content.
    pub fn choose(trusted: bool, text: &str) -> TextSource {
        if trusted && Self::useful_count(text) >= Self::MIN_USEFUL { TextSource::Ax } else { TextSource::Ocr }
    }
}

fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn first_filled(candidates: [&str; 3]) -> &str {
    candidates.into_iter().find(|s| !blank(s)).unwrap_or("")
}

/// Swift's `Character.isNewline` (a "\r\n" pair is one character).
fn is_newline(g: &str) -> bool {
    matches!(g, "\n" | "\r" | "\r\n" | "\x0B" | "\x0C" | "\u{85}" | "\u{2028}" | "\u{2029}")
}

/// Split on newlines, dropping empty pieces (Swift's `split`).
pub(crate) fn lines(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    for g in s.graphemes(true) {
        if is_newline(g) {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push_str(g);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Words separated by single spaces (runs of spaces and tabs collapsed, ends trimmed).
fn collapse_spaces(s: &str) -> String {
    let mut words: Vec<String> = vec![];
    let mut cur = String::new();
    for g in s.graphemes(true) {
        if g == " " || g == "\t" {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push_str(g);
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words.join(" ")
}
