// context-capsule — linux: which installed app an event's app is (for its icon in the shared window), from
// .desktop files: by desktop id, StartupWMClass, the program Exec runs, or the app's name. Plain std, no GTK,
// so it is tested anywhere.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// An installed app as the matcher sees it: desktop id (without ".desktop"), StartupWMClass, the program its Exec
/// line runs (file name only), and its name. Empty: not known.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Installed {
    pub id: String,
    pub wm_class: String,
    pub exec: String,
    pub name: String,
}

/// Which of `apps` is the event's app: `bundle` (the desktop id, else the program's name) against the desktop
/// id, then StartupWMClass, then the program Exec runs; then `app` (the name: WM_CLASS or the accessibility
/// name) against StartupWMClass, the name, the desktop id, the program. Case doesn't matter. Earlier apps win
/// a tie, so callers put the apps shown in menus first.
pub fn best(apps: &[Installed], app: &str, bundle: &str) -> Option<usize> {
    let bundle = bundle.trim();
    let bundle = bundle.strip_suffix(".desktop").unwrap_or(bundle);
    let app = app.trim();
    let same = |a: &str, b: &str| !a.is_empty() && !b.is_empty() && a.eq_ignore_ascii_case(b);
    let find = |want: &str, field: fn(&Installed) -> &str| apps.iter().position(|a| same(field(a), want));
    let by_bundle: [fn(&Installed) -> &str; 3] = [|a| &a.id, |a| &a.wm_class, |a| &a.exec];
    let by_name: [fn(&Installed) -> &str; 4] = [|a| &a.wm_class, |a| &a.name, |a| &a.id, |a| &a.exec];
    by_bundle
        .iter()
        .find_map(|f| find(bundle, *f))
        .or_else(|| by_name.iter().find_map(|f| find(app, *f)))
}

/// The program an Exec line runs, file name only: `env A=b /snap/bin/firefox %u` → "firefox",
/// `"/opt/My App/app" --x` → "app". Empty for none, and for `flatpak` (the desktop id names those apps).
pub fn exec_name(commandline: &str) -> String {
    let mut rest = commandline.trim();
    loop {
        let (word, after) = first_word(rest);
        rest = after;
        if word.is_empty() {
            return String::new();
        }
        let is_assignment = word.contains('=') && !word.starts_with('/');
        let name = word.rsplit('/').next().unwrap_or(&word);
        if name == "env" || is_assignment || (name.starts_with('-') && name.len() > 1) {
            continue;
        }
        return if name == "flatpak" { String::new() } else { name.to_string() };
    }
}

/// The first word of an Exec line (double quotes group, backslash escapes inside them), and what follows.
fn first_word(s: &str) -> (String, &str) {
    let s = s.trim_start();
    let mut word = String::new();
    let mut quoted = false;
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => quoted = !quoted,
            '\\' if quoted => {
                if let Some((_, n)) = chars.next() {
                    word.push(n);
                }
            }
            c if c.is_whitespace() && !quoted => return (word, &s[i..]),
            c => word.push(c),
        }
    }
    (word, "")
}

/// StartupWMClass from a .desktop file's text (its [Desktop Entry] group only).
pub fn startup_wm_class(text: &str) -> Option<String> {
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == "StartupWMClass" {
                let v = v.trim();
                return (!v.is_empty()).then(|| v.to_string());
            }
        }
    }
    None
}

