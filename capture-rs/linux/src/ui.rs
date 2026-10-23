// context-capsule — linux: the window's small decisions that need no GTK — the `--window-only` flag, the
// first-launch marker, the account's full name, app ids for the never-record list, keycaps, the date line and
// the values Settings writes. The words and Home's numbers are shared (core/src/window.rs).
#![cfg_attr(not(all(feature = "window", feature = "voice")), allow(dead_code))]

use context_capsule_core::capsules::{project_name, CapsuleSummary};
use context_capsule_core::window::{content_width, Section, MAX_CONTENT, MAX_CONTENT_WIDE};
use context_capsule_core::shortcuts::{self, Key, Pressed};
use context_capsule_core::Project;
use std::path::Path;

/// Space around a page's content (left and right; top and bottom are a bit less).
pub const PAGE_MARGIN: f64 = 32.0;
/// The sidebar's width.
pub const SIDEBAR_WIDTH: i32 = 190;

/// `--window-only` (Home) or `--window-only=settings`: just the window, no tray, engine or portal (to try the
/// window and take pictures of it). None: not asked.
pub fn window_only_arg(args: &[String]) -> Option<Section> {
    args.iter().find_map(|a| {
        if a == "--window-only" {
            Some(Section::Home)
        } else {
            a.strip_prefix("--window-only=").map(|id| section_from_id(id).unwrap_or(Section::Home))
        }
    })
}

/// The shared window (ui/, in WebKitGTK), the default. The GTK window only with `--old-window` or config.json
/// "new_window": false (`--new-window` wins over the config).
/// Off unless asked.
pub fn new_window(args: &[String], config: &[u8]) -> bool {
    if args.iter().any(|a| a == "--old-window") {
        return false;
    }
    args.iter().any(|a| a == "--new-window")
        || serde_json::from_slice::<serde_json::Value>(config).ok().and_then(|v| v.get("new_window").and_then(|n| n.as_bool())) != Some(false)
}

/// "1100x760" → (1100, 760).
pub fn parse_size(s: &str) -> Option<(i32, i32)> {
    let (w, h) = s.trim().split_once('x')?;
    Some((w.parse().ok().filter(|w| *w > 0)?, h.parse().ok().filter(|h| *h > 0)?))
}

/// "settings" → Settings.
pub fn section_from_id(id: &str) -> Option<Section> {
    Section::ALL.into_iter().find(|s| s.id().eq_ignore_ascii_case(id.trim()))
}

/// The window opens by itself on the first launch only; this marker in the data folder remembers it did.
pub const SEEN_MARKER: &str = ".window-seen";

/// The first launch: true once, then never again.
pub fn first_launch(data_dir: &Path) -> bool {
    let marker = data_dir.join(SEEN_MARKER);
    if marker.exists() {
        return false;
    }
    let _ = std::fs::create_dir_all(data_dir);
    std::fs::write(&marker, "1\n").is_ok()
}

/// The full name in a passwd line's comment field ("Saachi Badal,,," → "Saachi Badal") for `user`.
pub fn gecos_name(passwd: &str, user: &str) -> Option<String> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        if fields.next()? != user {
            return None;
        }
        let gecos = fields.nth(3)?;
        let name = gecos.split(',').next().unwrap_or("").trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

/// The account's full name (from /etc/passwd), else the login name.
pub fn account_name(env: &dyn Fn(&str) -> Option<String>, passwd: &str) -> String {
    let user = env("USER").or_else(|| env("LOGNAME")).unwrap_or_default();
    if user.is_empty() {
        return String::new();
    }
    gecos_name(passwd, &user).unwrap_or(user)
}

/// What the never-record list stores for an installed app: its desktop id without ".desktop"
/// ("org.gnome.Nautilus.desktop" → "org.gnome.Nautilus"), the same id the recorder gets from the app's
/// systemd scope (apps.rs).
pub fn desktop_app_id(desktop_file_id: &str) -> String {
    desktop_file_id.strip_suffix(".desktop").unwrap_or(desktop_file_id).to_string()
}

