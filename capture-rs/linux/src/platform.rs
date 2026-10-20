// context-capsule — linux: the engine's seam to the desktop — the picked content, the front window
// (AT-SPI, plus the real X11 window stack on X11), its text and title, OCR, the tray, messages, the soft
// start/stop sounds and the evening reminder.

use crate::apps::ProcApp;
use crate::atspi_front::{is_shell, Accessibility, ActiveWindow};
use crate::desktop::{Overview, SessionKind};
use crate::notify::Notifier;
use crate::ocr::{Ocr, OcrResult, INSTALL_HINT};
use crate::portal::{self, Capture, SourceKind};
use crate::tray::Tray;
use crate::{atspi_text, reminder, x11};
use context_capsule_core::{
    Config, Cue, Frame, Front, Image, Indicator, ListedWindow, Membership, PickedContent, Platform, Rect, Status, WindowList,
};
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::runtime::Handle;

/// Longest any one desktop query may take before we treat it as unanswered.
const QUERY: Duration = Duration::from_millis(1500);

pub struct LinuxPlatform {
    pub rt: Handle,
    pub session: SessionKind,
    pub capture: Capture,
    pub a11y: Option<Arc<Accessibility>>,
    /// The X server: the window stack on X11; on Wayland only Xwayland's monitor list.
    pub x11: Option<x11::Desktop>,
    pub overview: Option<Overview>,
    pub changed: Arc<Mutex<Option<SystemTime>>>,
    pub notifier: Notifier,
    pub tray: Tray,
    pub ocr: Ocr,
    /// The AT-SPI active window of the current sample (front() sets it, ui_text / ui_title use it).
    current: Option<ActiveWindow>,
    /// The first document in the window's tree, from the latest text walk (for `location`).
    document: Option<atspi::ObjectRefOwned>,
    /// Per shared window: the window whose size matches its stream (None: none, or more than one).
    matched: HashMap<u32, Option<u64>>,
    ocr_hint_shown: bool,
    /// The last status, to play a sound when recording or the mic starts or stops.
    last_status: Status,
    /// Where the data lives (config.json: "sounds").
    config: std::path::PathBuf,
}

/// On Wayland no app knows where its window is. With picked monitors, the front window counts as on one
/// only when every monitor is shared (one monitor, or several added one by one); then it gets a sub-pixel
/// frame at the first shared monitor's centre: inside for the membership test (so accessibility text is
/// read) but too small to crop, so OCR skips with `not_picked` instead of reading a guessed region. Not
/// every (or an unknown number of) monitor shared: an empty frame, so nothing is read.
pub fn wayland_monitor_frame(picks: &[Rect], monitors: usize) -> Rect {
    let mut shared: Vec<Rect> = vec![];
    for p in picks.iter().filter(|p| !p.is_empty()) {
        if !shared.iter().any(|s| Membership::same_frame(s, p)) {
            shared.push(*p);
        }
    }
    match shared.first() {
        Some(first) if monitors >= 1 && shared.len() >= monitors => Rect::new(first.mid_x(), first.mid_y(), 0.5, 0.5),
        _ => Rect::ZERO,
    }
}

