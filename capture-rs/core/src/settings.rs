// context-capsule — core: settings in config.json — writing one key at a time (every other key kept), what starting
// shares, Light / Dark, and the voice notes' languages the page lists.
//! A port of `ConfigFile` in capture/Sources.

use crate::json::write_atomically;
use std::path::Path;

pub struct ConfigFile;

impl ConfigFile {
    /// Writes one setting into config.json, keeping every other key (and any the app doesn't know) as it was.
    /// None writes `null`. A missing or broken file starts empty.
    pub fn set(path: &Path, key: &str, value: Option<serde_json::Value>) -> std::io::Result<()> {
        let mut obj = match std::fs::read(path).ok().and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok()) {
            Some(serde_json::Value::Object(o)) => o,
            _ => serde_json::Map::new(),
        };
        obj.insert(key.to_string(), value.unwrap_or(serde_json::Value::Null));
        let mut sorted: Vec<(&String, &serde_json::Value)> = obj.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));
        let map: serde_json::Map<String, serde_json::Value> = sorted.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let text = serde_json::to_string_pretty(&serde_json::Value::Object(map)).map_err(std::io::Error::other)?;
        write_atomically(path, &(text + "\n"))
    }
}

pub const KEY_APPEARANCE: &str = "appearance";

/// What starting shares (config.json "share_screens"): the picker, for one screen or window ("pick"), or
/// every screen at once ("all"). The same contract as the Mac.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShareScreens {
    #[default]
    Pick,
    All,
}

impl ShareScreens {
    pub const ALL: [ShareScreens; 2] = [ShareScreens::Pick, ShareScreens::All];

    /// "pick" | "all"
    pub fn value(self) -> &'static str {
        match self {
            ShareScreens::Pick => "pick",
            ShareScreens::All => "all",
        }
    }

    /// Missing or unknown: pick.
    pub fn parse(s: &str) -> ShareScreens {
        let s = s.trim().to_lowercase();
        ShareScreens::ALL.into_iter().find(|a| a.value() == s).unwrap_or_default()
    }

    /// From config.json.
    pub fn load(config: &Path) -> ShareScreens {
        crate::exclusion::Config::load(config).share_screens
    }
}

/// Light or dark: like the computer, or always one of them (config.json "appearance").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];

    /// "system" | "light" | "dark"
    pub fn value(self) -> &'static str {
        match self {
            Appearance::System => "system",
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }

    pub fn parse(s: &str) -> Appearance {
        Appearance::ALL.into_iter().find(|a| a.value() == s.trim()).unwrap_or_default()
    }

    /// From config.json; missing or unknown: like the computer.
    pub fn load(config: &Path) -> Appearance {
        let value = std::fs::read(config).ok().and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok());
        value.as_ref().and_then(|v| v.get(KEY_APPEARANCE)).and_then(|v| v.as_str()).map_or(Appearance::System, Appearance::parse)
    }

    /// Dark or not, given whether the computer is dark.
    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            Appearance::System => system_dark,
            Appearance::Light => false,
            Appearance::Dark => true,
        }
    }
}

/// Languages for voice notes (whisper's multilingual model): a locale for config.json, and its name.
/// The first entry ("") means the computer's own language.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("ar-SA", "Arabic"),
    ("zh-CN", "Chinese"),
    ("cs-CZ", "Czech"),
    ("da-DK", "Danish"),
    ("nl-NL", "Dutch"),
    ("en-US", "English"),
    ("fi-FI", "Finnish"),
    ("fr-FR", "French"),
    ("de-DE", "German"),
    ("el-GR", "Greek"),
    ("he-IL", "Hebrew"),
    ("hi-IN", "Hindi"),
    ("hu-HU", "Hungarian"),
    ("id-ID", "Indonesian"),
    ("it-IT", "Italian"),
    ("ja-JP", "Japanese"),
    ("ko-KR", "Korean"),
    ("ms-MY", "Malay"),
    ("nb-NO", "Norwegian"),
    ("pl-PL", "Polish"),
    ("pt-PT", "Portuguese"),
    ("ro-RO", "Romanian"),
    ("ru-RU", "Russian"),
    ("es-ES", "Spanish"),
    ("sv-SE", "Swedish"),
    ("ta-IN", "Tamil"),
    ("th-TH", "Thai"),
    ("tr-TR", "Turkish"),
    ("uk-UA", "Ukrainian"),
    ("ur-PK", "Urdu"),
    ("vi-VN", "Vietnamese"),
];

