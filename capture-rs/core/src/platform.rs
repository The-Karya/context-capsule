// context-capsule — core: the one seam between the engine and the OS. Windows and Linux implement `Platform`.
//! The engine is single-threaded: the platform runs it on its own thread (see `Engine::run`) and
//! feeds it [`Msg`]s from hotkeys, the tray menu and the capture picker. Every method below is called
//! on that thread, so implementations may hop to their UI thread as they need.

use crate::engine::empty_session_alert;
use crate::image::Image;
use crate::membership::{ListedWindow, PickedContent, Skip};
use crate::projects::Project;
use crate::voice::PlatformText;
use std::sync::Arc;
use std::time::SystemTime;

/// Most screens or windows shared at once (each picked by the user, one capture each).
pub const MAX_SOURCES: usize = 4;

/// The latest captured frame of the picked content, BGRA, top-left origin. Memory only.
#[derive(Clone, Debug)]
pub struct Frame {
    pub bgra: Arc<[u8]>,
    pub width: usize,
    pub height: usize,
    /// Bytes per row (at least `width * 4`).
    pub stride: usize,
    /// When it arrived. Frames only come when the content changes (see `Freshness`).
    pub arrived: SystemTime,
}

/// The frontmost app and its windows, as the OS reports them right now.
#[derive(Clone, Debug, Default)]
pub struct Front {
    pub app_name: String,
    /// Stable app id: the exe name or AUMID on Windows, the desktop-file / WM_CLASS id on Linux.
    pub app_id: Option<String>,
    pub pid: u32,
    /// The app's front window, if it has a normal one on screen (usually `WindowList::front(pid, &list)`).
    pub window: Option<ListedWindow>,
    /// Every on-screen window, front to back, with never-record apps marked `excluded`.
    pub list: Vec<ListedWindow>,
    /// A window overview (Task View, GNOME Activities, KDE Overview…) shows every window: read nothing.
    pub overlay: bool,
    /// Set by the engine on the Front it gets from `Platform::pointer_front`: `window` is the one under the
    /// mouse, not the one with keyboard focus. Read that window itself (never the focused one); if it can't
    /// be read on its own, give no accessibility text (the engine then uses OCR of its frame).
    pub pointer: bool,
}

/// How the capture picker (or the capture it started) ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickOutcome {
    /// The user chose content and frames are flowing.
    Picked,
    /// The user closed the picker.
    Cancelled,
    /// The picker or the capture could not start. Prefix the message: "picker: …" or "capture: …".
    Failed(String),
    /// Every capture ended on its own (None: the user stopped sharing from the OS).
    Stopped(Option<String>),
    /// While recording, another screen or window is shared too (how many now).
    Added(usize),
    /// The user closed the picker (or it failed) when adding a screen; recording goes on.
    AddCancelled,
    /// One of several shared screens or windows stopped on its own (how many are left).
    Removed(usize),
    /// When adding a screen, the user picked one that's already shared: no capture was started.
    AlreadyShared,
}

/// The message when the user adds a screen or window that's already shared.
pub const ALREADY_SHARED: &str = "That screen is already shared. Pick a different one.";

/// What the tray icon shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indicator {
    Idle,
    Picking,
    Recording,
    Listening,
}

/// A snapshot for the tray menu ("Recording since 10:01 — 12 events", "Skipping: …").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub recording: bool,
    pub picking: bool,
    pub listening: bool,
    pub session_id: Option<String>,
    pub started_at: Option<SystemTime>,
    pub events: usize,
    pub skip: Option<Skip>,
    pub last_error: Option<String>,
    /// Every project in projects.json, for the "Project: …" submenu.
    pub projects: Vec<Project>,
    /// The project the user picked (None: auto-detect).
    pub pinned: Option<String>,
    /// The current project's id and how it was chosen ("auto" / "you").
    pub project: Option<String>,
    pub project_by: Option<String>,
    /// A clue from the window of the latest sample, to prefill "New project from this window".
    pub new_project_clue: Option<String>,
    /// How many screens or windows are shared right now.
    pub sources: usize,
}

impl Status {
    /// The current project, if it is in projects.json.
    pub fn current_project(&self) -> Option<&Project> {
        let id = self.project.as_deref()?;
        self.projects.iter().find(|p| p.id == id)
    }

    /// The submenu's title: "Project: Client report" when picked, "Project: Auto-detect (Client report)"
    /// when detected, "Project: Auto-detect" when nothing matched yet.
    pub fn project_title(&self) -> String {
        let current = self.current_project().map(|p| p.name.as_str());
        match (&self.pinned, current) {
            (Some(pinned), current) => format!("Project: {}", current.unwrap_or(pinned)),
            (None, Some(name)) => format!("Project: Auto-detect ({name})"),
            (None, None) => "Project: Auto-detect".into(),
        }
    }

