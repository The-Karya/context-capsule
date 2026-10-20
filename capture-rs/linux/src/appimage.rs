// context-capsule — linux: running as an AppImage — the file to start (and to update) is $APPIMAGE, not the
// mounted binary. The first time, "Add Context Capsule to your apps?" puts a menu entry (and the icon) in
// ~/.local/share, pointing at the AppImage.

use crate::notify::Notifier;
use std::path::{Path, PathBuf};

pub const ADD_QUESTION: &str = "Add Context Capsule to your apps?";
pub const ADD_BUTTON: &str = "Add";
pub const NOT_NOW: &str = "Not now";
/// The desktop file and icon name (the app id portals know us by).
pub const DESKTOP_FILE: &str = "com.thekarya.contextcapsule.desktop";
pub const ICON_NAME: &str = "com.thekarya.contextcapsule";

/// The AppImage file when we run from one ($APPIMAGE: an absolute path to a file that exists).
pub fn appimage(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let p = PathBuf::from(env("APPIMAGE")?.trim());
    (p.is_absolute() && p.is_file()).then_some(p)
}

/// What to start at login or from the app menu: the AppImage, else this program.
pub fn launcher(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    appimage(env).or_else(|| std::env::current_exe().ok())
}

/// A path for a desktop entry's Exec line: quoted, with `"` `` ` `` `$` `\` escaped and `%` doubled.
pub fn exec_quote(path: &str) -> String {
    let mut out = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '%' => out.push_str("%%"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The app menu entry.
pub fn menu_entry(exe: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Context Capsule\nComment=Turn your work into context for Claude\n\
         Exec={}\nIcon={ICON_NAME}\nTerminal=false\nCategories=Utility;\nX-GNOME-UsesNotifications=true\n",
        exec_quote(exe)
    )
}

/// `$XDG_DATA_HOME` (absolute), else `~/.local/share`.
pub fn data_home(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if let Some(x) = env("XDG_DATA_HOME").filter(|x| x.starts_with('/')) {
        return Some(PathBuf::from(x));
    }
    env("HOME").map(|h| PathBuf::from(h).join(".local/share"))
}

/// Write the menu entry for `exe`, and the icon from the AppImage (`$APPDIR`) when there is one.
pub fn add_to_apps(share: &Path, exe: &Path, appdir: Option<&Path>) -> std::io::Result<()> {
    let apps = share.join("applications");
    std::fs::create_dir_all(&apps)?;
    std::fs::write(apps.join(DESKTOP_FILE), menu_entry(&exe.to_string_lossy()))?;
    if let Some(icon) = appdir.map(|d| d.join(format!("{ICON_NAME}.png"))).filter(|i| i.is_file()) {
        let dir = share.join("icons/hicolor/256x256/apps");
        std::fs::create_dir_all(&dir)?;
        std::fs::copy(icon, dir.join(format!("{ICON_NAME}.png")))?;
    }
    Ok(())
}

/// Our menu entry already there: keep it pointing at this AppImage (it may have been moved).
fn refresh(share: &Path, exe: &Path) -> bool {
    let file = share.join("applications").join(DESKTOP_FILE);
    let Ok(old) = std::fs::read_to_string(&file) else { return false };
    let want = menu_entry(&exe.to_string_lossy());
    if old != want && old.contains(&format!("Icon={ICON_NAME}")) {
        let _ = std::fs::write(&file, want);
    }
    true
}

/// From an AppImage: ask once (a marker in the data folder remembers) to add it to the apps. Blocking (a dialog):
/// call on a thread of its own.
pub fn offer_once(data_dir: &Path, notifier: &Notifier) {
    let env = |k: &str| std::env::var(k).ok();
    let (Some(image), Some(share)) = (appimage(&env), data_home(&env)) else { return };
    if refresh(&share, &image) {
        return;
    }
    let marker = data_dir.join(".apps-asked");
    if marker.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(data_dir);
    let _ = std::fs::write(&marker, "1\n");
    if !notifier.ask("Context Capsule", ADD_QUESTION, ADD_BUTTON, NOT_NOW) {
        eprintln!("Context Capsule: add to apps: not now");
        return;
    }
    let appdir = env("APPDIR").map(PathBuf::from);
    match add_to_apps(&share, &image, appdir.as_deref()) {
        Ok(()) => eprintln!("Context Capsule: added to the apps"),
        Err(e) => eprintln!("Context Capsule: cannot add to the apps: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_and_entry() {
        assert_eq!(exec_quote("/home/s/Apps/Context Capsule.AppImage"), "\"/home/s/Apps/Context Capsule.AppImage\"");
        assert_eq!(exec_quote("/a/$b\"c`d\\e%f"), "\"/a/\\$b\\\"c\\`d\\\\e%%f\"");
        let e = menu_entry("/x/cc.AppImage");
        assert!(e.contains("Exec=\"/x/cc.AppImage\"\n") && e.contains("Icon=com.thekarya.contextcapsule\n"));
        assert!(e.starts_with("[Desktop Entry]\nType=Application\n"));
    }

    #[test]
    fn places_and_detection() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(data_home(&env(&[("HOME", "/home/s")])), Some(PathBuf::from("/home/s/.local/share")));
        assert_eq!(data_home(&env(&[("HOME", "/home/s"), ("XDG_DATA_HOME", "/d")])), Some(PathBuf::from("/d")));
        assert_eq!(appimage(&env(&[])), None);
        assert_eq!(appimage(&env(&[("APPIMAGE", "relative.AppImage")])), None);
        assert_eq!(appimage(&env(&[("APPIMAGE", "/no/such/file.AppImage")])), None);
    }

    #[test]
    fn adds_and_refreshes() {
        let tmp = std::env::temp_dir().join(format!("cc-appimage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let (share, appdir) = (tmp.join("share"), tmp.join("appdir"));
        std::fs::create_dir_all(&appdir).unwrap();
        std::fs::write(appdir.join("com.thekarya.contextcapsule.png"), b"png").unwrap();
        add_to_apps(&share, Path::new("/a/one.AppImage"), Some(&appdir)).unwrap();
        let entry = share.join("applications").join(DESKTOP_FILE);
        assert!(std::fs::read_to_string(&entry).unwrap().contains("/a/one.AppImage"));
        assert!(share.join("icons/hicolor/256x256/apps/com.thekarya.contextcapsule.png").is_file());
        assert!(refresh(&share, Path::new("/b/moved.AppImage")));
        assert!(std::fs::read_to_string(&entry).unwrap().contains("/b/moved.AppImage"), "follows the AppImage");
        assert!(!refresh(&tmp.join("none"), Path::new("/a")), "no entry yet");
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
