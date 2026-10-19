// context-capsule — core: the one seam between the engine and the OS. Windows and Linux implement `Platform`.
//! The engine is single-threaded: the platform runs it on its own thread (see `Engine::run`) and
//! feeds it [`Msg`]s from hotkeys, the tray menu and the capture picker. Every method below is called
//! on that thread, so implementations may hop to their UI thread as they need.

use crate::engine::empty_session_alert;
use crate::image::Image;
use crate::membership::{ListedWindow, PickedContent, Skip};
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

    /// A session ended with nothing written; `why` is the most common skip.
    fn empty_session(&mut self, why: Option<Skip>) {
        let (title, body) = empty_session_alert(why);
        self.alert(&title, &body);
    }
}

/// Both seams must stay object-safe (platforms may hold `Box<dyn …>`).
#[allow(dead_code)]
fn object_safe(_: &dyn Platform, _: &dyn crate::voice::VoiceEngine) {}
