// context-capsule — core: your own shortcuts — config.json "hotkeys": {"record": "Alt+C", "voice": "Alt+V"}, read,
// written and checked the same way on every recorder (SPEC.md, "Your own shortcuts").
//! A shortcut is modifiers then one key, joined by `+`: modifiers `Ctrl`, `Alt` (Option on a Mac), `Shift`, `Cmd`
//! (Mac only); the key is `A`–`Z`, `0`–`9` or `F1`–`F12`. It needs Ctrl, Alt or Cmd (F-keys may stand alone).
//! Written in the order Ctrl, Alt, Shift, Cmd. Windows and Linux show them as written (`Alt+C`).

use std::fmt;
use std::path::Path;

pub const KEY_HOTKEYS: &str = "hotkeys";

/// The key after the modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// `A`–`Z` (always upper case).
    Letter(char),
    /// `0`–`9`.
    Digit(u8),
    /// `F1`–`F12`.
    F(u8),
}

impl Key {
    /// "C", "7", "F5"; case-insensitive. None: not a key a shortcut can use.
    pub fn parse(s: &str) -> Option<Key> {
        let s = s.trim();
        let mut chars = s.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_ascii_alphabetic() => Some(Key::Letter(c.to_ascii_uppercase())),
            (Some(c), None) if c.is_ascii_digit() => Some(Key::Digit(c as u8 - b'0')),
            (Some('F' | 'f'), Some(_)) => s[1..].parse::<u8>().ok().filter(|n| (1..=12).contains(n) && !s[1..].starts_with('0')).map(Key::F),
            _ => None,
        }
    }

    pub fn is_function(self) -> bool {
        matches!(self, Key::F(_))
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Letter(c) => write!(f, "{c}"),
            Key::Digit(d) => write!(f, "{d}"),
            Key::F(n) => write!(f, "F{n}"),
        }
    }
}

/// One shortcut: modifiers and a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// ⌘ (Mac only).
    pub cmd: bool,
    pub key: Key,
}

impl Shortcut {
    pub const RECORD: Shortcut = Shortcut { ctrl: false, alt: true, shift: false, cmd: false, key: Key::Letter('C') };
    pub const VOICE: Shortcut = Shortcut { ctrl: false, alt: true, shift: false, cmd: false, key: Key::Letter('V') };

    /// "Alt+C", "ctrl + shift + f5", "Option+Command+K"… Modifiers in any order, then exactly one key; None when
    /// it can't be read (no key, two keys, a modifier twice, an unknown name). Not checked for being allowed:
    /// see [`Shortcut::problem`].
    pub fn parse(s: &str) -> Option<Shortcut> {
        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let (key, mods) = parts.split_last()?;
        let mut out = Shortcut { ctrl: false, alt: false, shift: false, cmd: false, key: Key::parse(key)? };
        for m in mods {
            let flag = match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" | "⌃" => &mut out.ctrl,
                "alt" | "option" | "opt" | "⌥" => &mut out.alt,
                "shift" | "⇧" => &mut out.shift,
                "cmd" | "command" | "⌘" => &mut out.cmd,
                _ => return None,
            };
            if *flag {
                return None;
            }
            *flag = true;
        }
        Some(out)
    }

    /// Why it can't be used, if it can't: it needs Ctrl, Alt or (on a Mac) Cmd, unless the key is F1–F12. Cmd
    /// exists only on a Mac (`mac` false: a shortcut with Cmd is not allowed).
    pub fn problem(&self, mac: bool) -> Option<Refusal> {
        if self.cmd && !mac {
            return Some(Refusal::NeedsModifier);
        }
        (!(self.ctrl || self.alt || self.cmd || self.key.is_function())).then_some(Refusal::NeedsModifier)
    }

    /// As written in config.json and shown on Windows and Linux: "Ctrl+Alt+Shift+Cmd+K" (in that order).
    pub fn label(&self) -> String {
        let mut parts: Vec<String> = vec![];
        for (on, name) in [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.cmd, "Cmd")] {
            if on {
                parts.push(name.into());
            }
        }
        parts.push(self.key.to_string());
        parts.join("+")
    }

}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

/// Which of the two shortcuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Which {
    Record,
    Voice,
}

impl Which {
    pub const ALL: [Which; 2] = [Which::Record, Which::Voice];

    /// Its name in sentences ("Already used for Start or stop").
    pub fn title(self) -> &'static str {
        match self {
            Which::Record => START_OR_STOP,
            Which::Voice => SPEAK_A_NOTE,
        }
    }

    /// The key inside config.json "hotkeys".
    pub fn id(self) -> &'static str {
        match self {
            Which::Record => "record",
            Which::Voice => "voice",
        }
    }

    pub fn other(self) -> Which {
        match self {
            Which::Record => Which::Voice,
            Which::Voice => Which::Record,
        }
    }
}