impl LinuxPlatform {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rt: Handle,
        session: SessionKind,
        capture: Capture,
        a11y: Option<Arc<Accessibility>>,
        x11: Option<x11::Desktop>,
        overview: Option<Overview>,
        changed: Arc<Mutex<Option<SystemTime>>>,
        notifier: Notifier,
        tray: Tray,
        locale: &str,
        config: std::path::PathBuf,
    ) -> LinuxPlatform {
        LinuxPlatform {
            rt,
            session,
            capture,
            a11y,
            x11,
            overview,
            changed,
            notifier,
            tray,
            ocr: Ocr::new(locale),
            current: None,
            document: None,
            matched: HashMap::new(),
            ocr_hint_shown: false,
            last_status: Status::default(),
            config,
        }
    }

    /// Run a desktop query from the engine's thread, giving up after `QUERY`.
    fn wait<T>(&self, f: impl Future<Output = T>) -> Option<T> {
        self.rt.block_on(async { tokio::time::timeout(QUERY, f).await.ok() })
    }

    fn monitors(&self) -> Vec<Rect> {
        self.x11.as_ref().map(|x| x.monitors()).unwrap_or_default()
    }

    fn overlay(&self) -> bool {
        match &self.overview {
            Some(o) => self.wait(o.is_open()).unwrap_or(false),
            None => false,
        }
    }

    /// The AT-SPI window, if it belongs to `pid`.
    fn current_for(&self, pid: u32) -> Option<&ActiveWindow> {
        self.current.as_ref().filter(|a| self.session != SessionKind::X11 || a.pid == pid)
    }

    fn front_wayland(&mut self, is_excluded: &dyn Fn(Option<&str>, &str) -> bool, overlay: bool) -> Option<Front> {
        let a = self.current.clone()?;
        let proc_app = ProcApp::of(a.pid);
        let app_name =
            if a.app_name.is_empty() { proc_app.exe.clone().unwrap_or_default() } else { a.app_name.clone() };
        let app_id = proc_app.app_id(is_excluded, &app_name);
        let mut front = Front {
            app_name: app_name.clone(),
            app_id: app_id.clone(),
            pid: a.pid,
            window: None,
            list: vec![],
            overlay,
        };
        if is_shell(&a.app_name, proc_app.exe.as_deref()) {
            return Some(front); // the shell's own menus: no app window
        }
        let size = a.extents.map_or(Rect::ZERO, |r| Rect::new(0.0, 0.0, r.width, r.height));
        let sources = self.capture.sources();
        let windows: Vec<(u32, Rect)> =
            sources.iter().filter(|(_, s)| s.kind == SourceKind::Window).map(|(id, s)| (*id, s.size_rect())).collect();
        if !windows.is_empty() {
            let candidates = match &self.a11y {
                Some(a11y) => self.wait(a11y.window_sizes()).unwrap_or_default(),
                None => vec![],
            };
            self.matched = windows.iter().map(|(id, size)| (*id, portal::match_window(*size, &candidates))).collect();
        }
        let monitors = self.monitors();
        let shared: Vec<Rect> = sources
            .iter()
            .filter(|(_, s)| s.kind == SourceKind::Monitor)
            .map(|(_, s)| portal::monitor_rect(s, &monitors))
            .collect();
        // Shared monitors: placed only when every monitor is shared. Windows only: matched by id.
        let frame = if sources.iter().any(|(_, s)| s.kind == SourceKind::Monitor) {
            wayland_monitor_frame(&shared, monitors.len())
        } else {
            size
        };
        let window = ListedWindow {
            id: a.id(),
            pid: a.pid,
            owner: app_name.clone(),
            frame,
            name: Some(a.title.clone()).filter(|t| !t.is_empty()),
            excluded: is_excluded(app_id.as_deref(), &app_name),
            ..Default::default()
        };
        front.list = vec![window.clone()];
        front.window = Some(window);
        Some(front)
    }

    fn front_x11(&mut self, is_excluded: &dyn Fn(Option<&str>, &str) -> bool, overlay: bool) -> Option<Front> {
        let x = self.x11.as_ref()?;
        let active = x.active_window();
        let windows = x.windows();
        let list = x11::listed(&windows, is_excluded);
        let entry = active.and_then(|a| list.iter().find(|w| w.id == a as u64)).cloned();
        let pid = entry.as_ref().map(|w| w.pid).filter(|p| *p != 0).or(self.current.as_ref().map(|a| a.pid))?;
        let proc_app = ProcApp::of(pid);
        let class = entry.as_ref().map(|w| w.owner.clone()).unwrap_or_default();
        let app_name = match self.current_for(pid) {
            Some(a) if !a.app_name.is_empty() => a.app_name.clone(),
            _ if !class.is_empty() => class,
            _ => proc_app.exe.clone().unwrap_or_default(),
        };
        let app_id = proc_app.app_id(is_excluded, &app_name);
        let shell = is_shell(&app_name, proc_app.exe.as_deref());
        let window = match entry {
            _ if shell => None,
            Some(w) if WindowList::is_normal(&w) => Some(w),
            _ => WindowList::front(pid, &list).cloned(),
        };
        let windows: Vec<(u32, Rect)> = self
            .capture
            .sources()
            .into_iter()
            .filter(|(_, s)| s.kind == SourceKind::Window)
            .map(|(id, s)| (id, s.size_rect()))
            .collect();
        if !windows.is_empty() {
            let candidates: Vec<(u64, Rect)> =
                list.iter().filter(|w| WindowList::is_normal(w)).map(|w| (w.id, w.frame)).collect();
            self.matched = windows.iter().map(|(id, size)| (*id, portal::match_window(*size, &candidates))).collect();
        }
        Some(Front { app_name, app_id, pid, window, list, overlay })
    }
}

impl Platform for LinuxPlatform {
    fn present_picker(&mut self) {
        self.capture.present();
    }

    fn stop_capture(&mut self) {
        self.capture.stop();
        self.matched.clear();
    }

    fn add_source(&mut self) {
        self.capture.present_add();
    }

    fn picked(&self) -> Vec<PickedContent> {
        let monitors = self.monitors();
        self.capture
            .sources()
            .into_iter()
            .map(|(id, src)| portal::picked_content(&src, &monitors, self.matched.get(&id).copied().flatten()).with_source(id))
            .collect()
    }

    fn latest_frame(&self, source: u32) -> Option<Frame> {
        self.capture.latest_frame(source)
    }

