// context-capsule — core: the app's name, the window's pages and size, and the few words the Windows and Linux
// recorders' own dialogs, tray and menus use. The window itself is the shared page (`ui/`); see `bridge`.

/// The app's name where people see it as a name: window title, taskbar, Start menu, Settings → Apps, dialog titles,
/// the tray's tooltip, notifications. In sentences it stays "Context Capsule".
pub const APP_NAME: &str = "Karya Context Capsule";
/// The tray / menu item that opens the window.
pub const OPEN_WINDOW: &str = "Open Context Capsule";

/// The window's pages, for opening it on one (`--show help`, first launch on Settings).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    Home,
    Timeline,
    Capsules,
    Projects,
    Settings,
    Help,
}

impl Section {
    pub const ALL: [Section; 6] = [Section::Home, Section::Timeline, Section::Capsules, Section::Projects, Section::Settings, Section::Help];

    pub fn title(self) -> &'static str {
        match self {
            Section::Home => "Home",
            Section::Timeline => "Timeline",
            Section::Capsules => "Capsules",
            Section::Projects => "Projects",
            Section::Settings => "Settings",
            Section::Help => "Help",
        }
    }

    /// A short id ("home"): the shared page's name for it.
    pub fn id(self) -> &'static str {
        match self {
            Section::Home => "home",
            Section::Timeline => "timeline",
            Section::Capsules => "capsules",
            Section::Projects => "projects",
            Section::Settings => "settings",
            Section::Help => "help",
        }
    }
}

/// Window sizes, in device-independent pixels.
pub const MIN_WIDTH: f64 = 640.0;
pub const MIN_HEIGHT: f64 = 480.0;
pub const START_WIDTH: f64 = 900.0;
pub const START_HEIGHT: f64 = 640.0;

/// The tray / menu item that makes the day's capsules.
pub const MAKE_TITLE: &str = "Make today's capsules";
pub const MAKE_TODAYS: &str = MAKE_TITLE;

/// The file dialogs' buttons.
pub const OPEN: &str = "Open";
pub const SAVE_COPY: &str = "Save a copy…";
pub const OPEN_CAPSULE: &str = "Open a capsule…";
pub const CANCEL: &str = "Cancel";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages() {
        assert_eq!(Section::ALL.map(|s| s.title()), ["Home", "Timeline", "Capsules", "Projects", "Settings", "Help"]);
        assert_eq!(Section::ALL.map(|s| s.id()), ["home", "timeline", "capsules", "projects", "settings", "help"]);
    }
}