/// "Same as my computer", then every language; a saved locale not in the list is shown as it is.
pub fn language_options(current: Option<&str>) -> Vec<(String, String)> {
    let mut list = vec![(String::new(), SAME_LANGUAGE.to_string())];
    list.extend(LANGUAGES.iter().map(|(id, name)| (id.to_string(), name.to_string())));
    if let Some(c) = current.filter(|c| !c.is_empty()) {
        let lang = |s: &str| s.split(['-', '_']).next().unwrap_or("").to_lowercase();
        if !LANGUAGES.iter().any(|(id, _)| lang(id) == lang(c)) {
            list.push((c.to_string(), c.to_string()));
        }
    }
    list
}

pub const SAME_LANGUAGE: &str = "Same as my computer";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exclusion::Config;

    #[test]
    fn one_key_at_a_time() {
        let dir = std::env::temp_dir().join(format!("cc-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.json");
        ConfigFile::set(&path, "sounds", Some(false.into())).unwrap();
        ConfigFile::set(&path, "remind_at", Some("19:30".into())).unwrap();
        ConfigFile::set(&path, "exclude_apps", Some(serde_json::json!(["com.example.app"]))).unwrap();
        let c = Config::load(&path);
        assert!(!c.sounds && c.remind_at.as_deref() == Some("19:30") && c.exclude_apps == ["com.example.app"]);
        std::fs::write(&path, r#"{"author": "Saachi", "future_key": 7}"#).unwrap();
        ConfigFile::set(&path, "remind_at", Some(false.into())).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"author\": \"Saachi\"") && text.contains("future_key"), "other keys kept: {text}");
        assert!(text.find("author") < text.find("future_key") && text.find("future_key") < text.find("remind_at"), "sorted");
        assert_eq!(Config::load(&path).remind_at, None);
        std::fs::write(&path, "broken").unwrap();
        ConfigFile::set(&path, "keep_recordings_days", Some(7.into())).unwrap();
        assert_eq!(Config::load(&path).keep_days, 7, "a broken file starts over");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn languages() {
        assert_eq!(language_options(None)[0].1, SAME_LANGUAGE);
        assert_eq!(language_options(None).len(), LANGUAGES.len() + 1);
        assert_eq!(language_options(Some("en-GB")).len(), LANGUAGES.len() + 1, "same language: not added");
        assert_eq!(language_options(Some("eu-ES")).last().unwrap().0, "eu-ES", "unknown: kept");
    }

    #[test]
    fn share_screens() {
        assert_eq!(ShareScreens::parse("all"), ShareScreens::All);
        assert_eq!(ShareScreens::parse(" ALL "), ShareScreens::All);
        assert_eq!(ShareScreens::parse("pick"), ShareScreens::Pick);
        assert_eq!(ShareScreens::parse("every"), ShareScreens::Pick, "unknown: pick");
        let dir = std::env::temp_dir().join(format!("cc-share-{}", std::process::id()));
        let path = dir.join("config.json");
        assert_eq!(ShareScreens::load(&path), ShareScreens::Pick, "default");
        ConfigFile::set(&path, "share_screens", Some(ShareScreens::All.value().into())).unwrap();
        assert_eq!(ShareScreens::load(&path), ShareScreens::All);
        assert!(std::fs::read_to_string(&path).unwrap().contains(r#""share_screens": "all""#));
        ConfigFile::set(&path, "share_screens", Some(ShareScreens::Pick.value().into())).unwrap();
        assert_eq!(ShareScreens::load(&path), ShareScreens::Pick);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn appearance() {
        assert_eq!(Appearance::parse("dark"), Appearance::Dark);
        assert_eq!(Appearance::parse("purple"), Appearance::System);
        assert!(Appearance::System.is_dark(true) && !Appearance::Light.is_dark(true) && Appearance::Dark.is_dark(false));
        let dir = std::env::temp_dir().join(format!("cc-appearance-{}", std::process::id()));
        let path = dir.join("config.json");
        assert_eq!(Appearance::load(&path), Appearance::System);
        ConfigFile::set(&path, KEY_APPEARANCE, Some(Appearance::Light.value().into())).unwrap();
        assert_eq!(Appearance::load(&path), Appearance::Light);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