    fn front(&mut self, is_excluded: &dyn Fn(Option<&str>, &str) -> bool) -> Option<Front> {
        let overlay = self.overlay();
        self.current = match &self.a11y {
            Some(a11y) => self.wait(a11y.active_window()).flatten(),
            None => None,
        };
        match self.session {
            SessionKind::X11 => self.front_x11(is_excluded, overlay),
            _ => self.front_wayland(is_excluded, overlay),
        }
    }

    fn ui_text(&mut self, front: &Front) -> Option<String> {
        self.document = None;
        let a11y = self.a11y.clone()?;
        let obj = self.current_for(front.pid)?.obj.clone();
        let read = self.wait(atspi_text::read(a11y.connection(), &obj)).flatten()?;
        self.document = read.document;
        Some(read.text)
    }

    /// The address of the document the text walk found (a web view's page, an editor's file).
    fn location(&mut self, front: &Front) -> Option<String> {
        let a11y = self.a11y.clone()?;
        self.current_for(front.pid)?;
        let doc = self.document.clone()?;
        self.wait(atspi_text::document_address(a11y.connection(), &doc)).flatten()
    }

    fn ui_title(&mut self, front: &Front) -> Option<String> {
        self.current_for(front.pid).map(|a| a.title.clone()).filter(|t| !t.is_empty())
    }

    fn ocr(&mut self, image: &Image) -> String {
        match self.ocr.read(image) {
            OcrResult::Text(t) => t,
            OcrResult::Missing => {
                if !self.ocr_hint_shown {
                    self.ocr_hint_shown = true;
                    eprintln!("Context Capsule: tesseract is not installed; OCR is off");
                    self.notifier.toast(INSTALL_HINT);
                }
                String::new()
            }
        }
    }

    fn changed_at(&self) -> Option<SystemTime> {
        *self.changed.lock().unwrap()
    }

    fn set_indicator(&mut self, indicator: Indicator) {
        self.tray.set_indicator(indicator);
    }

    fn toast(&mut self, text: &str) {
        self.notifier.toast(text);
    }

    fn alert(&mut self, title: &str, body: &str) {
        self.notifier.alert(title, body);
    }

    fn host_name(&self) -> String {
        host_name()
    }

    fn status_changed(&mut self, status: &Status) {
        let cues = Cue::changes(&self.last_status, status);
        if !cues.is_empty() && Config::load(&self.config).sounds {
            cues.into_iter().for_each(reminder::play_cue);
        }
        self.last_status = status.clone();
        self.tray.set_status(status);
    }

    fn remind(&mut self) {
        reminder::ask_and_make(self.notifier.clone());
    }
}

pub fn host_name() -> String {
    ["/proc/sys/kernel/hostname", "/etc/hostname"]
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .find(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use context_capsule_core::{Membership, OcrDecision, Skip};

    #[test]
    fn wayland_single_monitor_reads_text_but_never_guesses_ocr() {
        let monitor = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let pick = PickedContent::display(monitor);
        let mut front = ListedWindow::new(1, 1, wayland_monitor_frame(&[monitor], 1));
        front.layer = 0;
        assert!(Membership::contains(&pick, &front), "accessibility text is read");
        assert_eq!(
            Membership::ocr_decision(&pick, &front, std::slice::from_ref(&front)),
            OcrDecision::Skip(Skip::NotPicked),
            "OCR fails closed"
        );
    }

    #[test]
    fn wayland_several_monitors_read_nothing() {
        let monitor = Rect::new(1920.0, 0.0, 1920.0, 1080.0);
        let pick = PickedContent::display(monitor);
        for n in [0, 2, 3] {
            let front = ListedWindow::new(1, 1, wayland_monitor_frame(&[monitor], n));
            assert!(!Membership::contains(&pick, &front), "{n} monitors: cannot tell where the window is");
        }
        assert!(wayland_monitor_frame(&[Rect::ZERO], 1).is_empty());
        assert!(wayland_monitor_frame(&[monitor, monitor], 2).is_empty(), "the same monitor twice is one");
    }

    #[test]
    fn wayland_every_monitor_shared_reads_text() {
        let (left, right) = (Rect::new(0.0, 0.0, 1920.0, 1080.0), Rect::new(1920.0, 0.0, 1920.0, 1080.0));
        let frame = wayland_monitor_frame(&[right, left], 2);
        let picks = [PickedContent::display(right).with_source(1), PickedContent::display(left).with_source(2)];
        let front = ListedWindow::new(1, 1, frame);
        assert_eq!(Membership::pick(&front, &picks).map(|p| p.source), Some(1), "both shared: text is read");
        assert_eq!(
            Membership::ocr_decision(&picks[0], &front, std::slice::from_ref(&front)),
            OcrDecision::Skip(Skip::NotPicked),
            "OCR still fails closed"
        );
    }

    #[test]
    fn host() {
        assert!(!host_name().contains('\n'));
    }
}
