// context-capsule — linux: open at login by default (an XDG autostart entry), once — if the user removes it
// (or turns it off in their desktop's startup settings), it stays off. From an AppImage it starts $APPIMAGE.

use std::path::{Path, PathBuf};

/// The autostart entry that starts `exe` at login.
pub fn desktop_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Context Capsule\nComment=Turn your work into context for Claude\n\
         Exec={}\nIcon=media-record\nX-GNOME-Autostart-enabled=true\nNoDisplay=false\n",
        crate::appimage::exec_quote(exe)
    )
}

/// `$XDG_CONFIG_HOME/autostart` (else `~/.config/autostart`).
pub fn autostart_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if let Some(x) = env("XDG_CONFIG_HOME").filter(|x| x.starts_with('/')) {
        return Some(PathBuf::from(x).join("autostart"));
    }
    env("HOME").map(|h| PathBuf::from(h).join(".config/autostart"))
}

/// Add the entry the first time the app runs; a marker in the data folder remembers it was done.
pub fn install_once(data_dir: &Path, env: &dyn Fn(&str) -> Option<String>) -> Option<&'static str> {
    let marker = data_dir.join(".login-set");
    if marker.exists() {
        return None;
    }
    let _ = std::fs::create_dir_all(data_dir);
    let _ = std::fs::write(&marker, "1\n");
    let dir = autostart_dir(env)?;
    // From an AppImage: the AppImage file (the program inside is gone once it quits).
    let exe = crate::appimage::launcher(env)?;
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("com.thekarya.contextcapsule.desktop"), desktop_entry(&exe.to_string_lossy())).ok()?;
    Some("open at login: on (first launch)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_and_folder() {
        let e = desktop_entry("/opt/cc/context-capsule");
        assert!(e.contains("Exec=\"/opt/cc/context-capsule\"") && e.contains("Type=Application"));
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
        std::fs::remove_file(home.join(".config/autostart/com.thekarya.contextcapsule.desktop")).unwrap();
        assert!(install_once(&data, &env).is_none()); // removed by the user: stays off
        assert!(!home.join(".config/autostart/com.thekarya.contextcapsule.desktop").exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