/// The app chooser's search: every word typed is in the name or the id.
pub fn app_matches(query: &str, name: &str, id: &str) -> bool {
    let hay = format!("{} {}", name.to_lowercase(), id.to_lowercase());
    query.split_whitespace().all(|w| hay.contains(&w.to_lowercase()))
}

/// How a never-record entry is shown: the app's name when it's installed, else the entry itself.
pub fn excluded_name(entry: &str, installed: &[(String, String)]) -> String {
    installed.iter().find(|(id, _)| id.eq_ignore_ascii_case(entry)).map_or_else(|| entry.to_string(), |(_, name)| name.clone())
}

/// "Alt+C" → "<Alt>c", "Ctrl+F5" → "<Ctrl>F5", for a GTK keycap (GDK key names: lower-case letters, "F5").
pub fn accelerator(keys: &str) -> String {
    let parts: Vec<&str> = keys.split('+').map(str::trim).collect();
    let Some((key, mods)) = parts.split_last() else { return String::new() };
    let mods: String = mods.iter().map(|m| format!("<{m}>")).collect();
    let key = if key.chars().count() == 1 { key.to_lowercase() } else { key.to_string() };
    format!("{mods}{key}")
}

/// A GDK key name ("c", "C", "7", "F5", "KP_7") as a shortcut's key. None: not one a shortcut can use (modifiers,
/// Esc, punctuation…).
pub fn key_from_name(name: &str) -> Option<Key> {
    let name = name.strip_prefix("KP_").filter(|n| n.len() == 1 && n.chars().all(|c| c.is_ascii_digit())).unwrap_or(name);
    Key::parse(name)
}

/// One key press while Settings → Shortcuts listens: `name` is the key's GDK name (the unshifted one when
/// Shift is held, e.g. "1" rather than "exclam"); `ctrl` / `alt` / `shift` / `logo`: the modifiers held.
pub fn shortcut_press(name: &str, ctrl: bool, alt: bool, shift: bool, logo: bool) -> Pressed {
    shortcuts::pressed(ctrl, alt, shift, logo, key_from_name(name), name == "Escape")
}

/// "Tuesday 20 October" / "Tuesday, October 20": as the Timeline writes days (`formats`, set from the locale).
pub fn date_line(day: chrono::NaiveDate) -> String {
    context_capsule_core::formats::date(day, false)
}

/// For a content area `width` pixels wide (next to the sidebar, like the Mac's detail area): the widest the page
/// may be, and whether Home has a side column.
pub fn layout(width: f64) -> (i32, bool) {
    let (_, wide) = content_width(width.max(0.0));
    (if wide { MAX_CONTENT_WIDE } else { MAX_CONTENT } as i32, wide)
}

/// A capsule on Home: its project's name (else how it's named), and the day or "Whole project" on the right.
pub fn recent_capsule(c: &CapsuleSummary, projects: &[Project], today: chrono::NaiveDate) -> (String, String) {
    let label = c.label(today);
    if c.project.is_empty() {
        return (label, c.when(today));
    }
    let side = if c.kind == "daily" || c.kind == "project" { label } else { c.when(today) };
    (project_name(projects, &c.project), side)
}

/// config.json "remind_at": the time when on, `false` when off.
pub fn remind_value(on: bool, time: &str) -> serde_json::Value {
    if on {
        serde_json::Value::String(time.to_string())
    } else {
        serde_json::Value::Bool(false)
    }
}

/// config.json "voice_locale": "" (the computer's language) is `null`.
pub fn language_value(id: &str) -> Option<serde_json::Value> {
    (!id.is_empty()).then(|| serde_json::Value::String(id.to_string()))
}

/// The words above the name field of "New project".
pub fn new_project_body(clue: Option<&str>) -> String {
    match clue {
        Some(clue) => format!("Windows with “{clue}” will count as this project from now on."),
        None => "Name the project. Pick it under Working on while you work on it.".into(),
    }
}

/// The voice model download, as the Settings row says it.
pub fn download_line(done: u64, total: u64) -> String {
    let pct = (done.min(total) * 100).checked_div(total).unwrap_or(0);
    format!("Downloading… {pct}%")
}

pub const GET_VOICE_TITLE: &str = "Get voice notes?";
pub const GET_VOICE_BODY: &str =
    "Context Capsule downloads a speech model once (about 150 MB, from Hugging Face). Speech then becomes text on this computer. Audio is never saved.";
