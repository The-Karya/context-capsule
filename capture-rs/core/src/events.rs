// context-capsule — core: events.jsonl — one line per event; a repeat of the last sample only moves its `end`.

use crate::json::{json_object, json_string, JsonValue};
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Event {
    pub t: String,
    pub end: String,
    pub app: String,
    pub bundle: String,
    pub title: String,
    /// Where `text` came from: "ax", "ocr", "voice" or "typed". Empty = not written (older files).
    pub src: String,
    pub text: String,
    /// "voice" or "note" for what the user said or typed; empty for screen samples (written without it).
    pub kind: String,
}

impl Event {
    pub const VOICE: &'static str = "voice";
    pub const NOTE: &'static str = "note";

    /// Spoken or typed by the user (not a screen sample). Never merged with another event.
    pub fn is_note(&self) -> bool {
        self.kind == Self::VOICE || self.kind == Self::NOTE
    }

    pub fn same_content(&self, o: &Event) -> bool {
        !self.is_note()
            && !o.is_note()
            && self.kind == o.kind
            && self.src == o.src
            && self.app == o.app
            && self.bundle == o.bundle
            && self.title == o.title
            && self.text == o.text
    }

    /// One events.jsonl line (no newline), keys in SPEC order.
    pub fn line(&self) -> String {
        let s = |v: &str| JsonValue::Str(v.to_string());
        if !self.kind.is_empty() {
            return json_object(&[
                ("t", s(&self.t)),
                ("end", s(&self.end)),
                ("kind", s(&self.kind)),
                ("src", s(&self.src)),
                ("app", s(&self.app)),
                ("bundle", s(&self.bundle)),
                ("title", s(&self.title)),
                ("text", s(&self.text)),
            ]);
        }
        let mut pairs = vec![
            ("t", s(&self.t)),
            ("end", s(&self.end)),
            ("app", s(&self.app)),
            ("bundle", s(&self.bundle)),
            ("title", s(&self.title)),
        ];
        if !self.src.is_empty() {
            pairs.push(("src", s(&self.src)));
        }
        pairs.push(("text", s(&self.text)));
        json_object(&pairs)
    }

    /// Byte offset of the `end` value (after its opening quote) inside `line`.
    pub fn end_value_offset(&self) -> usize {
        "{\"t\":".len() + json_string(&self.t).len() + ",\"end\":\"".len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded {
    Appended,
    Extended,
}

/// Append-only events.jsonl. A repeat of the last sample only moves the last line's `end`.
pub struct EventLog {
    path: PathBuf,
    last: Option<Event>,
    /// Lines written by this log (new events, not extensions).
    appended: usize,
    last_line_start: u64,
}

impl EventLog {
    /// Opens (creating it and its folder if needed) an events file.
    pub fn open(path: &Path) -> std::io::Result<EventLog> {
        if !path.exists() {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            File::create(path)?;
        }
        Ok(EventLog { path: path.to_path_buf(), last: None, appended: 0, last_line_start: 0 })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn appended(&self) -> usize {
        self.appended
    }
    pub fn last(&self) -> Option<&Event> {
        self.last.as_ref()
    }

    pub fn record(&mut self, e: &Event) -> std::io::Result<Recorded> {
        if let Some(prev) = &self.last {
            if prev.same_content(e) {
                let mut moved = prev.clone();
                moved.end = e.end.clone();
                self.rewrite_last(moved)?;
                return Ok(Recorded::Extended);
            }
        }
        let mut f = OpenOptions::new().write(true).open(&self.path)?;
        self.last_line_start = f.seek(SeekFrom::End(0))?;
        f.write_all((e.line() + "\n").as_bytes())?;
        self.last = Some(e.clone());
        self.appended += 1;
        Ok(Recorded::Appended)
    }

    /// Something else happened (excluded app, failed capture): the next sample starts a new event.
    pub fn break_run(&mut self) {
        self.last = None;
    }

    fn rewrite_last(&mut self, e: Event) -> std::io::Result<()> {
        let mut f = OpenOptions::new().write(true).open(&self.path)?;
        let same_len = self.last.as_ref().is_some_and(|old| old.end.len() == e.end.len());
        if same_len {
            // Same length (always, for ISO times): overwrite just the end value in place.
            f.seek(SeekFrom::Start(self.last_line_start + e.end_value_offset() as u64))?;
            f.write_all(e.end.as_bytes())?;
        } else {
            f.set_len(self.last_line_start)?;
            f.seek(SeekFrom::Start(self.last_line_start))?;
            f.write_all((e.line() + "\n").as_bytes())?;
        }
        self.last = Some(e);
        Ok(())
    }
}
