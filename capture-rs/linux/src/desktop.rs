// context-capsule — linux: which session this is (Wayland or X11) and whether a window overview is
// open — GNOME Activities (org.gnome.Shell OverviewActive) or a KWin overview effect.

use zbus::Connection;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKind {
    Wayland,
    X11,
    /// Neither (a console, or a test container without a display).
    Unknown,
}

impl SessionKind {
    pub fn detect(env: &dyn Fn(&str) -> Option<String>) -> SessionKind {
        let set = |k: &str| env(k).is_some_and(|v| !v.trim().is_empty());
        match env("XDG_SESSION_TYPE").unwrap_or_default().trim() {
            "wayland" => return SessionKind::Wayland,
            "x11" => return SessionKind::X11,
            _ => {}
        }
        if set("WAYLAND_DISPLAY") {
            SessionKind::Wayland
        } else if set("DISPLAY") {
            SessionKind::X11
        } else {
            SessionKind::Unknown
        }
    }
}

/// KWin effects that show every window at once.
pub fn is_overview_effect(effect: &str) -> bool {
    matches!(effect, "overview" | "windowview" | "presentwindows" | "desktopgrid" | "expo" | "cube")
}

/// Asks the shell over the session bus.
pub struct Overview {
    bus: Connection,
}

impl Overview {
    pub async fn connect() -> Option<Overview> {
        Connection::session().await.ok().map(|bus| Overview { bus })
    }

    /// True while GNOME Activities or a KDE overview is open. False when neither shell answers.
    pub async fn is_open(&self) -> bool {
        if let Ok(p) = zbus::Proxy::new(&self.bus, "org.gnome.Shell", "/org/gnome/Shell", "org.gnome.Shell").await {
            if let Ok(open) = p.get_property::<bool>("OverviewActive").await {
                return open;
            }
        }
        if let Ok(p) = zbus::Proxy::new(&self.bus, "org.kde.KWin", "/Effects", "org.kde.kwin.Effects").await {
            if let Ok(active) = p.get_property::<Vec<String>>("activeEffects").await {
                return active.iter().any(|e| is_overview_effect(e));
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn session_kind() {
        assert_eq!(
            SessionKind::detect(&env(&[("XDG_SESSION_TYPE", "wayland"), ("DISPLAY", ":0")])),
            SessionKind::Wayland
        );
        assert_eq!(SessionKind::detect(&env(&[("XDG_SESSION_TYPE", "x11")])), SessionKind::X11);
        assert_eq!(
            SessionKind::detect(&env(&[("WAYLAND_DISPLAY", "wayland-0"), ("DISPLAY", ":0")])),
            SessionKind::Wayland,
            "Xwayland is not X11"
        );
        assert_eq!(SessionKind::detect(&env(&[("DISPLAY", ":99")])), SessionKind::X11);
        assert_eq!(SessionKind::detect(&env(&[])), SessionKind::Unknown);
    }

    #[test]
    fn kwin_effects() {
        assert!(is_overview_effect("overview"));
        assert!(is_overview_effect("windowview"));
        assert!(!is_overview_effect("blur"));
    }
}