pub const DOWNLOAD: &str = "Download";
pub const NOT_NOW: &str = "Not now";
pub const DOWNLOAD_FAILED: &str = "The download didn't work. Check the internet and try again.";
pub const CHOOSE_APP: &str = "Choose an app";
pub const SEARCH_APPS: &str = "Search apps";
pub const NEW_PROJECT_TITLE: &str = "New project";
pub const ADD: &str = "Add";
pub const EDIT_PROJECT: &str = "Edit project";
pub const SHOW_IN_FOLDER: &str = "Show in folder";
pub const SAVED_COPY: &str = "Saved.";
pub const CANT_OPEN: &str = "Couldn't open it.";
pub const CANT_SAVE: &str = "Couldn't save that. Try again.";
/// Settings' group names that the Mac has and core doesn't (Screens: when you start; Look: light or dark).
pub const SCREENS: &str = "Screens";
pub const LOOK: &str = "Look";
pub const CANT_OPEN_CLAUDE: &str = "Couldn't open Claude";

/// What to say when Claude couldn't be opened: type the request yourself.
pub fn cant_open_claude_body(prompt: &str) -> String {
    format!("Open Claude (Claude Code, or claude.ai in your browser), start a new chat and type:\n\n{prompt}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_flag() {
        assert_eq!(window_only_arg(&args(&["cc"])), None);
        assert_eq!(window_only_arg(&args(&["cc", "--window-only"])), Some(Section::Home));
        assert_eq!(window_only_arg(&args(&["cc", "--window-only=Settings"])), Some(Section::Settings));
        assert_eq!(window_only_arg(&args(&["cc", "--window-only=nope"])), Some(Section::Home));
        assert_eq!(section_from_id("capsules"), Some(Section::Capsules));
        assert_eq!(parse_size("1100x760"), Some((1100, 760)));
        assert_eq!(parse_size("0x760"), None);
        assert_eq!(parse_size("big"), None);
        assert!(new_window(&args(&["cc"]), b"{}"), "on by default");
        assert!(new_window(&args(&["cc"]), b""));
        assert!(new_window(&args(&["cc"]), br#"{"new_window": "yes"}"#));
        assert!(new_window(&args(&["cc"]), b"not json"));
        assert!(!new_window(&args(&["cc"]), br#"{"new_window": false}"#), "the way back");
        assert!(!new_window(&args(&["cc", "--old-window"]), b"{}"));
        assert!(new_window(&args(&["cc", "--new-window"]), br#"{"new_window": false}"#));
    }

    #[test]
    fn seen_once() {
        let dir = std::env::temp_dir().join(format!("cc-seen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(first_launch(&dir));
        assert!(!first_launch(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn names() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\nsaachi:x:1000:1000:Saachi Badal,,,:/home/saachi:/bin/bash\nbob:x:1001:1001::/home/bob:/bin/sh\n";
        assert_eq!(gecos_name(passwd, "saachi").as_deref(), Some("Saachi Badal"));
        assert_eq!(gecos_name(passwd, "bob"), None);
        let env = |user: &'static str| move |k: &str| (k == "USER").then(|| user.to_string());
        assert_eq!(account_name(&env("saachi"), passwd), "Saachi Badal");
        assert_eq!(account_name(&env("bob"), passwd), "bob");
        assert_eq!(account_name(&|_| None, passwd), "");
    }

    #[test]
    fn apps() {
        assert_eq!(desktop_app_id("org.gnome.Nautilus.desktop"), "org.gnome.Nautilus");
        assert_eq!(desktop_app_id("firefox"), "firefox");
        assert!(app_matches("fire", "Firefox", "firefox"));
        assert!(app_matches("  ", "Firefox", "firefox"));
        assert!(app_matches("gnome files", "Files", "org.gnome.Nautilus"));
        assert!(!app_matches("slack", "Firefox", "firefox"));
        let installed = vec![("org.gnome.Nautilus".to_string(), "Files".to_string())];
        assert_eq!(excluded_name("org.gnome.nautilus", &installed), "Files");
        assert_eq!(excluded_name("zoom", &installed), "zoom");
    }

    #[test]
    fn small_words() {
        assert_eq!(accelerator("Alt+C"), "<Alt>c");
        assert_eq!(accelerator("Ctrl+Shift+V"), "<Ctrl><Shift>v");
        assert_eq!(accelerator("Ctrl+F5"), "<Ctrl>F5");
        assert_eq!(accelerator("F12"), "F12");
        assert_eq!(accelerator("Alt+7"), "<Alt>7");
        assert_eq!(date_line(chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()), "Tuesday 20 October");
        assert_eq!(layout(500.0), (760, false));
        assert_eq!(layout(860.0), (760, false));
        assert_eq!(layout(899.0), (980, true), "a 1100 window: two columns");
        assert_eq!(remind_value(false, "18:00"), serde_json::Value::Bool(false));
        assert_eq!(remind_value(true, "19:30"), serde_json::Value::String("19:30".into()));
        assert_eq!(language_value(""), None);
        assert_eq!(language_value("de-DE"), Some("de-DE".into()));
        assert!(new_project_body(Some("acme.com")).contains("“acme.com”"));
        assert!(new_project_body(None).contains("Working on"));
        assert_eq!(download_line(50, 200), "Downloading… 25%");
        assert_eq!(download_line(5, 0), "Downloading… 0%");
        assert!(cant_open_claude_body("Hi").ends_with("\n\nHi"));
    }

    #[test]
    fn shortcut_keys() {
        use context_capsule_core::Shortcut;
        assert_eq!(key_from_name("c"), Some(Key::Letter('C')));
        assert_eq!(key_from_name("C"), Some(Key::Letter('C')), "with Shift held GDK names the capital");
        assert_eq!(key_from_name("7"), Some(Key::Digit(7)));
        assert_eq!(key_from_name("KP_7"), Some(Key::Digit(7)), "the number pad too");
        assert_eq!(key_from_name("F1"), Some(Key::F(1)));
        assert_eq!(key_from_name("F12"), Some(Key::F(12)));
        for no in ["F13", "Alt_L", "Control_R", "Shift_L", "Super_L", "ISO_Level3_Shift", "Escape", "exclam", "space", "Tab", "KP_Enter", "KP_10"] {
            assert_eq!(key_from_name(no), None, "{no}");
        }
        let s = |t: &str| Shortcut::parse(t).unwrap();
        assert_eq!(shortcut_press("Escape", false, false, false, false), Pressed::Cancel);
        assert_eq!(shortcut_press("Alt_L", false, false, false, false), Pressed::Wait, "a modifier alone: keep listening");
        assert_eq!(shortcut_press("Control_L", false, true, false, false), Pressed::Wait);
        assert_eq!(shortcut_press("r", true, false, true, false), Pressed::Done(s("Ctrl+Shift+R")));
        assert_eq!(shortcut_press("c", false, true, false, false), Pressed::Done(Shortcut::RECORD));
        assert_eq!(shortcut_press("F9", false, false, false, false), Pressed::Done(s("F9")));
        assert_eq!(shortcut_press("k", false, false, false, true), Pressed::Done(s("Cmd+K")), "Super: refused later (Add Ctrl or Alt)");
    }

    #[test]
    fn recent_rows() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let projects = vec![Project { id: "web".into(), name: "Website".into(), clues: vec![] }];
        let cap = |project: &str, kind: &str, date: &str| CapsuleSummary {
            title: "Notes".into(),
            project: project.into(),
            kind: kind.into(),
            date: date.into(),
            created: "2026-10-19T18:00:00Z".into(),
            ..Default::default()
        };
        assert_eq!(recent_capsule(&cap("web", "daily", "2026-10-19"), &projects, today), ("Website".into(), "Yesterday".into()));
        assert_eq!(recent_capsule(&cap("web", "project", ""), &projects, today), ("Website".into(), "Whole project".into()));
        assert_eq!(recent_capsule(&cap("web", "", ""), &projects, today), ("Website".into(), "Yesterday".into()));
        assert_eq!(recent_capsule(&cap("", "", ""), &projects, today), ("Notes".into(), "Yesterday".into()));
    }
}
