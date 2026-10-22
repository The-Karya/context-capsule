// context-capsule — core: gaps.jsonl — the stretches of a recording that weren't read, and why.
//! One line per stretch: `{"t":"…","end":"…","why":"private_app|not_shared|no_text|system|locked"}` (SPEC.md,
//! Timeline). Consecutive skipped samples with the same reason extend one line, like events; a stretch shorter
//! than 5 s isn't written. Lines are appended whole and extended in place (the `end` value only), so a reader
//! never sees half a line.

use crate::clock;
use crate::json::{json_object, json_string, JsonValue};
use crate::membership::Skip;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Why a stretch wasn't read (the `why` of a gaps.jsonl line).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Why {
    PrivateApp,
    NotShared,
    NoText,
    System,
    Locked,
}

impl Why {
    pub const ALL: [Why; 5] = [Why::PrivateApp, Why::NotShared, Why::NoText, Why::System, Why::Locked];

    pub fn as_str(self) -> &'static str {
        match self {
            Why::PrivateApp => "private_app",
            Why::NotShared => "not_shared",
            Why::NoText => "no_text",
            Why::System => "system",
            Why::Locked => "locked",
        }
    }

    pub fn parse(s: &str) -> Option<Why> {
        Why::ALL.into_iter().find(|w| w.as_str() == s)
    }

    /// The end of "Not read 10:40–10:55: …".
    pub fn words(self) -> &'static str {
        match self {
            Why::PrivateApp => "a private app was in front",
            Why::NotShared => "this window wasn't shared",
            Why::NoText => "nothing readable",
            Why::System => "system screen",
            Why::Locked => "the screen was locked",
        }
    }

    /// The engine's skip reasons, as gap codes (the Mac's `Gaps.why`). The lock screen in front is "locked",
    /// whatever the sample said.
    pub fn of(skip: Skip, locked: bool) -> Why {
        if locked {
            return Why::Locked;
        }
        match skip {
            Skip::PrivateApp | Skip::PrivateWindow => Why::PrivateApp,
            Skip::NotPicked | Skip::UnknownDisplay => Why::NotShared,
            Skip::Overlay => Why::System,
            Skip::NoWindow | Skip::WaitingFrame => Why::NoText,
        }
    }
}

/// The lock screen is in front (Windows' LockApp; it is a never-record app, so it is skipped as private).
pub fn is_lock_screen(app_id: Option<&str>, app_name: &str) -> bool {
    app_id.is_some_and(|id| id.eq_ignore_ascii_case("LockApp.exe")) || app_name.eq_ignore_ascii_case("LockApp")
}

/// Stretches shorter than this aren't written.
pub const MIN_GAP: Duration = Duration::from_secs(5);

/// One gaps.jsonl line (no newline).
pub fn gap_line(t: &str, end: &str, why: Why) -> String {
    let s = |v: &str| JsonValue::Str(v.to_string());
    json_object(&[("t", s(t)), ("end", s(end)), ("why", s(why.as_str()))])
}

struct Open {
    why: Why,
    t: SystemTime,
    end: SystemTime,
    /// Where its line starts in the file, once written.
    at: Option<u64>,
    /// The `end` written in that line.
    written_end: String,
}

/// gaps.jsonl of one recording. Nothing is created until a stretch is long enough.
pub struct GapLog {
    path: PathBuf,
    open: Option<Open>,
}

impl GapLog {
    pub fn new(path: &Path) -> GapLog {
        GapLog { path: path.to_path_buf(), open: None }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A sample at `now` read nothing, for `why`.
    pub fn skipped(&mut self, why: Why, now: SystemTime) -> std::io::Result<()> {
        match &mut self.open {
            Some(o) if o.why == why && now >= o.end => o.end = now,
            _ => {
                self.open = Some(Open { why, t: now, end: now, at: None, written_end: String::new() });
                return Ok(());
            }
        }
        let path = self.path.clone();
        let Some(o) = self.open.as_mut() else { return Ok(()) };
        if o.end.duration_since(o.t).unwrap_or_default() < MIN_GAP {
            return Ok(());
        }
        let (t, end) = (clock::iso(o.t), clock::iso(o.end));
        match o.at {
            None => {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
                let at = f.seek(SeekFrom::End(0))?;
                f.write_all((gap_line(&t, &end, o.why) + "\n").as_bytes())?;
                o.at = Some(at);
            }
            Some(at) => {
                let mut f = OpenOptions::new().write(true).open(&path)?;
                if o.written_end.len() == end.len() {
                    // Same length (always, for ISO times): just the end value, in place.
                    let offset = "{\"t\":".len() + json_string(&t).len() + ",\"end\":\"".len();
                    f.seek(SeekFrom::Start(at + offset as u64))?;
                    f.write_all(end.as_bytes())?;
                } else {
                    f.set_len(at)?;
                    f.seek(SeekFrom::Start(at))?;
                    f.write_all((gap_line(&t, &end, o.why) + "\n").as_bytes())?;
                }
            }
        }
        o.written_end = end;
        Ok(())
    }

    /// A sample read something (or recording stopped): the stretch is over.
    pub fn read(&mut self) {
        self.open = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons() {
        assert_eq!(Why::of(Skip::PrivateWindow, false), Why::PrivateApp);
        assert_eq!(Why::of(Skip::UnknownDisplay, false), Why::NotShared);
        assert_eq!(Why::of(Skip::WaitingFrame, false), Why::NoText);
        assert_eq!(Why::of(Skip::NoWindow, false), Why::NoText);
        assert_eq!(Why::of(Skip::Overlay, false), Why::System);
        assert_eq!(Why::of(Skip::NotPicked, true), Why::Locked);
        for w in Why::ALL {
            assert_eq!(Why::parse(w.as_str()), Some(w));
        }
        assert_eq!(Why::parse("nope"), None);
        assert!(is_lock_screen(Some("lockapp.exe"), "") && !is_lock_screen(Some("chrome.exe"), "Chrome"));
        assert_eq!(gap_line("a", "b", Why::Locked), r#"{"t":"a","end":"b","why":"locked"}"#);
    }
}