    /// The "Project" submenu, like the Mac's: Auto-detect, every project (the picked one checked),
    /// then "New project from this window".
    pub fn project_items(&self) -> Vec<ProjectItem> {
        let mut items = vec![ProjectItem::Auto { checked: self.pinned.is_none() }];
        if !self.projects.is_empty() {
            items.push(ProjectItem::Separator);
        }
        for p in &self.projects {
            let checked = self.pinned.as_deref() == Some(p.id.as_str());
            items.push(ProjectItem::Project { id: p.id.clone(), name: p.name.clone(), checked });
        }
        items.extend([ProjectItem::Separator, ProjectItem::New]);
        items
    }

    /// "Add a screen", with how many are shared once there are several.
    pub fn add_screen_label(&self) -> String {
        if self.sources > 1 {
            format!("Add a screen ({} shared)", self.sources)
        } else {
            "Add a screen".into()
        }
    }

    /// While recording, and fewer than `MAX_SOURCES` are shared.
    pub fn can_add_screen(&self) -> bool {
        self.recording && self.sources < MAX_SOURCES
    }

    /// The words above the name field of "New project from this window".
    pub fn new_project_prompt(&self) -> String {
        match &self.new_project_clue {
            Some(clue) => format!("Windows with “{clue}” will count as this project from now on."),
            None => "Name the project. Pick it from this menu while you work on it.".into(),
        }
    }
}

/// One line of the "Project" submenu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectItem {
    /// "Auto-detect", checked when no project is picked.
    Auto { checked: bool },
    Project { id: String, name: String, checked: bool },
    Separator,
    /// "New project from this window"
    New,
}

impl ProjectItem {
    pub const AUTO: &'static str = "Auto-detect";
    pub const NEW: &'static str = "New project from this window";
}

/// The short message when the current project changes.
pub fn project_message(project: Option<&Project>) -> String {
    format!("Project: {}", project.map_or("not sure yet", |p| p.name.as_str()))
}

/// The short message when recording starts (Mac parity: "● Recording started…"). With several monitors
/// it says how to share another one.
pub fn recording_started_message(t: &PlatformText, several_screens: bool) -> String {
    let more = if several_screens { " More screens? Menu → Add a screen." } else { "" };
    format!("● Contextualizing. Press {} to stop, {} to talk.{more}", t.record_hotkey, t.voice_hotkey)
}

/// A screen was added, or one of several stopped sharing.
pub fn screens_message(count: usize, added: bool) -> String {
    if added {
        format!("Screen added: {count} screens shared.")
    } else {
        format!("A screen stopped sharing: {count} left.")
    }
}

/// Shown once when a hotkey can't be registered because another app has it.
pub fn hotkeys_taken_message(taken: &[&str]) -> String {
    let (keys, verb) = match taken {
        [one] => (one.to_string(), "is"),
        _ => (taken.join(" and "), "are"),
    };
    format!("{keys} {verb} taken by another app. The Context Capsule menu in the tray still works.")
}

/// The short message when recording stops.
pub fn recording_stopped_message(events: usize) -> String {
    format!("■ Stopped contextualizing: {events} moment{} saved.", if events == 1 { "" } else { "s" })
}

/// The banner that stays up while a voice note listens.
pub fn listening_message(t: &PlatformText) -> String {
    format!("Listening… press {} to finish", t.voice_hotkey)
}

/// What the platform sends the engine's thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Msg {
    /// Record hotkey or menu item: start (open the picker), cancel the picker, or stop.
    ToggleRecord,
    /// Voice hotkey or menu item: start or finish a voice note.
    ToggleVoice,
    /// "Add a screen": while recording, the picker again for one more screen or window.
    AddScreen,
    /// The picker / capture outcome.
    Pick(PickOutcome),
    /// A voice engine reply arrived (send this from `VoiceNotes::set_wake`).
    VoiceReady,
    /// The "Project" menu: stick to this project id, or auto-detect again (None).
    PickProject(Option<String>),
    /// "New project from this window": the name the user typed, and the clue the dialog showed.
    NewProject { name: String, clue: Option<String> },
    /// Stop any session, write state.json, and return from `run`.
    Quit,
}

/// Everything the engine needs from the OS. Small and object-safe (`dyn Platform` works).
pub trait Platform {
    /// Show the system capture picker, or bring an open one to the front. The result comes back
    /// later as `Msg::Pick(..)` (or a direct `Engine::on_pick`). Capture only what the user picks.
    fn present_picker(&mut self);

    /// Stop capturing and close the picker if it is open. Safe in any state. Do not report a
    /// `PickOutcome` for this; drop the latest frame.
    fn stop_capture(&mut self);

    /// While recording: show the picker for one more screen or window (up to `MAX_SOURCES`). Report
    /// `Added(n)`, `AddCancelled`, and later `Removed(n)` when one of several stops on its own (the last
    /// one stopping is `Stopped`). Platforms that share one source only may do nothing.
    fn add_source(&mut self) {}

