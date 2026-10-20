// context-capsule — core: changing settings from the window — one key of config.json at a time (every other key
// kept), the choices Settings offers, and editing projects (rename, "Found by" clues, delete).
//! A port of `ConfigFile`, `Preferences` and `LibraryModel`'s edits in capture/Sources.

use crate::exclusion::trim_spaces;
use crate::json::write_atomically;
use crate::projects::{Project, Projects};
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

pub const KEY_AUTHOR: &str = "author";
pub const KEY_REMIND_AT: &str = "remind_at";
pub const KEY_KEEP_DAYS: &str = "keep_recordings_days";
pub const KEY_SOUNDS: &str = "sounds";
pub const KEY_VOICE_LOCALE: &str = "voice_locale";
pub const KEY_EXCLUDE_APPS: &str = "exclude_apps";
pub const KEY_APPEARANCE: &str = "appearance";

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

    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "System",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
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

/// The name to save: trimmed; empty means none (`null`).
pub fn author_value(typed: &str) -> Option<serde_json::Value> {
    let clean = trim_spaces(typed);
    (!clean.is_empty()).then(|| serde_json::Value::String(clean.to_string()))
}

/// The first word of a name, for the greeting.
pub fn first_name(name: &str) -> String {
    name.split_whitespace().next().unwrap_or("").to_string()
}

/// "Delete old recordings after": the choices, and the current value when it is another one.
pub fn keep_options(current: u32) -> Vec<(u32, String)> {
    let mut list: Vec<(u32, String)> =
        [(7, "1 week"), (30, "30 days"), (90, "90 days"), (0, "Never")].iter().map(|(d, s)| (*d, s.to_string())).collect();
    if !list.iter().any(|(d, _)| *d == current) {
        list.insert(list.len() - 1, (current, if current == 1 { "1 day".into() } else { format!("{current} days") }));
    }
    list
}

/// The evening reminder's time choices, every half hour from noon to 11:30 pm ("18:00"), plus the current one.
pub fn remind_options(current: Option<&str>) -> Vec<String> {
    let mut list: Vec<String> = (24..48).map(|half| format!("{:02}:{:02}", half / 2, (half % 2) * 30)).collect();
    if let Some(c) = current.filter(|c| crate::reminder::Reminder::parse(c).is_some()) {
        let c = normalize_time(c);
        if !list.contains(&c) {
            list.push(c);
            list.sort();
        }
    }
    list
}