/// Why a new shortcut was refused (the old one stays).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The same keys as the other shortcut.
    SameAs(Which),
    /// No Ctrl / Alt / Cmd (and not an F-key).
    NeedsModifier,
    /// Another app has these keys.
    Taken,
}

impl Refusal {
    /// The short line under the row.
    pub fn message(self, mac: bool) -> String {
        match self {
            Refusal::SameAs(w) => format!("Already used for {}", w.title()),
            Refusal::NeedsModifier if mac => "Add Ctrl, Alt or ⌘".into(),
            Refusal::NeedsModifier => "Add Ctrl or Alt".into(),
            Refusal::Taken => "Another app uses these keys".into(),
        }
    }
}

/// Both shortcuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hotkeys {
    pub record: Shortcut,
    pub voice: Shortcut,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Hotkeys::DEFAULT
    }
}

impl Hotkeys {
    pub const DEFAULT: Hotkeys = Hotkeys { record: Shortcut::RECORD, voice: Shortcut::VOICE };

    pub fn get(&self, which: Which) -> Shortcut {
        match which {
            Which::Record => self.record,
            Which::Voice => self.voice,
        }
    }

    /// With `which` set to `to`.
    pub fn with(&self, which: Which, to: Shortcut) -> Hotkeys {
        let mut h = *self;
        match which {
            Which::Record => h.record = to,
            Which::Voice => h.voice = to,
        }
        h
    }

    /// From config.json's "hotkeys" value. Each one missing, unreadable or not allowed means its default; the two
    /// the same means both defaults. `mac`: whether Cmd is allowed.
    pub fn from_value(value: Option<&serde_json::Value>, mac: bool) -> Hotkeys {
        let one = |which: Which, default: Shortcut| {
            value
                .and_then(|v| v.get(which.id()))
                .and_then(|v| v.as_str())
                .and_then(Shortcut::parse)
                .filter(|s| s.problem(mac).is_none())
                .unwrap_or(default)
        };
        let h = Hotkeys { record: one(Which::Record, Shortcut::RECORD), voice: one(Which::Voice, Shortcut::VOICE) };
        if h.record == h.voice {
            Hotkeys::DEFAULT
        } else {
            h
        }
    }

    /// From config.json (Windows and Linux: no Cmd).
    pub fn load(config: &Path) -> Hotkeys {
        let value = std::fs::read(config).ok().and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok());
        Hotkeys::from_value(value.as_ref().and_then(|v| v.get(KEY_HOTKEYS)), false)
    }

    /// What goes into config.json: {"record": "Alt+C", "voice": "Alt+V"}.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::json!({ "record": self.record.label(), "voice": self.voice.label() })
    }

    /// Can `which` become `to`? (Whether another app has it is the platform's to find out: [`Refusal::Taken`].)
    pub fn check(&self, which: Which, to: Shortcut, mac: bool) -> Result<Hotkeys, Refusal> {
        if let Some(p) = to.problem(mac) {
            return Err(p);
        }
        if self.get(which.other()) == to {
            return Err(Refusal::SameAs(which.other()));
        }
        Ok(self.with(which, to))
    }

    /// Save to config.json (every other key kept).
    pub fn save(&self, config: &Path) -> std::io::Result<()> {
        crate::settings::ConfigFile::set(config, KEY_HOTKEYS, Some(self.to_value()))
    }
}

// MARK: words