    /// Everything shared, placed on screen (global coordinates, top-left origin), each with its own
    /// `source` id. Empty if nothing is being captured. A display whose place is unknown has an empty rect.
    fn picked(&self) -> Vec<PickedContent>;

    /// The latest frame of one shared source, or None if none has arrived yet.
    fn latest_frame(&self, source: u32) -> Option<Frame>;

    /// The frontmost app and the on-screen window list. Mark each window `excluded` with
    /// `is_excluded(app_id, app_name)`, and `notification` for notification banners / toasts.
    /// None if there is no front app at all.
    fn front(&mut self, is_excluded: &dyn Fn(Option<&str>, &str) -> bool) -> Option<Front>;

    /// The focused window's text from the accessibility API (UIA / AT-SPI), assembled with
    /// `UiText::assemble` within its limits. None = accessibility is not available; Some("") = it is,
    /// but there was no text. Never read password fields.
    fn ui_text(&mut self, front: &Front) -> Option<String>;

    /// The focused window's title from the accessibility API (or the picked window's title).
    /// None = unknown; the engine then uses `front.window.name`, else "".
    fn ui_title(&mut self, front: &Front) -> Option<String>;

    /// On-device OCR of an already cropped and masked image: lines top to bottom, joined by "\n".
    fn ocr(&mut self, image: &Image) -> String;

    /// The last app switch, virtual-desktop switch or overview: frames from before it are stale.
    fn changed_at(&self) -> Option<SystemTime>;

    /// Update the tray icon. Called only when it changes.
    fn set_indicator(&mut self, indicator: Indicator);

    /// A short message that fades by itself (voice-note outcomes).
    fn toast(&mut self, text: &str);

    /// A message the user must dismiss.
    fn alert(&mut self, title: &str, body: &str);

    /// This computer's name, for session.json.
    fn host_name(&self) -> String;

    /// Anything shown in the tray menu changed.
    fn status_changed(&mut self, _status: &Status) {}

    /// Evening: ask "Make today's capsules?" with Make / Not now (see `reminder`). Never blocks the engine.
    fn remind(&mut self) {}

    /// The front window's page address (browsers) or file path, if the accessibility API gives it
    /// cheaply. Never slow: None rather than wait.
    fn location(&mut self, _front: &Front) -> Option<String> {
        None
    }

    /// The mouse position (global, top-left origin: the window list's coordinates). None: unknown (Wayland),
    /// and then only the window in use is read.
    fn pointer(&mut self) -> Option<(f64, f64)> {
        None
    }

    /// When the mouse last moved or scrolled, if the platform watches it while recording (hooks). The engine
    /// also notices movement between samples by itself.
    fn pointer_activity(&self) -> Option<SystemTime> {
        None
    }

    /// The window under the mouse as something to read: its app's name and id, `window` = it, `list` =
    /// `front.list`. None: can't tell whose window it is (it is then not read).
    fn pointer_front(&mut self, _window: &ListedWindow, _front: &Front, _is_excluded: &dyn Fn(Option<&str>, &str) -> bool) -> Option<Front> {
        None
    }

    /// While recording, the current project changed (None: auto-detect found nothing yet).
    fn project_changed(&mut self, project: Option<&Project>) {
        self.toast(&project_message(project));
    }

    /// A session ended with nothing written; `why` is the most common skip.
    fn empty_session(&mut self, why: Option<Skip>) {
        let (title, body) = empty_session_alert(why);
        self.alert(&title, &body);
    }
}

/// Both seams must stay object-safe (platforms may hold `Box<dyn …>`).
#[allow(dead_code)]
fn object_safe(_: &dyn Platform, _: &dyn crate::voice::VoiceEngine) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_messages() {
        let t = PlatformText::WINDOWS;
        assert_eq!(
            recording_started_message(&t, false),
            "● Contextualizing. Press Alt+C to stop, Alt+V to talk."
        );
        assert!(recording_started_message(&t, true).ends_with(" More screens? Menu → Add a screen."));
        assert_eq!(screens_message(2, true), "Screen added: 2 screens shared.");
        assert_eq!(screens_message(1, false), "A screen stopped sharing: 1 left.");
        assert_eq!(recording_stopped_message(12), "■ Stopped contextualizing: 12 moments saved.");
        assert_eq!(recording_stopped_message(1), "■ Stopped contextualizing: 1 moment saved.");
        assert_eq!(listening_message(&t), "Listening… press Alt+V to finish");
        assert_eq!(hotkeys_taken_message(&["Alt+C"]), "Alt+C is taken by another app. The Context Capsule menu in the tray still works.");
        assert_eq!(hotkeys_taken_message(&["Alt+C", "Alt+V"]), "Alt+C and Alt+V are taken by another app. The Context Capsule menu in the tray still works.");
    }
}
