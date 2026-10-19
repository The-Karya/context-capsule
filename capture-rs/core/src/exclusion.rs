// context-capsule — core: the never-record list and config.json (exclude_apps, voice_locale, author).

use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub exclude_apps: Vec<String>,
    /// Locale for voice notes, e.g. "en-GB". None: the user's locale.
    pub voice_locale: Option<String>,
    /// Capsule author name. None: the OS user's name (the MCP decides).
    pub author: Option<String>,
}

impl Config {
    /// Reads `{"exclude_apps": [...], "voice_locale": "en-GB", "author": "…"}`; a missing or broken
    /// file means defaults.
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
        Config { exclude_apps, voice_locale: text("voice_locale"), author: text("author") }
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
    }
}