pub const START_OR_STOP: &str = "Start or stop";
pub const SPEAK_A_NOTE: &str = "Speak a note";

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> Shortcut {
        Shortcut::parse(text).unwrap()
    }

    #[test]
    fn parse_and_write() {
        assert_eq!(s("Alt+C"), Shortcut::RECORD);
        assert_eq!(s(" alt + v "), Shortcut::VOICE);
        assert_eq!(s("Shift+Ctrl+k").label(), "Ctrl+Shift+K", "written in order");
        assert_eq!(s("Cmd+Shift+Alt+Ctrl+F12").label(), "Ctrl+Alt+Shift+Cmd+F12");
        assert_eq!(s("Option+Command+9").label(), "Alt+Cmd+9");
        assert_eq!(s("F5").label(), "F5");
        assert_eq!(s("⌥+⌘+X").label(), "Alt+Cmd+X");
        for bad in ["", "Alt", "Alt+", "Alt+CC", "Alt+Alt+C", "Super+C", "Alt+F13", "Alt+F0", "Alt+F01", "C+Alt", "Alt+Tab", "Alt+é"] {
            assert_eq!(Shortcut::parse(bad), None, "{bad:?}");
        }
        for text in ["Alt+C", "Ctrl+Shift+5", "F1", "Ctrl+Alt+Shift+Cmd+F12"] {
            assert_eq!(s(text).label(), text, "round trip");
        }
    }

    #[test]
    fn allowed() {
        assert_eq!(s("C").problem(false), Some(Refusal::NeedsModifier));
        assert_eq!(s("Shift+C").problem(false), Some(Refusal::NeedsModifier));
        assert_eq!(s("F7").problem(false), None, "F-keys may stand alone");
        assert_eq!(s("Shift+F7").problem(false), None);
        assert_eq!(s("Ctrl+1").problem(false), None);
        assert_eq!(s("Cmd+K").problem(true), None);
        assert_eq!(s("Cmd+K").problem(false), Some(Refusal::NeedsModifier), "no Cmd off the Mac");
        assert_eq!(Refusal::NeedsModifier.message(false), "Add Ctrl or Alt");
        assert_eq!(Refusal::NeedsModifier.message(true), "Add Ctrl, Alt or ⌘");
        assert_eq!(Refusal::SameAs(Which::Voice).message(false), "Already used for Speak a note");
        assert_eq!(Refusal::SameAs(Which::Record).message(false), "Already used for Start or stop");
        assert_eq!(Refusal::Taken.message(false), "Another app uses these keys");
    }

    #[test]
    fn config_value() {
        let v = |t: &str| serde_json::from_str::<serde_json::Value>(t).unwrap();
        assert_eq!(Hotkeys::from_value(None, false), Hotkeys::DEFAULT, "missing: the defaults");
        let h = Hotkeys::from_value(Some(&v(r#"{"record":"Ctrl+Shift+R","voice":"F9"}"#)), false);
        assert_eq!((h.record.label(), h.voice.label()), ("Ctrl+Shift+R".into(), "F9".into()));
        let h = Hotkeys::from_value(Some(&v(r#"{"record":"R","voice":"nonsense"}"#)), false);
        assert_eq!(h, Hotkeys::DEFAULT, "each bad one: its default");
        let h = Hotkeys::from_value(Some(&v(r#"{"voice":"Ctrl+N"}"#)), false);
        assert_eq!((h.record, h.voice.label()), (Shortcut::RECORD, "Ctrl+N".into()));
        assert_eq!(Hotkeys::from_value(Some(&v(r#"{"record":"Cmd+K"}"#)), false).record, Shortcut::RECORD);
        assert_eq!(Hotkeys::from_value(Some(&v(r#"{"record":"Cmd+K"}"#)), true).record.label(), "Cmd+K");
        assert_eq!(Hotkeys::from_value(Some(&v(r#"{"record":"Alt+V"}"#)), false), Hotkeys::DEFAULT, "the same twice: defaults");
        assert_eq!(Hotkeys::from_value(Some(&v(r#""Alt+C""#)), false), Hotkeys::DEFAULT);
        assert_eq!(Hotkeys::DEFAULT.to_value(), v(r#"{"record":"Alt+C","voice":"Alt+V"}"#));
    }

    #[test]
    fn change_and_save() {
        let h = Hotkeys::DEFAULT;
        assert_eq!(h.check(Which::Record, s("Alt+V"), false), Err(Refusal::SameAs(Which::Voice)));
        assert_eq!(h.check(Which::Voice, s("Alt+C"), false), Err(Refusal::SameAs(Which::Record)));
        assert_eq!(h.check(Which::Voice, s("V"), false), Err(Refusal::NeedsModifier));
        assert_eq!(h.check(Which::Voice, s("Alt+V"), false), Ok(h), "the same keys again is fine");
        let changed = h.check(Which::Record, s("Ctrl+Alt+R"), false).unwrap();
        assert_eq!((changed.record.label(), changed.voice), ("Ctrl+Alt+R".into(), Shortcut::VOICE));

        let dir = std::env::temp_dir().join(format!("cc-shortcuts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.json");
        std::fs::write(&config, r#"{"author":"Saachi"}"#).unwrap();
        assert_eq!(Hotkeys::load(&config), Hotkeys::DEFAULT);
        changed.save(&config).unwrap();
        assert_eq!(Hotkeys::load(&config), changed);
        let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
        assert_eq!(saved["author"], "Saachi", "other keys kept");
        assert_eq!(saved["hotkeys"]["record"], "Ctrl+Alt+R");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
