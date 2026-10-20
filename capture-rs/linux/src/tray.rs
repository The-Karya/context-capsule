// context-capsule — linux: the tray icon (StatusNotifierItem) and its menu, like the Mac menu bar —
// hollow circle idle, filled red while recording (the running time in its title and tooltip), a red mic with
// bars that move with the voice while listening; status, skip reason and actions. Nothing on screen.

use crate::notify::Notifier;
use context_capsule_core::look::{self, LEVEL_FRAMES};
use context_capsule_core::{Indicator, Msg, ProjectItem, Status, Voice};
use ksni::menu::{CheckmarkItem, StandardItem, SubMenu};
use ksni::{MenuItem, TrayMethods};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::runtime::Handle;

/// While recording or listening, the time and the voice bars are refreshed this often.
const LOOK_EVERY: Duration = Duration::from_millis(125);

pub const RECORD_KEY: &str = "Ctrl+Alt+Shift+C";
pub const VOICE_KEY: &str = "Ctrl+Alt+Shift+V";

pub struct CapsuleTray {
    tx: Sender<Msg>,
    notifier: Notifier,
    status: Status,
    indicator: Indicator,
    /// The listening step (`look::level_frame`).
    level: usize,
}

/// The menu's first line, as of `now` (the running time).
pub fn info_line(status: &Status, now: SystemTime, time: &dyn Fn(SystemTime) -> String) -> String {
    match (&status.session_id, status.started_at) {
        (Some(_), Some(started)) if status.recording => {
            let n = status.events;
            let elapsed = look::elapsed(status, now).unwrap_or_default();
            format!("● Contextualizing {elapsed} — since {} — {} moment{}", time(started), n, if n == 1 { "" } else { "s" })
        }
        _ if status.picking => "Pick a window or screen…".into(),
        _ => "Not contextualizing".into(),
    }
}

pub fn toggle_label(status: &Status) -> &'static str {
    if status.recording {
        "Stop contextualizing"
    } else if status.picking {
        "Cancel picking"
    } else {
        "Start contextualizing…"
    }
}

/// The tray's title: the running time while contextualizing.
pub fn title_text(status: &Status, now: SystemTime) -> String {
    match look::elapsed(status, now) {
        Some(time) => format!("Context Capsule ● {time}"),
        None => "Context Capsule".into(),
    }
}

/// The hover text, as of `now`.
pub fn tooltip_text(indicator: Indicator, status: &Status, now: SystemTime) -> String {
    match indicator {
        Indicator::Listening => format!("Listening — press {VOICE_KEY} to finish the voice note"),
        Indicator::Recording => {
            let time = look::elapsed(status, now).unwrap_or_default();
            format!("● Contextualizing {time} — {RECORD_KEY} to stop, {VOICE_KEY} to talk")
        }
        _ => format!("Context Capsule ({RECORD_KEY} to start contextualizing)"),
    }
}

/// Menu labels treat "_" as an access-key marker: double it so names show as typed.
pub fn label(s: &str) -> String {
    s.replace('_', "__")
}

/// Said when there is no zenity or kdialog to type a name in.
pub const NO_DIALOG: &str =
    "To name a new project, install zenity (or kdialog), or ask Claude to create it (create_project).";

fn local_time(t: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(t).format("%H:%M").to_string()
}

impl ksni::Tray for CapsuleTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "context-capsule".into()
    }
    fn title(&self) -> String {
        title_text(&self.status, SystemTime::now())
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        [22, 44].iter().map(|s| icon(self.indicator, self.level, *s)).collect()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip { title: tooltip_text(self.indicator, &self.status, SystemTime::now()), ..Default::default() }
    }
    fn status(&self) -> ksni::Status {
        ksni::Status::Active
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let send =
            |msg: Msg| Box::new(move |t: &mut Self| drop(t.tx.send(msg.clone()))) as Box<dyn Fn(&mut Self) + Send>;
        let skip = self.status.skip.filter(|_| self.status.recording);
        vec![
            StandardItem { label: info_line(&self.status, SystemTime::now(), &local_time), enabled: false, ..Default::default() }
                .into(),
            StandardItem {
                label: format!("Skipping: {}", skip.map(|s| s.message()).unwrap_or("")),
                enabled: false,
                visible: skip.is_some(),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: toggle_label(&self.status).into(),
                shortcut: vec![keys("C")],
                activate: send(Msg::ToggleRecord),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: if self.status.listening { "Finish voice note" } else { "Add voice note" }.into(),
                shortcut: vec![keys("V")],
                activate: send(Msg::ToggleVoice),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.status.add_screen_label(),
                enabled: self.status.can_add_screen(),
                visible: self.status.recording,
                activate: send(Msg::AddScreen),
                ..Default::default()
            }
            .into(),
            self.project_menu(),
            MenuItem::Separator,
            StandardItem { label: "Quit Context Capsule".into(), activate: send(Msg::Quit), ..Default::default() }
                .into(),
        ]
    }
}

