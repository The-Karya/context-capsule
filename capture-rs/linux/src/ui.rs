// context-capsule — linux: the window's small decisions that need no GTK — the `--window-only` flag, the
// first-launch marker, the account's full name, app ids for the never-record list, and the words of its native
// dialogs. The page's words are shared (ui/, core/src/window.rs).
#![cfg_attr(not(all(feature = "window", feature = "voice")), allow(dead_code))]

use context_capsule_core::window::Section;
use std::path::Path;

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

/// The words above the name field of "New project".
pub fn new_project_body(clue: Option<&str>) -> String {
    match clue {
        Some(clue) => format!("Windows with “{clue}” will count as this project from now on."),
        None => "Name the project. Pick it under Working on while you work on it.".into(),
    }
}

pub const GET_VOICE_TITLE: &str = "Get voice notes?";
pub const GET_VOICE_BODY: &str =
    "Context Capsule downloads a speech model once (about 150 MB, from Hugging Face). Speech then becomes text on this computer. Audio is never saved.";
pub const DOWNLOAD: &str = "Download";
pub const NOT_NOW: &str = "Not now";
pub const CHOOSE_APP: &str = "Choose an app";
pub const SEARCH_APPS: &str = "Search apps";
pub const NEW_PROJECT_TITLE: &str = "New project";
pub const ADD: &str = "Add";
pub const CANT_OPEN: &str = "Couldn't open it.";
pub const CANT_SAVE: &str = "Couldn't save that. Try again.";
pub const CANT_OPEN_CLAUDE: &str = "Couldn't open Claude";

/// Said when the window can't open because WebKitGTK 6.0 isn't installed (the tray and recording still work).
pub const NEEDS_WEBKIT_TITLE: &str = "Install WebKitGTK to open the window";
pub const NEEDS_WEBKIT_BODY: &str = "Context Capsule's window needs WebKitGTK 6.0. Install it with your software app or package manager, then open Context Capsule again.

Debian and Ubuntu: libwebkitgtk-6.0-4
Fedora: webkitgtk6.0
Arch: webkitgtk-6.0
openSUSE: libwebkitgtk-6_0-4

Until then, recording, the tray menu and the shortcuts still work.";
pub const OK: &str = "OK";

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
        assert!(new_project_body(Some("acme.com")).contains("“acme.com”"));
        assert!(new_project_body(None).contains("Working on"));
        assert!(cant_open_claude_body("Hi").ends_with("\n\nHi"));
        for package in ["libwebkitgtk-6.0-4", "webkitgtk6.0", "webkitgtk-6.0"] {
            assert!(NEEDS_WEBKIT_BODY.contains(package), "{package}");
        }
    }
}
