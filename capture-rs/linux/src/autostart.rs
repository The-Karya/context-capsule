// context-capsule — linux: open at login by default (an XDG autostart entry), once — if the user removes it
// (or turns it off in their desktop's startup settings), it stays off. From an AppImage it starts $APPIMAGE.

use std::path::{Path, PathBuf};

/// The autostart entry that starts `exe` at login.
pub fn desktop_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName={}\nComment=Turn your work into context for Claude\n\
         Exec={}\nIcon={}\nX-GNOME-Autostart-enabled=true\nNoDisplay=false\n",
        context_capsule_core::window::APP_NAME,
        crate::appimage::exec_quote(exe),
        crate::appimage::ICON_NAME
    )
}

/// An entry written by 0.2.0 or earlier ("Context Capsule", the media-record icon) with today's name and icon;
/// None when there's nothing to change. Only those two lines change: on/off and Exec stay as they are.
pub fn renamed(old: &str) -> Option<String> {
    let mut changed = false;
    let lines: Vec<String> = old
        .split('\n')
        .map(|line| match line.trim() {
            "Name=Context Capsule" => {
                changed = true;
                format!("Name={}", context_capsule_core::window::APP_NAME)
            }
            "Icon=media-record" => {
                changed = true;
                format!("Icon={}", crate::appimage::ICON_NAME)
            }
            _ => line.to_string(),
        })
        .collect();
    changed.then(|| lines.join("\n"))
}

/// Bring an older autostart entry up to date (same file: never a second entry). Missing: nothing to do.
pub fn refresh(env: &dyn Fn(&str) -> Option<String>) {
    let Some(file) = autostart_dir(env).map(|d| d.join(FILE)) else { return };
    if let Some(new) = std::fs::read_to_string(&file).ok().and_then(|old| renamed(&old)) {
        let _ = std::fs::write(&file, new);
    }
}

/// `$XDG_CONFIG_HOME/autostart` (else `~/.config/autostart`).
pub fn autostart_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if let Some(x) = env("XDG_CONFIG_HOME").filter(|x| x.starts_with('/')) {
        return Some(PathBuf::from(x).join("autostart"));
    }
    env("HOME").map(|h| PathBuf::from(h).join(".config/autostart"))
}

/// The entry's file name.
pub const FILE: &str = "com.thekarya.contextcapsule.desktop";

/// Add the entry the first time the app runs; a marker in the data folder remembers it was done.
pub fn install_once(data_dir: &Path, env: &dyn Fn(&str) -> Option<String>) -> Option<&'static str> {
    let marker = data_dir.join(".login-set");
    if marker.exists() {
        return None;
    }
    let _ = std::fs::create_dir_all(data_dir);
    let _ = std::fs::write(&marker, "1\n");
    enable(env).ok()?;
    Some("open at login: on (first launch)")
}

/// Write the entry (Settings → Open at login → Turn On).
pub fn enable(env: &dyn Fn(&str) -> Option<String>) -> std::io::Result<()> {
    let dir = autostart_dir(env).ok_or_else(|| std::io::Error::other("no home folder"))?;
    // From an AppImage: the AppImage file (the program inside is gone once it quits).
    let exe = crate::appimage::launcher(env).ok_or_else(|| std::io::Error::other("no program path"))?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(FILE), desktop_entry(&exe.to_string_lossy()))
}

/// The entry is there and not turned off (`Hidden=true`, or `X-GNOME-Autostart-enabled=false`).
#[cfg_attr(not(feature = "window"), allow(dead_code))]
pub fn is_enabled(env: &dyn Fn(&str) -> Option<String>) -> bool {
    autostart_dir(env).and_then(|d| std::fs::read_to_string(d.join(FILE)).ok()).is_some_and(|text| entry_enabled(&text))
}

/// Whether a desktop entry's text says it starts (the `[Desktop Entry]` group only).
#[cfg_attr(not(feature = "window"), allow(dead_code))]
pub fn entry_enabled(text: &str) -> bool {
    let mut in_entry = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim().to_ascii_lowercase());
        if in_entry && ((key == "Hidden" && value == "true") || (key == "X-GNOME-Autostart-enabled" && value == "false")) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_and_folder() {
        let e = desktop_entry("/opt/cc/context-capsule");
        assert!(e.contains("Exec=\"/opt/cc/context-capsule\"") && e.contains("Type=Application"));
        assert!(e.contains("\nName=Karya Context Capsule\n") && e.contains("\nIcon=com.thekarya.contextcapsule\n"));
        let env = |k: &str| match k {
            "HOME" => Some("/home/s".to_string()),
            _ => None,
        };
        assert_eq!(autostart_dir(&env), Some(PathBuf::from("/home/s/.config/autostart")));
        let xdg = |k: &str| match k {
            "XDG_CONFIG_HOME" => Some("/cfg".to_string()),
            _ => None,
        };
        assert_eq!(autostart_dir(&xdg), Some(PathBuf::from("/cfg/autostart")));
    }

    #[test]
    fn only_once() {
        let tmp = std::env::temp_dir().join(format!("cc-autostart-{}", std::process::id()));
        let home = tmp.join("home");
        let data = tmp.join("data");
        let env = |k: &str| (k == "HOME").then(|| home.to_string_lossy().into_owned());
        assert!(install_once(&data, &env).is_some());
        assert!(home.join(".config/autostart/com.thekarya.contextcapsule.desktop").exists());
        assert!(is_enabled(&env));
        std::fs::remove_file(home.join(".config/autostart/com.thekarya.contextcapsule.desktop")).unwrap();
        assert!(install_once(&data, &env).is_none()); // removed by the user: stays off
        assert!(!home.join(".config/autostart/com.thekarya.contextcapsule.desktop").exists());
        assert!(!is_enabled(&env));
        enable(&env).unwrap();
        assert!(is_enabled(&env), "Turn On");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn old_entries_renamed() {
        let old = "[Desktop Entry]\nType=Application\nName=Context Capsule\nComment=Turn your work into context for Claude\n\
                   Exec=\"/x\"\nIcon=media-record\nX-GNOME-Autostart-enabled=false\nNoDisplay=false\n";
        let new = renamed(old).unwrap();
        assert!(new.contains("\nName=Karya Context Capsule\n") && new.contains("\nIcon=com.thekarya.contextcapsule\n"));
        assert!(new.contains("X-GNOME-Autostart-enabled=false"), "turned off stays off");
        assert!(!entry_enabled(&new));
        assert_eq!(renamed(&new), None, "already new");
        assert_eq!(renamed(&desktop_entry("/x")), None);

        let tmp = std::env::temp_dir().join(format!("cc-autostart-rename-{}", std::process::id()));
        let env = |k: &str| (k == "HOME").then(|| tmp.to_string_lossy().into_owned());
        refresh(&env); // no entry: nothing written
        let dir = tmp.join(".config/autostart");
        assert!(!dir.join(FILE).exists());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), old).unwrap();
        refresh(&env);
        assert_eq!(std::fs::read_to_string(dir.join(FILE)).unwrap(), new);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "one entry, same file");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn turned_off_entries() {
        assert!(entry_enabled(&desktop_entry("/x")));
        assert!(!entry_enabled("[Desktop Entry]\nExec=x\nHidden=true\n"));
        assert!(!entry_enabled("[Desktop Entry]\nX-GNOME-Autostart-enabled = False\n"));
        assert!(entry_enabled("[Desktop Entry]\nExec=x\n[Desktop Action a]\nHidden=true\n"), "other groups don't count");
    }
}