impl CapsuleTray {
    /// "Project: …" with Auto-detect, each project (the picked one checked) and "New project from
    /// this window…".
    fn project_menu(&self) -> MenuItem<Self> {
        let pick = |id: Option<String>| {
            Box::new(move |t: &mut Self| drop(t.tx.send(Msg::PickProject(id.clone())))) as Box<dyn Fn(&mut Self) + Send>
        };
        let submenu = self
            .status
            .project_items()
            .into_iter()
            .map(|item| match item {
                ProjectItem::Auto { checked } => {
                    CheckmarkItem { label: ProjectItem::AUTO.into(), checked, activate: pick(None), ..Default::default() }
                        .into()
                }
                ProjectItem::Project { id, name, checked } => {
                    CheckmarkItem { label: label(&name), checked, activate: pick(Some(id)), ..Default::default() }.into()
                }
                ProjectItem::Separator => MenuItem::Separator,
                ProjectItem::New => StandardItem {
                    label: ProjectItem::NEW.into(),
                    activate: Box::new(|t: &mut Self| t.new_project()),
                    ..Default::default()
                }
                .into(),
            })
            .collect();
        SubMenu { label: label(&self.status.project_title()), submenu, ..Default::default() }.into()
    }

    /// Ask for a name (prefilled with the window's clue) on a thread of its own: the dialog blocks.
    fn new_project(&self) {
        let (tx, notifier) = (self.tx.clone(), self.notifier.clone());
        let clue = self.status.new_project_clue.clone();
        let prompt = self.status.new_project_prompt();
        std::thread::spawn(move || {
            match notifier.ask_text("New project", &prompt, clue.as_deref().unwrap_or("")) {
                Ok(Some(name)) => drop(tx.send(Msg::NewProject { name, clue })),
                Ok(None) => {}
                Err(()) => notifier.toast(NO_DIALOG),
            }
        });
    }
}

fn keys(key: &str) -> Vec<String> {
    ["Control", "Alt", "Shift", key].iter().map(|s| s.to_string()).collect()
}

/// What the refresh task needs to know: recording or listening, and what it last showed.
#[derive(Default)]
struct Look {
    status: Status,
    shown: Option<(usize, String)>,
}

/// The running tray (or nothing, when the desktop has no tray).
pub struct Tray {
    rt: Handle,
    handle: Option<ksni::Handle<CapsuleTray>>,
    look: Arc<Mutex<Look>>,
}

impl Tray {
    /// Show the tray icon. On GNOME this needs the AppIndicator extension; without it we log and go on.
    pub fn spawn(rt: &Handle, tx: Sender<Msg>, notifier: Notifier) -> Tray {
        let tray = CapsuleTray { tx, notifier, status: Status::default(), indicator: Indicator::Idle, level: 0 };
        let handle = match rt.block_on(tray.spawn()) {
            Ok(h) => Some(h),
            Err(e) => {
                eprintln!(
                    "Context Capsule: no tray icon ({e}). On GNOME install the AppIndicator extension; the hotkeys still work."
                );
                None
            }
        };
        let look = Arc::new(Mutex::new(Look::default()));
        if let Some(h) = handle.clone() {
            rt.spawn(refresh(h, look.clone()));
        }
        Tray { rt: rt.clone(), handle, look }
    }

    /// No tray (for `--probe`).
    pub fn none(rt: &Handle) -> Tray {
        Tray { rt: rt.clone(), handle: None, look: Arc::default() }
    }

    pub fn is_shown(&self) -> bool {
        self.handle.is_some()
    }

    pub fn set_status(&self, status: &Status) {
        self.look.lock().unwrap().status = status.clone();
        let status = status.clone();
        self.update(move |t| t.status = status);
    }

    pub fn set_indicator(&self, indicator: Indicator) {
        self.update(move |t| t.indicator = indicator);
    }

    fn update(&self, f: impl FnOnce(&mut CapsuleTray) + Send + 'static) {
        if let Some(h) = self.handle.clone() {
            self.rt.spawn(async move {
                h.update(f).await;
            });
        }
    }

    pub fn shutdown(&self) {
        if let Some(h) = &self.handle {
            self.rt.block_on(h.shutdown());
        }
    }
}

/// While recording or listening: the time in the title and tooltip once a second, the voice bars as the mic
/// level changes. Only sends an update when something visible changed.
async fn refresh(h: ksni::Handle<CapsuleTray>, look: Arc<Mutex<Look>>) {
    let mut tick = tokio::time::interval(LOOK_EVERY);
    loop {
        tick.tick().await;
        let next = {
            let mut l = look.lock().unwrap();
            if !l.status.recording && !l.status.listening {
                l.shown = None;
                continue;
            }
            let level = if l.status.listening { look::level_frame(Voice::level()) } else { 0 };
            let shown = (level, look::elapsed(&l.status, SystemTime::now()).unwrap_or_default());
            if l.shown.as_ref() == Some(&shown) {
                continue;
            }
            l.shown = Some(shown);
            level
        };
        if h.update(move |t| t.level = next).await.is_none() {
            return; // the tray is gone
        }
    }
}