/// Where .desktop files are, most important first: $XDG_DATA_HOME (else ~/.local/share), then $XDG_DATA_DIRS
/// (else /usr/local/share and /usr/share), each with "applications".
pub fn desktop_dirs(data_home: Option<&str>, home: Option<&str>, data_dirs: Option<&str>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    match (data_home.filter(|s| !s.is_empty()), home.filter(|s| !s.is_empty())) {
        (Some(d), _) => roots.push(d.into()),
        (None, Some(h)) => roots.push(Path::new(h).join(".local/share")),
        _ => {}
    }
    let dirs = data_dirs.filter(|s| !s.is_empty()).unwrap_or("/usr/local/share:/usr/share");
    roots.extend(dirs.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    let mut out: Vec<PathBuf> = Vec::new();
    for r in roots {
        let d = r.join("applications");
        if !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

/// The StartupWMClass of every .desktop file in `dirs`, by desktop id (without ".desktop"; files in
/// subfolders are "sub-name", as the desktop-entry spec says). The first file of an id wins.
pub fn wm_classes(dirs: &[PathBuf]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    for d in dirs {
        walk(d, "", 0, &mut |id, path| {
            if seen.insert(id.to_string()) {
                if let Some(c) = std::fs::read_to_string(path).ok().as_deref().and_then(startup_wm_class) {
                    out.insert(id.to_string(), c);
                }
            }
        });
    }
    out
}

fn walk(dir: &Path, prefix: &str, depth: u32, f: &mut dyn FnMut(&str, &Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else { continue };
        if p.is_dir() {
            if depth < 4 {
                walk(&p, &format!("{prefix}{name}-"), depth + 1, f);
            }
        } else if let Some(stem) = name.strip_suffix(".desktop") {
            f(&format!("{prefix}{stem}"), &p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, wm_class: &str, exec: &str, name: &str) -> Installed {
        Installed { id: id.into(), wm_class: wm_class.into(), exec: exec.into(), name: name.into() }
    }

    fn apps() -> Vec<Installed> {
        vec![
            app("org.gnome.Nautilus", "", "nautilus", "Files"),
            app("firefox", "firefox", "firefox", "Firefox"),
            app("org.signal.Signal", "Signal", "", "Signal"),
            app("org.keepassxc.KeePassXC", "keepassxc", "keepassxc", "KeePassXC"),
            app("code", "Code", "code", "Visual Studio Code"),
            app("libreoffice-writer", "libreoffice-writer", "libreoffice", "LibreOffice Writer"),
            app("jetbrains-idea", "jetbrains-idea", "idea.sh", "IntelliJ IDEA"),
        ]
    }

    #[test]
    fn by_desktop_id() {
        let a = apps();
        assert_eq!(best(&a, "Files", "org.gnome.Nautilus"), Some(0));
        assert_eq!(best(&a, "", "ORG.SIGNAL.SIGNAL"), Some(2));
        assert_eq!(best(&a, "", "firefox.desktop"), Some(1));
    }

    #[test]
    fn by_wm_class_then_program() {
        let a = apps();
        assert_eq!(best(&a, "", "keepassxc"), Some(3));
        assert_eq!(best(&a, "", "nautilus"), Some(0));
        assert_eq!(best(&a, "", "idea.sh"), Some(6));
        // The desktop id beats a program of the same name elsewhere.
        assert_eq!(best(&a, "", "code"), Some(4));
    }

    #[test]
    fn by_name_when_the_id_is_unknown() {
        let a = apps();
        assert_eq!(best(&a, "Signal", "signal-desktop"), Some(2));
        assert_eq!(best(&a, "Visual Studio Code", "electron"), Some(4));
        assert_eq!(best(&a, "jetbrains-idea", "java"), Some(6));
        assert_eq!(best(&a, "Nothing", "nothing"), None);
        assert_eq!(best(&a, "", ""), None);
        // Empty fields never match an empty request.
        assert_eq!(best(&[app("x", "", "", "")], "", "y"), None);
    }

    #[test]
    fn programs_from_exec_lines() {
        assert_eq!(exec_name("firefox %u"), "firefox");
        assert_eq!(exec_name("/usr/bin/nautilus --new-window %U"), "nautilus");
        assert_eq!(exec_name("env BAMF_DESKTOP_FILE_HINT=/x.desktop /snap/bin/firefox %u"), "firefox");
        assert_eq!(exec_name("env GDK_BACKEND=x11 code"), "code");
        assert_eq!(exec_name("\"/opt/My App/my-app\" --x"), "my-app");
        assert_eq!(exec_name("/usr/bin/flatpak run --branch=stable org.signal.Signal"), "");
        assert_eq!(exec_name(""), "");
    }

    #[test]
    fn wm_class_from_desktop_files() {
        let text = "[Desktop Entry]\nName=Code\nStartupWMClass = Code \n[Desktop Action new]\nStartupWMClass=nope\n";
        assert_eq!(startup_wm_class(text).as_deref(), Some("Code"));
        assert_eq!(startup_wm_class("[Desktop Action x]\nStartupWMClass=nope\n[Desktop Entry]\nName=A\n"), None);
        assert_eq!(startup_wm_class("[Desktop Entry]\nStartupWMClass=\n"), None);
    }

    #[test]
    fn where_desktop_files_are() {
        let d = desktop_dirs(None, Some("/home/a"), None);
        assert_eq!(d, [PathBuf::from("/home/a/.local/share/applications"), "/usr/local/share/applications".into(), "/usr/share/applications".into()]);
        let d = desktop_dirs(Some("/x"), Some("/home/a"), Some("/var/lib/flatpak/exports/share:/usr/share:/usr/share"));
        assert_eq!(d, [PathBuf::from("/x/applications"), "/var/lib/flatpak/exports/share/applications".into(), "/usr/share/applications".into()]);
    }

    #[test]
    fn wm_classes_from_folders() {
        let root = std::env::temp_dir().join(format!("cc-icon-match-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (a, b) = (root.join("a"), root.join("b"));
        std::fs::create_dir_all(a.join("kde4")).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("code.desktop"), "[Desktop Entry]\nStartupWMClass=Code\n").unwrap();
        std::fs::write(a.join("kde4/kate.desktop"), "[Desktop Entry]\nStartupWMClass=kate\n").unwrap();
        std::fs::write(a.join("plain.desktop"), "[Desktop Entry]\nName=Plain\n").unwrap();
        std::fs::write(b.join("code.desktop"), "[Desktop Entry]\nStartupWMClass=Other\n").unwrap();
        std::fs::write(b.join("plain.desktop"), "[Desktop Entry]\nStartupWMClass=Hidden\n").unwrap();
        let m = wm_classes(&[a, b, root.join("missing")]);
        assert_eq!(m.get("code").map(String::as_str), Some("Code"));
        assert_eq!(m.get("kde4-kate").map(String::as_str), Some("kate"));
        // The first file of an id wins, even without StartupWMClass.
        assert_eq!(m.get("plain"), None);
        assert_eq!(m.len(), 2);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
