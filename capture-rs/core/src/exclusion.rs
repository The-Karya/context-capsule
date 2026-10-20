// context-capsule — core: the never-record list and config.json (exclude_apps, voice_locale, author, remind_at,
// keep_recordings_days, sounds, share_screens).

use crate::settings::ShareScreens;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub exclude_apps: Vec<String>,
    /// Locale for voice notes, e.g. "en-GB". None: the user's locale.
    pub voice_locale: Option<String>,
    /// Capsule author name. None: the OS user's name (the MCP decides).
    pub author: Option<String>,
    /// Evening reminder "Make today's capsules?" at this local time ("18:00"); None = off.
    pub remind_at: Option<String>,
    /// Raw recordings older than this many days are deleted (capsules are kept); 0 = keep forever.
    pub keep_days: u32,
    /// Soft sounds when recording or the mic starts and stops.
    pub sounds: bool,
    /// What starting shares: the picker (one screen or window) or every screen at once.
    pub share_screens: ShareScreens,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            exclude_apps: vec![],
            voice_locale: None,
            author: None,
            remind_at: Some(Self::REMIND_AT.into()),
            keep_days: 30,
            sounds: true,
            share_screens: ShareScreens::Pick,
        }
    }
}

impl Config {
    pub const REMIND_AT: &'static str = "18:00";

    /// Reads `{"exclude_apps": [...], "voice_locale": "en-GB", "author": "…", "remind_at": "18:00" | false,
    /// "keep_recordings_days": 30, "sounds": true, "share_screens": "pick" | "all"}`; a missing or broken file means defaults.
    pub fn load(path: &Path) -> Config {
        std::fs::read(path).map(|d| Config::parse(&d)).unwrap_or_default()
    }

    pub fn parse(data: &[u8]) -> Config {
        let Ok(serde_json::Value::Object(obj)) = serde_json::from_slice::<serde_json::Value>(data) else {
            return Config::default();
        };
        let text = |k: &str| {
            obj.get(k).and_then(|v| v.as_str()).map(trim_spaces).filter(|s| !s.is_empty()).map(str::to_string)
        };
        // Like Swift's `as? [String]`: every entry must be a string, else the list is ignored.
        let exclude_apps = obj
            .get("exclude_apps")
            .and_then(|v| v.as_array())
            .and_then(|a| a.iter().map(|v| v.as_str().map(str::to_string)).collect::<Option<Vec<_>>>())
            .unwrap_or_default();
        // Like Swift: null or false turns the reminder off; a time it can't read means the default.
        let remind_at = match obj.get("remind_at") {
            Some(serde_json::Value::Null) | Some(serde_json::Value::Bool(false)) => None,
            Some(serde_json::Value::String(r)) if crate::reminder::Reminder::parse(r).is_some() => Some(r.clone()),
            _ => Some(Self::REMIND_AT.into()),
        };
        // Like NSNumber.intValue: floats truncate, booleans are 1 / 0; below 0 means 0.
        let keep_days = match obj.get("keep_recordings_days") {
            Some(serde_json::Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(30),
            Some(serde_json::Value::Bool(b)) => *b as i64,
            _ => 30,
        }
        .clamp(0, u32::MAX as i64) as u32;
        let sounds = obj.get("sounds").and_then(|v| v.as_bool()).unwrap_or(true);
        let share_screens = obj.get("share_screens").and_then(|v| v.as_str()).map_or(ShareScreens::Pick, ShareScreens::parse);
        Config { exclude_apps, voice_locale: text("voice_locale"), author: text("author"), remind_at, keep_days, sounds, share_screens }
    }
}

pub struct Exclusion;

impl Exclusion {
    /// Password managers, private messengers, lock screens. Matched as substrings.
    /// "keepass" (KeePass / KeePassXC) and "lockapp" (the Windows lock screen) are for Windows and Linux.
    pub const BUILT_IN: &'static [&'static str] = &[
        "1password", "bitwarden", "keychain", "lastpass", "dashlane", "signal", "whatsapp",
        "com.apple.passwords", "com.apple.loginwindow", "com.apple.screensaver", "keepass", "lockapp",
    ];

    /// Built-ins match anywhere in the app id or app name; user entries match an app id or app name exactly.
    pub fn is_excluded(app_id: Option<&str>, app_name: Option<&str>, user_list: &[String]) -> bool {
        let b = app_id.unwrap_or("").to_lowercase();
        let n = app_name.unwrap_or("").to_lowercase();
        if Self::BUILT_IN.iter().any(|w| b.contains(w) || n.contains(w)) {
            return true;
        }
        user_list.iter().any(|entry| {
            let e = trim_spaces(&entry.to_lowercase()).to_string();
            !e.is_empty() && (e == b || e == n)
        })
    }
}

/// Foundation's `.whitespaces`: spaces and tabs, not newlines.
pub(crate) fn trim_spaces(s: &str) -> &str {
    s.trim_matches(|c: char| {
        c.is_whitespace() && !matches!(c, '\n' | '\r' | '\x0B' | '\x0C' | '\u{85}' | '\u{2028}' | '\u{2029}')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_built_ins() {
        assert!(Exclusion::is_excluded(Some("KeePassXC.exe"), Some("KeePassXC"), &[]));
        assert!(Exclusion::is_excluded(Some("org.keepassxc.KeePassXC"), None, &[]));
        assert!(Exclusion::is_excluded(Some("LockApp.exe"), Some("LockApp"), &[]));
        let c = Config::parse(br#"{"author": " Saachi ", "exclude_apps": ["a", 1]}"#);
        assert_eq!(c.author.as_deref(), Some("Saachi"));
        assert!(c.exclude_apps.is_empty(), "a non-string entry drops the list, like Swift");
        assert_eq!(c.share_screens, ShareScreens::Pick, "missing: pick");
        assert_eq!(Config::parse(br#"{"share_screens": "all"}"#).share_screens, ShareScreens::All);
        assert_eq!(Config::parse(br#"{"share_screens": true}"#).share_screens, ShareScreens::Pick, "not a string: pick");
    }
}
