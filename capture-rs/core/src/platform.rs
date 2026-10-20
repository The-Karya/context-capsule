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
    /// The capture ended on its own (None: the user stopped sharing from the OS).
    Stopped(Option<String>),
}

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
    /// A clue from the window of the latest sample, to prefill "New project from this window…".
    pub new_project_clue: Option<String>,
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
    /// then "New project from this window…".
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

    /// The words above the name field of "New project from this window…".
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
    /// "New project from this window…"
    New,
}

impl ProjectItem {
    pub const AUTO: &'static str = "Auto-detect";
    pub const NEW: &'static str = "New project from this window…";
}

/// The short message when the current project changes.
pub fn project_message(project: Option<&Project>) -> String {
    format!("Project: {}", project.map_or("not sure yet", |p| p.name.as_str()))
}

/// The short message when recording starts (Mac parity: "● Recording started…").
pub fn recording_started_message(t: &PlatformText) -> String {
    format!("● Recording started. Press {} to stop, {} to talk.", t.record_hotkey, t.voice_hotkey)
}

/// The short message when recording stops.
pub fn recording_stopped_message(events: usize) -> String {
    format!("■ Recording stopped: {events} event{} saved.", if events == 1 { "" } else { "s" })
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
    /// The picker / capture outcome.
    Pick(PickOutcome),
    /// A voice engine reply arrived (send this from `VoiceNotes::set_wake`).
    VoiceReady,
    /// The "Project" menu: stick to this project id, or auto-detect again (None).
    PickProject(Option<String>),
    /// "New project from this window…": the name the user typed, and the clue the dialog showed.
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

    /// What the user picked, placed on screen (global coordinates, top-left origin). None if nothing
    /// is being captured. A display whose place is unknown has an empty rect.
    fn picked(&self) -> Option<PickedContent>;

    /// The latest frame of the picked content, or None if none has arrived yet.
    fn latest_frame(&self) -> Option<Frame>;

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

    /// The front window's page address (browsers) or file path, if the accessibility API gives it
    /// cheaply. Never slow: None rather than wait.
    fn location(&mut self, _front: &Front) -> Option<String> {
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
            recording_started_message(&t),
            "● Recording started. Press Ctrl+Alt+Shift+C to stop, Ctrl+Alt+Shift+V to talk."
        );
        assert_eq!(recording_stopped_message(12), "■ Recording stopped: 12 events saved.");
        assert_eq!(recording_stopped_message(1), "■ Recording stopped: 1 event saved.");
        assert_eq!(listening_message(&t), "Listening… press Ctrl+Alt+Shift+V to finish");
    }
}
