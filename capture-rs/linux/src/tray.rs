// context-capsule — linux: the tray icon (StatusNotifierItem) and its menu, like the Mac menu bar —
// hollow circle idle, filled while recording, a mic while listening; status, skip reason and actions.

use context_capsule_core::{Indicator, Msg, Status};
use ksni::menu::StandardItem;
use ksni::{MenuItem, TrayMethods};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::SystemTime;
use tokio::runtime::Handle;

pub const RECORD_KEY: &str = "Ctrl+Alt+Shift+C";
pub const VOICE_KEY: &str = "Ctrl+Alt+Shift+V";

pub struct CapsuleTray {
    tx: Sender<Msg>,
    sessions: PathBuf,
    status: Status,
    indicator: Indicator,
}

/// The menu's first line.
pub fn info_line(status: &Status, time: &dyn Fn(SystemTime) -> String) -> String {
    match (&status.session_id, status.started_at) {
        (Some(id), Some(started)) if status.recording => {
            let n = status.events;
            format!("Recording since {} — {} event{} — {}", time(started), n, if n == 1 { "" } else { "s" }, id)
        }
        _ if status.picking => "Pick a window or screen to record…".into(),
        _ => "Not recording".into(),
    }
}

pub fn toggle_label(status: &Status) -> &'static str {
    if status.recording {
        "Stop recording"
    } else if status.picking {
        "Cancel picking"
    } else {
        "Start recording…"
    }
}

fn local_time(t: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(t).format("%H:%M").to_string()
}

impl ksni::Tray for CapsuleTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "context-capsule".into()
    }
    fn title(&self) -> String {
        "Context Capsule".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        [22, 44].iter().map(|s| icon(self.indicator, *s)).collect()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        let title = match self.indicator {
            Indicator::Listening => format!("Listening — press {VOICE_KEY} to finish the voice note"),
            Indicator::Recording => {
                format!("Context Capsule is recording ({RECORD_KEY} to stop, {VOICE_KEY} for a voice note)")
            }
            _ => format!("Context Capsule ({RECORD_KEY} to record)"),
        };
        ksni::ToolTip { title, ..Default::default() }
    }
    fn status(&self) -> ksni::Status {
        ksni::Status::Active
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let send =
            |msg: Msg| Box::new(move |t: &mut Self| drop(t.tx.send(msg.clone()))) as Box<dyn Fn(&mut Self) + Send>;
        let skip = self.status.skip.filter(|_| self.status.recording);
        vec![
            StandardItem { label: info_line(&self.status, &local_time), enabled: false, ..Default::default() }.into(),
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
                label: "Open sessions folder".into(),
                activate: Box::new(|t: &mut Self| open_folder(&t.sessions)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem { label: "Quit Context Capsule".into(), activate: send(Msg::Quit), ..Default::default() }
                .into(),
        ]
    }
}

fn keys(key: &str) -> Vec<String> {
    ["Control", "Alt", "Shift", key].iter().map(|s| s.to_string()).collect()
}

fn open_folder(dir: &std::path::Path) {
    let _ = std::fs::create_dir_all(dir);
    match std::process::Command::new("xdg-open").arg(dir).spawn() {
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(e) => eprintln!("Context Capsule: cannot open {}: {e}", dir.display()),
    }
}

/// The running tray (or nothing, when the desktop has no tray).
pub struct Tray {
    rt: Handle,
    handle: Option<ksni::Handle<CapsuleTray>>,
}

impl Tray {
    /// Show the tray icon. On GNOME this needs the AppIndicator extension; without it we log and go on.
    pub fn spawn(rt: &Handle, tx: Sender<Msg>, sessions: PathBuf) -> Tray {
        let tray = CapsuleTray { tx, sessions, status: Status::default(), indicator: Indicator::Idle };
        let handle = match rt.block_on(tray.spawn()) {
            Ok(h) => Some(h),
            Err(e) => {
                eprintln!(
                    "Context Capsule: no tray icon ({e}). On GNOME install the AppIndicator extension; the hotkeys still work."
                );
                None
            }
        };
        Tray { rt: rt.clone(), handle }
    }

    /// No tray (for `--probe`).
    pub fn none(rt: &Handle) -> Tray {
        Tray { rt: rt.clone(), handle: None }
    }

    pub fn is_shown(&self) -> bool {
        self.handle.is_some()
    }

    pub fn set_status(&self, status: &Status) {
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

/// A `size`×`size` ARGB32 (network order) icon: hollow circle (idle, picking), filled circle
/// (recording), microphone (listening). Light strokes with a dark edge, so it shows on any panel.
pub fn icon(indicator: Indicator, size: i32) -> ksni::Icon {
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
                Indicator::Idle | Indicator::Picking => ((px - c).hypot(py - c) - r).abs() - stroke / 2.0,
                Indicator::Recording => (px - c).hypot(py - c) - r,
                Indicator::Listening => mic_distance(px / s, py / s) * s,
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

/// A microphone in a unit square: a capsule, a U-shaped holder and a stand. Distance in units.
fn mic_distance(x: f64, y: f64) -> f64 {
    let capsule = {
        let (cx, top, bottom, rad) = (0.5, 0.26, 0.48, 0.13);
        let cy = y.clamp(top, bottom);
        (x - cx).hypot(y - cy) - rad
    };
    let holder = {
        let (cx, cy, rad, w) = (0.5, 0.48, 0.23, 0.035);
        let ring = ((x - cx).hypot(y - cy) - rad).abs() - w;
        if y < cy {
            ring.max(cy - y)
        } else {
            ring
        }
    };
    let stand = (x - 0.5).abs().max((y - 0.80).abs() - 0.06) - 0.035;
    let base = (x - 0.5).abs().max((y - 0.86).abs()) - 0.0;
    let base = base.max((x - 0.5).abs() - 0.16).max((y - 0.86).abs() - 0.035);
    capsule.min(holder).min(stand).min(base)
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
        let idle = icon(Indicator::Idle, 22);
        assert_eq!(idle.data.len(), 22 * 22 * 4);
        assert_eq!(alpha(&idle, 11, 11), 0, "hollow");
        assert!(alpha(&idle, 11, 3) > 200, "ring");
        let rec = icon(Indicator::Recording, 22);
        assert_eq!(alpha(&rec, 11, 11), 255, "filled");
        assert_eq!(rec.data[(11 * 22 + 11) * 4 + 1], 0xE0, "red");
        let mic = icon(Indicator::Listening, 44);
        assert!(alpha(&mic, 22, 14) > 200, "capsule");
        assert_eq!(alpha(&mic, 2, 2), 0);
    }

    #[test]
    fn menu_lines() {
        let t = |_: SystemTime| "10:01".to_string();
        let mut s = Status::default();
        assert_eq!(info_line(&s, &t), "Not recording");
        assert_eq!(toggle_label(&s), "Start recording…");
        s.picking = true;
        assert_eq!(info_line(&s, &t), "Pick a window or screen to record…");
        assert_eq!(toggle_label(&s), "Cancel picking");
        s = Status {
            recording: true,
            session_id: Some("20261019-100103".into()),
            started_at: Some(SystemTime::now()),
            events: 1,
            skip: Some(Skip::NotPicked),
            ..Default::default()
        };
        assert_eq!(info_line(&s, &t), "Recording since 10:01 — 1 event — 20261019-100103");
        assert_eq!(toggle_label(&s), "Stop recording");
    }
}
