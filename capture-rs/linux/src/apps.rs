// context-capsule — linux: which app a process is — its desktop-file id (from the systemd scope),
// else its executable name — so the never-record list matches (signal, keepassxc, bitwarden…).

use std::path::Path;

/// The app behind a process id, as /proc tells it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcApp {
    /// The .desktop id from the cgroup scope, e.g. "org.gnome.Nautilus" or "org.signal.Signal".
    pub desktop_id: Option<String>,
    /// The executable's file name, e.g. "keepassxc".
    pub exe: Option<String>,
    /// /proc/<pid>/comm (at most 15 bytes).
    pub comm: Option<String>,
}

impl ProcApp {
    pub fn of(pid: u32) -> ProcApp {
        if pid == 0 {
            return ProcApp::default();
        }
        let base = Path::new("/proc").join(pid.to_string());
        let cgroup = std::fs::read_to_string(base.join("cgroup")).unwrap_or_default();
        let exe = std::fs::read_link(base.join("exe"))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().trim_end_matches(" (deleted)").to_string()));
        let comm = std::fs::read_to_string(base.join("comm")).ok().map(|s| s.trim().to_string());
        ProcApp { desktop_id: desktop_id_from_cgroup(&cgroup), exe, comm: comm.filter(|c| !c.is_empty()) }
    }

    /// Every name this app goes by, best first.
    pub fn ids(&self) -> Vec<&str> {
        [&self.desktop_id, &self.exe, &self.comm].into_iter().flatten().map(String::as_str).collect()
    }

    /// The app id for events and the never-record check: the desktop id, else the exe name. If any
    /// other name of the app is never-record, that one, so the engine skips it too.
    pub fn app_id(&self, is_excluded: &dyn Fn(Option<&str>, &str) -> bool, name: &str) -> Option<String> {
        let ids = self.ids();
        if let Some(private) = ids.iter().find(|id| is_excluded(Some(id), name)) {
            return Some(private.to_string());
        }
        ids.first().map(|s| s.to_string())
    }
}

/// The desktop-file id systemd puts in an app's scope name:
/// `app-gnome-org.gnome.Nautilus-1234.scope`, `app-flatpak-org.signal.Signal-5678.scope`,
/// `app-org.kde.dolphin@0a1b.service`, `app-firefox-1234.scope`. None outside an app scope.
pub fn desktop_id_from_cgroup(cgroup: &str) -> Option<String> {
    let unit = cgroup.lines().filter_map(|l| l.rsplit('/').find(|seg| seg.starts_with("app-"))).next_back()?;
    let unit = unit.strip_suffix(".scope").or_else(|| unit.strip_suffix(".service")).unwrap_or(unit);
    // systemd escapes "-" inside names as "\x2d".
    let unescaped = unit.strip_prefix("app-")?.replace("\\x2d", "-");
    let mut rest = unescaped.as_str();
    // An optional launcher prefix ("gnome-", "flatpak-", "kde-"…) comes before an id with dots.
    for launcher in ["gnome-", "flatpak-", "kde-", "dbus-:1.2-", "snap-"] {
        if let Some(r) = rest.strip_prefix(launcher) {
            rest = r;
            break;
        }
    }
    // Drop the instance: "@xyz" for services, "-<digits or hex>" for scopes.
    let rest = rest.split('@').next().unwrap_or(rest);
    let id = match rest.rsplit_once('-') {
        Some((head, tail)) if !head.is_empty() && tail.chars().all(|c| c.is_ascii_hexdigit()) => head,
        _ => rest,
    };
    (!id.is_empty() && id != "gnome-session-manager").then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use context_capsule_core::Exclusion;

    #[test]
    fn desktop_ids() {
        let gnome =
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-gnome-org.gnome.Nautilus-4242.scope";
        assert_eq!(desktop_id_from_cgroup(gnome).as_deref(), Some("org.gnome.Nautilus"));
        let flatpak =
            "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-flatpak-org.signal.Signal-91.scope";
        assert_eq!(desktop_id_from_cgroup(flatpak).as_deref(), Some("org.signal.Signal"));
        let kde = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-org.kde.dolphin@a1b2c3.service";
        assert_eq!(desktop_id_from_cgroup(kde).as_deref(), Some("org.kde.dolphin"));
        let plain = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-firefox-1234.scope";
        assert_eq!(desktop_id_from_cgroup(plain).as_deref(), Some("firefox"));
        let dbus = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-dbus\\x2d:1.2\\x2dorg.gnome.Calculator@0.service";
        assert_eq!(desktop_id_from_cgroup(dbus).as_deref(), Some("org.gnome.Calculator"));
        let escaped = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-gnome-google\\x2dchrome-77.scope";
        assert_eq!(desktop_id_from_cgroup(escaped).as_deref(), Some("google-chrome"));
        assert_eq!(desktop_id_from_cgroup("0::/user.slice/user-1000.slice/session-2.scope"), None);
        assert_eq!(desktop_id_from_cgroup(""), None);
    }

    #[test]
    fn private_name_wins() {
        let app = ProcApp { desktop_id: Some("com.example.Vault".into()), exe: Some("keepassxc".into()), comm: None };
        let is_excluded = |id: Option<&str>, name: &str| Exclusion::is_excluded(id, Some(name), &[]);
        assert_eq!(app.app_id(&is_excluded, "Vault").as_deref(), Some("keepassxc"), "the exe is private");
        let editor = ProcApp {
            desktop_id: Some("org.gnome.TextEditor".into()),
            exe: Some("gnome-text-editor".into()),
            comm: None,
        };
        assert_eq!(editor.app_id(&is_excluded, "Text Editor").as_deref(), Some("org.gnome.TextEditor"));
        assert_eq!(ProcApp::default().app_id(&is_excluded, "x"), None);
    }
}