/// A `size`×`size` ARGB32 (network order) icon: hollow circle (idle, picking), filled red circle
/// (recording), a red mic with bars at `level` (listening). Light strokes with a dark edge, so it shows on
/// any panel.
pub fn icon(indicator: Indicator, level: usize, size: i32) -> ksni::Icon {
    if indicator == Indicator::Listening {
        let data = look::mic_bars(size as usize, level.min(LEVEL_FRAMES - 1))
            .into_iter()
            .flat_map(|a| [a, 0xE0, 0x1B, 0x24])
            .collect();
        return ksni::Icon { width: size, height: size, data };
    }
    let s = size as f64;
    let c = s / 2.0;
    let r = s * 0.36;
    let stroke = (s * 0.11).max(1.5);
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            // Signed distance to the shape (negative inside).
            let d = match indicator {
                Indicator::Recording => (px - c).hypot(py - c) - r,
                _ => ((px - c).hypot(py - c) - r).abs() - stroke / 2.0,
            };
            let fill = (0.5 - d).clamp(0.0, 1.0); // anti-aliased edge
            let edge = (0.5 - (d - 1.0)).clamp(0.0, 1.0); // a 1 px dark rim around it
            let (red, green, blue) =
                if indicator == Indicator::Recording { (0xE0, 0x1B, 0x24) } else { (0xF2, 0xF2, 0xF2) };
            let a = fill + (edge - fill) * 0.55;
            let mix = |v: u8| ((v as f64) * fill / a.max(1e-6)) as u8;
            data.extend_from_slice(&[(a * 255.0) as u8, mix(red), mix(green), mix(blue)]);
        }
    }
    ksni::Icon { width: size, height: size, data }
}

#[cfg(test)]
mod tests {
    use super::*;
    use context_capsule_core::Skip;

    fn alpha(icon: &ksni::Icon, x: i32, y: i32) -> u8 {
        icon.data[((y * icon.width + x) * 4) as usize]
    }

    #[test]
    fn icons() {
        let idle = icon(Indicator::Idle, 0, 22);
        assert_eq!(idle.data.len(), 22 * 22 * 4);
        assert_eq!(alpha(&idle, 11, 11), 0, "hollow");
        assert!(alpha(&idle, 11, 3) > 200, "ring");
        let rec = icon(Indicator::Recording, 0, 22);
        assert_eq!(alpha(&rec, 11, 11), 255, "filled");
        assert_eq!(rec.data[(11 * 22 + 11) * 4 + 1], 0xE0, "red");
        let mic = icon(Indicator::Listening, 0, 44);
        assert!(alpha(&mic, 12, 12) > 200, "mic capsule");
        assert_eq!(mic.data[(12 * 44 + 12) * 4 + 1], 0xE0, "red");
        assert_eq!(alpha(&mic, 0, 0), 0);
        let loud = icon(Indicator::Listening, 3, 44);
        assert!(alpha(&loud, 33, 8) > alpha(&mic, 33, 8), "louder: taller bars");
    }

    #[test]
    fn menu_lines() {
        let t = |_: SystemTime| "10:01".to_string();
        let now = SystemTime::now();
        let mut s = Status::default();
        assert_eq!(info_line(&s, now, &t), "Not contextualizing");
        assert_eq!(toggle_label(&s), "Start contextualizing…");
        assert_eq!(title_text(&s, now), "Context Capsule");
        s.picking = true;
        assert_eq!(info_line(&s, now, &t), "Pick a window or screen…");
        assert_eq!(toggle_label(&s), "Cancel picking");
        s = Status {
            recording: true,
            session_id: Some("20261019-100103".into()),
            started_at: Some(now),
            events: 1,
            skip: Some(Skip::NotPicked),
            ..Default::default()
        };
        let later = now + Duration::from_secs(754);
        assert_eq!(info_line(&s, later, &t), "● Contextualizing 12:34 — since 10:01 — 1 moment");
        assert_eq!(toggle_label(&s), "Stop contextualizing");
        assert_eq!(title_text(&s, later), "Context Capsule ● 12:34");
        assert_eq!(
            tooltip_text(Indicator::Recording, &s, later),
            "● Contextualizing 12:34 — Ctrl+Alt+Shift+C to stop, Ctrl+Alt+Shift+V to talk"
        );
        assert!(tooltip_text(Indicator::Listening, &s, later).starts_with("Listening"));
    }

    #[test]
    fn project_labels() {
        assert_eq!(label("my_project"), "my__project");
        let p = context_capsule_core::Project { id: "a-b".into(), name: "a_b".into(), clues: vec![] };
        let s = Status { projects: vec![p], pinned: Some("a-b".into()), project: Some("a-b".into()), ..Default::default() };
        assert_eq!(label(&s.project_title()), "Project: a__b");
        assert!(NO_DIALOG.contains("create_project"));
    }
}