/// "7:5" → "07:05".
pub fn normalize_time(hhmm: &str) -> String {
    crate::reminder::Reminder::parse(hhmm).map_or_else(|| hhmm.to_string(), |m| format!("{:02}:{:02}", m / 60, m % 60))
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

/// The option to select for the saved locale: the same language counts ("en-GB" selects English).
pub fn language_index(current: Option<&str>) -> usize {
    let options = language_options(current);
    let Some(c) = current.filter(|c| !c.is_empty()) else { return 0 };
    if let Some(i) = options.iter().position(|(id, _)| id == c) {
        return i;
    }
    let lang = |s: &str| s.split(['-', '_']).next().unwrap_or("").to_lowercase();
    options.iter().position(|(id, _)| !id.is_empty() && lang(id) == lang(c)).unwrap_or(0)
}

pub const SAME_LANGUAGE: &str = "Same as my computer";

// MARK: editing projects

/// "a, b ,x" → ["a", "b"]: comma-separated, trimmed, at least 2 characters each.
pub fn parse_clues(text: &str) -> Vec<String> {
    text.split(',').map(trim_spaces).filter(|c| c.chars().count() >= 2).map(str::to_string).collect()
}

/// Rename a project (an empty name changes nothing).
pub fn rename(projects: &[Project], id: &str, name: &str) -> Vec<Project> {
    let clean = trim_spaces(name);
    projects
        .iter()
        .map(|p| if p.id == id && !clean.is_empty() { Project { name: clean.to_string(), ..p.clone() } } else { p.clone() })
        .collect()
}

pub fn set_clues(projects: &[Project], id: &str, text: &str) -> Vec<Project> {
    let clues = parse_clues(text);
    projects.iter().map(|p| if p.id == id { Project { clues: clues.clone(), ..p.clone() } } else { p.clone() }).collect()
}

pub fn delete(projects: &[Project], id: &str) -> Vec<Project> {
    projects.iter().filter(|p| p.id != id).cloned().collect()
}

/// Rename and set clues in one go, then save projects.json. Returns the saved list.
pub fn save_project_edit(path: &Path, id: &str, name: &str, clues: &str) -> std::io::Result<Vec<Project>> {
    let list = set_clues(&rename(&Projects::load(path), id, name), id, clues);
    Projects::save(&list, path)?;
    Ok(list)
}

/// Delete a project from projects.json (its capsules stay). Returns the saved list.
pub fn delete_project(path: &Path, id: &str) -> std::io::Result<Vec<Project>> {
    let list = delete(&Projects::load(path), id);
    Projects::save(&list, path)?;
    Ok(list)
}

/// Add an app to the never-record list (no repeats, case-insensitive). Returns the new list.
pub fn add_excluded(list: &[String], app: &str) -> Vec<String> {
    let app = trim_spaces(app);
    let mut out = list.to_vec();
    if !app.is_empty() && !out.iter().any(|a| a.eq_ignore_ascii_case(app)) {
        out.push(app.to_string());
    }
    out
}

pub fn remove_excluded(list: &[String], app: &str) -> Vec<String> {
    list.iter().filter(|a| a.as_str() != app).cloned().collect()
}

pub fn strings(list: &[String]) -> serde_json::Value {
    serde_json::Value::Array(list.iter().map(|s| serde_json::Value::String(s.clone())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exclusion::Config;

    #[test]
    fn one_key_at_a_time() {
        let dir = std::env::temp_dir().join(format!("cc-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("config.json");
        ConfigFile::set(&path, KEY_SOUNDS, Some(false.into())).unwrap();
        ConfigFile::set(&path, KEY_REMIND_AT, Some("19:30".into())).unwrap();
        ConfigFile::set(&path, KEY_EXCLUDE_APPS, Some(strings(&["com.example.app".into()]))).unwrap();
        let c = Config::load(&path);
        assert!(!c.sounds && c.remind_at.as_deref() == Some("19:30") && c.exclude_apps == ["com.example.app"]);
        std::fs::write(&path, r#"{"author": "Saachi", "future_key": 7}"#).unwrap();
        ConfigFile::set(&path, KEY_REMIND_AT, Some(false.into())).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"author\": \"Saachi\"") && text.contains("future_key"), "other keys kept: {text}");
        assert!(text.find("author") < text.find("future_key") && text.find("future_key") < text.find("remind_at"), "sorted");
        assert_eq!(Config::load(&path).remind_at, None);
        ConfigFile::set(&path, KEY_AUTHOR, author_value("  ")).unwrap();
        assert_eq!(Config::load(&path).author, None, "an empty name is null");
        std::fs::write(&path, "broken").unwrap();
        ConfigFile::set(&path, KEY_KEEP_DAYS, Some(7.into())).unwrap();
        assert_eq!(Config::load(&path).keep_days, 7, "a broken file starts over");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn choices() {
        assert_eq!(first_name("Saachi Badal"), "Saachi");
        assert_eq!(first_name(""), "");
        assert_eq!(author_value(" Saachi "), Some("Saachi".into()));
        let days: Vec<u32> = keep_options(30).iter().map(|(d, _)| *d).collect();
        assert_eq!(days, [7, 30, 90, 0]);
        assert_eq!(keep_options(14)[3], (14, "14 days".to_string()), "a hand-set value is shown, before Never");
        let times = remind_options(Some("18:00"));
        assert_eq!((times.first().unwrap().as_str(), times.last().unwrap().as_str(), times.len()), ("12:00", "23:30", 24));
        assert!(remind_options(Some("7:5")).contains(&"07:05".to_string()));
        assert_eq!(language_options(None)[0].1, SAME_LANGUAGE);
        assert_eq!(language_options(None)[language_index(Some("en-GB"))].1, "English", "same language");
        assert_eq!(language_index(None), 0);
        assert_eq!(language_options(Some("eu-ES")).last().unwrap().0, "eu-ES", "unknown: kept");
        assert_eq!(language_options(Some("eu-ES"))[language_index(Some("eu-ES"))].0, "eu-ES");
    }

    #[test]
    fn appearance() {
        assert_eq!(Appearance::parse("dark"), Appearance::Dark);
        assert_eq!(Appearance::parse("purple"), Appearance::System);
        assert_eq!(Appearance::ALL.map(|a| a.label()), ["System", "Light", "Dark"]);
        assert!(Appearance::System.is_dark(true) && !Appearance::Light.is_dark(true) && Appearance::Dark.is_dark(false));
        let dir = std::env::temp_dir().join(format!("cc-appearance-{}", std::process::id()));
        let path = dir.join("config.json");
        assert_eq!(Appearance::load(&path), Appearance::System);
        ConfigFile::set(&path, KEY_APPEARANCE, Some(Appearance::Light.value().into())).unwrap();
        assert_eq!(Appearance::load(&path), Appearance::Light);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn project_edits() {
        let list = vec![
            Project { id: "web".into(), name: "Website".into(), clues: vec!["acme.com".into()] },
            Project { id: "app".into(), name: "App".into(), clues: vec![] },
        ];
        assert_eq!(parse_clues("figma, acme.com ,x, "), ["figma", "acme.com"]);
        assert_eq!(rename(&list, "web", "  New site ")[0].name, "New site");
        assert_eq!(rename(&list, "web", "  ")[0].name, "Website", "empty: unchanged");
        assert_eq!(set_clues(&list, "app", "ios, x")[1].clues, ["ios"]);
        assert_eq!(delete(&list, "web").len(), 1);
        assert_eq!(add_excluded(&["Slack.exe".into()], "slack.exe").len(), 1, "no repeats");
        assert_eq!(add_excluded(&[], " Notes.exe "), ["Notes.exe"]);
        assert!(remove_excluded(&["a".into()], "a").is_empty());
        let dir = std::env::temp_dir().join(format!("cc-projects-edit-{}", std::process::id()));
        let path = dir.join("projects.json");
        Projects::save(&list, &path).unwrap();
        let saved = save_project_edit(&path, "app", "Mobile app", "ios, android").unwrap();
        assert_eq!(Projects::load(&path), saved);
        assert_eq!(saved[1], Project { id: "app".into(), name: "Mobile app".into(), clues: vec!["ios".into(), "android".into()] });
        assert_eq!(delete_project(&path, "web").unwrap().len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
