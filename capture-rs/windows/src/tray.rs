// context-capsule — windows: the tray icon (the Karya panther idle — white on a dark taskbar, black on a light
// one, drawn for the taskbar's real icon size so Windows never scales it; a red filled circle recording, a red
// mic with bars that move with the voice while listening, drawn in code) and the words of its menu and tooltip. The tray can't show text next to its icon, so the running time is in the tooltip
// and the menu's first line. Nothing on screen.

use context_capsule_core::Hotkeys;
use context_capsule_core::look::{self, LEVEL_FRAMES};
use context_capsule_core::{Indicator, Status};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    /// The panther (idle, picking).
    Mark,
    Disc,
    /// The mic with bars, at one of `LEVEL_FRAMES` loudness steps.
    Bars(u8),
}

impl Glyph {
    /// The icon for an indicator; listening shows the bars at `level` (a `look::level_frame`).
    pub fn for_indicator(i: Indicator, level: usize) -> Glyph {
        match i {
            Indicator::Idle | Indicator::Picking => Glyph::Mark,
            Indicator::Recording => Glyph::Disc,
            Indicator::Listening => Glyph::Bars(level.min(LEVEL_FRAMES - 1) as u8),
        }
    }

    /// Every icon the tray needs.
    pub fn all() -> Vec<Glyph> {
        let mut all = vec![Glyph::Mark, Glyph::Disc];
        all.extend((0..LEVEL_FRAMES as u8).map(Glyph::Bars));
        all
    }

    /// Is (x, y) — 0…1 across the icon, top left origin — inside the recording dot?
    fn covers(x: f64, y: f64) -> bool {
        ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() <= 0.40
    }
}

/// Red while recording or listening, so it's obvious at a glance (Mac parity).
pub const RED: [u8; 3] = [0xE8, 0x11, 0x23];

/// Idle: white on a dark taskbar, near-black on a light one. Recording and listening: red.
pub fn glyph_color(glyph: Glyph, light_taskbar: bool) -> [u8; 3] {
    match glyph {
        Glyph::Disc | Glyph::Bars(_) => RED,
        Glyph::Mark if light_taskbar => [0x20, 0x20, 0x20],
        Glyph::Mark => [0xFF, 0xFF, 0xFF],
    }
}

/// A `size`×`size` BGRA image (straight alpha, top row first). The panther is drawn for this exact size
/// (`mark::panther`); the dot is anti-aliased by 4×4 supersampling.
pub fn glyph_pixels(glyph: Glyph, size: usize, rgb: [u8; 3]) -> Vec<u8> {
    let bgra = |alpha: Vec<u8>| alpha.into_iter().flat_map(|a| [rgb[2], rgb[1], rgb[0], a]).collect();
    match glyph {
        Glyph::Bars(frame) => return bgra(look::mic_bars(size, frame as usize)),
        Glyph::Mark => return bgra(context_capsule_core::mark::panther(size)),
        Glyph::Disc => {}
    }
    let mut out = Vec::with_capacity(size * size * 4);
    for py in 0..size {
        for px in 0..size {
            let mut hits = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let x = (px as f64 + (sx as f64 + 0.5) / 4.0) / size as f64;
                    let y = (py as f64 + (sy as f64 + 0.5) / 4.0) / size as f64;
                    hits += Glyph::covers(x, y) as u32;
                }
            }
            let alpha = (hits * 255 / 16) as u8;
            out.extend_from_slice(&[rgb[2], rgb[1], rgb[0], alpha]);
        }
    }
    out
}

/// The menu's changing lines (Mac parity).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuModel {
    pub info: String,
    pub skip: Option<String>,
    pub toggle: String,
    pub voice: String,
    /// "Add a screen" while recording (None: hidden), and whether it can be clicked.
    pub add_screen: Option<(String, bool)>,
    /// "Project: …", the title of the project submenu.
    pub project: String,
}

/// The menu as of `now` (the running time).
/// `keys`: the shortcuts in use now.
pub fn menu_model(s: &Status, now: SystemTime, short_time: &dyn Fn(SystemTime) -> String, keys: &Hotkeys) -> MenuModel {
    let (record, voice) = (keys.record.label(), keys.voice.label());
    let info = match (&s.session_id, s.started_at) {
        (Some(_), Some(at)) if s.recording => {
            let n = s.events;
            let time = look::elapsed(s, now).unwrap_or_default();
            format!("● Contextualizing {time} — since {} — {n} moment{}", short_time(at), if n == 1 { "" } else { "s" })
        }
        _ if s.picking => "Pick a window or screen…".into(),
        _ => "Not contextualizing".into(),
    };
    let skip = s.skip.filter(|_| s.recording).map(|k| format!("Skipping: {}", k.message()));
    let toggle = if s.recording {
        format!("Stop contextualizing\t{record}")
    } else if s.picking {
        format!("Cancel\t{record}")
    } else {
        format!("Start contextualizing\t{record}")
    };
    let voice = if s.listening { format!("Done speaking\t{voice}") } else { format!("Speak a note\t{voice}") };
    let add_screen = s.recording.then(|| (s.add_screen_label(), s.can_add_screen()));
    MenuModel { info, skip, toggle, voice, add_screen, project: menu_text(&s.project_title()) }
}

/// Menu text shows "&" as a shortcut marker: double it so names show as typed.
pub fn menu_text(s: &str) -> String {
    s.replace('&', "&&")
}

/// The hover text as of `now` (at most 127 characters, the tray's limit). While recording it shows the time.
pub fn tooltip(s: &Status, now: SystemTime, keys: &Hotkeys) -> String {
    let (record, voice) = (keys.record.label(), keys.voice.label());
    let t = if s.listening {
        format!("Listening — press {voice} to finish the voice note")
    } else if let Some(time) = look::elapsed(s, now) {
        format!("● Contextualizing {time} — {record} to stop, {voice} to talk")
    } else {
        format!("{} ({record} to start contextualizing)", context_capsule_core::window::APP_NAME)
    };
    t.chars().take(127).collect()
}

#[cfg(windows)]
pub use win::{window_icon, Tray};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::win::reg_dword;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::core::w;
    use windows::Win32::Graphics::Gdi::{
        CreateBitmap, CreateDIBSection, DeleteObject, MonitorFromWindow, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        MONITOR_DEFAULTTOPRIMARY,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, GetSystemMetricsForDpi, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER, NOTIFYICON_VERSION_4,
    };
    use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, FindWindowW, HICON, ICONINFO, SM_CXSMICON};

    const ID: u32 = 1;
    const THEME_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

    pub struct Tray {
        hwnd: HWND,
        callback: u32,
        icons: Vec<(Glyph, HICON)>,
        indicator: Indicator,
        /// The listening step (`look::level_frame`).
        level: usize,
        tip: String,
    }

    impl Tray {
        pub fn new(hwnd: HWND, callback: u32) -> Tray {
            let mut tray = Tray { hwnd, callback, icons: vec![], indicator: Indicator::Idle, level: 0, tip: String::new() };
            tray.make_icons();
            tray
        }

        /// (Re)draw the icons for the current taskbar theme and DPI: at the small-icon size of the taskbar's
        /// monitor (16 px at 100 %, 20 at 125 %, 24 at 150 %, 32 at 200 % …), so Windows shows them unscaled.
        pub fn make_icons(&mut self) {
            self.destroy_icons();
            let light = reg_dword(THEME_KEY, "SystemUsesLightTheme") == Some(1);
            // SAFETY: plain metrics query.
            let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, taskbar_dpi(self.hwnd)) }.max(16) as usize;
            for glyph in Glyph::all() {
                if let Some(icon) = make_icon(&glyph_pixels(glyph, size, glyph_color(glyph, light)), size) {
                    self.icons.push((glyph, icon));
                }
            }
        }

        fn destroy_icons(&mut self) {
            for (_, icon) in self.icons.drain(..) {
                // SAFETY: icons we created.
                unsafe {
                    let _ = DestroyIcon(icon);
                }
            }
        }

        fn data(&self) -> NOTIFYICONDATAW {
            let mut d = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: ID,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
                uCallbackMessage: self.callback,
                ..Default::default()
            };
            let glyph = Glyph::for_indicator(self.indicator, self.level);
            d.hIcon = self.icons.iter().find(|(g, _)| *g == glyph).map(|(_, i)| *i).unwrap_or_default();
            for (dst, src) in d.szTip.iter_mut().zip(self.tip.encode_utf16().take(127)) {
                *dst = src;
            }
            d
        }

        /// Add the icon (also after Explorer restarts).
        pub fn add(&mut self) {
            let mut d = self.data();
            // SAFETY: a fully initialised NOTIFYICONDATAW for our window.
            unsafe {
                if !Shell_NotifyIconW(NIM_ADD, &d).as_bool() {
                    eprintln!("Context Capsule: cannot add the tray icon");
                }
                d.Anonymous.uVersion = NOTIFYICON_VERSION_4;
                let _ = Shell_NotifyIconW(NIM_SETVERSION, &d);
            }
        }

        fn update(&self) {
            // SAFETY: as in `add`.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &self.data());
            }
        }

        pub fn set_indicator(&mut self, indicator: Indicator) {
            self.indicator = indicator;
            self.update();
        }

        /// While listening: show the bars for this loudness step (only redrawn when the step changes).
        pub fn set_level(&mut self, level: usize) {
            if level != self.level {
                self.level = level;
                if self.indicator == Indicator::Listening {
                    self.update();
                }
            }
        }

        pub fn set_tip(&mut self, tip: String) {
            if tip != self.tip {
                self.tip = tip;
                self.update();
            }
        }

        pub fn retheme(&mut self) {
            self.make_icons();
            self.update();
        }

        pub fn remove(&mut self) {
            // SAFETY: as in `add`.
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &self.data());
            }
            self.destroy_icons();
        }

        /// Where the icon is on screen (for toasts).
        pub fn rect(&self) -> Option<RECT> {
            let id = NOTIFYICONIDENTIFIER {
                cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: self.hwnd,
                uID: ID,
                ..Default::default()
            };
            // SAFETY: reads our own icon's rect.
            unsafe { Shell_NotifyIconGetRect(&id) }.ok()
        }
    }

    /// The DPI of the monitor the taskbar is on (where the tray icon shows); else our window's.
    fn taskbar_dpi(hwnd: HWND) -> u32 {
        // SAFETY: plain window and monitor queries; the out values are ours.
        unsafe {
            if let Ok(bar) = FindWindowW(w!("Shell_TrayWnd"), None) {
                let monitor = MonitorFromWindow(bar, MONITOR_DEFAULTTOPRIMARY);
                let (mut x, mut y) = (0, 0);
                if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y).is_ok() && x > 0 {
                    return x;
                }
            }
            GetDpiForWindow(hwnd).max(96)
        }
    }

    /// The bare mark as a window icon: only when the exe has no panther tile inside (`win::app_icon`).
    pub fn window_icon(size: usize) -> Option<HICON> {
        let light = reg_dword(THEME_KEY, "SystemUsesLightTheme") == Some(1);
        make_icon(&glyph_pixels(Glyph::Mark, size, glyph_color(Glyph::Mark, light)), size)
    }

    /// A 32-bit icon with alpha from BGRA pixels.
    fn make_icon(pixels: &[u8], size: usize) -> Option<HICON> {
        let header = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size as i32,
            biHeight: -(size as i32), // top row first
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };
        let info = BITMAPINFO { bmiHeader: header, ..Default::default() };
        let mut bits = std::ptr::null_mut();
        // SAFETY: a DIB section of size×size×4 bytes; we copy exactly that many in; GDI objects are
        // deleted once the icon holds its own copies.
        unsafe {
            let color = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, size * size * 4);
            let mask = CreateBitmap(size as i32, size as i32, 1, 1, None);
            let icon = CreateIconIndirect(&ICONINFO { fIcon: true.into(), hbmMask: mask, hbmColor: color, ..Default::default() });
            let _ = DeleteObject(color.into());
            let _ = DeleteObject(mask.into());
            icon.ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use context_capsule_core::Skip;
    use std::time::Duration;

    fn alpha(px: &[u8], size: usize, x: usize, y: usize) -> u8 {
        px[(y * size + x) * 4 + 3]
    }

    #[test]
    fn glyph_shapes() {
        let s = 32;
        let mark = glyph_pixels(Glyph::Mark, s, [255, 255, 255]);
        let disc = glyph_pixels(Glyph::Disc, s, [255, 255, 255]);
        let mic = glyph_pixels(Glyph::Bars(0), s, [255, 255, 255]);
        assert_eq!(mark.len(), s * s * 4);
        let panther = context_capsule_core::mark::panther(s);
        assert!((0..s * s).all(|i| mark[i * 4 + 3] == panther[i]), "idle: the panther, drawn for this size");
        assert_eq!(alpha(&mark, s, 0, 0), 0, "corners clear");
        assert_eq!(alpha(&disc, s, 16, 16), 255, "filled in the middle");
        assert_eq!(alpha(&mic, s, 9, 9), 255, "mic body");
        assert_eq!(alpha(&mic, s, 24, 16), 255, "a bar");
        assert_eq!(alpha(&mic, s, 0, 0), 0);
        let loud = glyph_pixels(Glyph::Bars(3), s, [255, 255, 255]);
        assert!(alpha(&loud, s, 24, 6) > alpha(&mic, s, 24, 6), "louder: taller bars");
        let idle = glyph_pixels(Glyph::Mark, 16, glyph_color(Glyph::Mark, true));
        let px = |p: &[u8], x: usize, y: usize| p[(y * 16 + x) * 4..(y * 16 + x) * 4 + 3].to_vec();
        assert_eq!(px(&idle, 8, 8), [0x20, 0x20, 0x20], "idle: dark panther on a light taskbar (BGR)");
        assert!((0..256).all(|i| [0, 255].contains(&idle[i * 4 + 3])), "16 px: crisp, on or off");
        let at150 = glyph_pixels(Glyph::Mark, 24, [255, 255, 255]);
        assert!(at150.chunks(4).filter(|p| p[3] == 255).count() > 24 * 24 / 4, "150 %: its own 24 px drawing");
        let rec = glyph_pixels(Glyph::Disc, 16, glyph_color(Glyph::Disc, false));
        assert_eq!(px(&rec, 8, 8), [0x23, 0x11, 0xE8], "recording: red (BGR)");
        assert_eq!(glyph_color(Glyph::Bars(2), true), RED, "listening: red on any taskbar");
        assert_eq!(glyph_color(Glyph::Mark, false), [0xFF, 0xFF, 0xFF], "idle: white on a dark taskbar");
    }

    #[test]
    fn icons_per_indicator() {
        assert_eq!(Glyph::for_indicator(Indicator::Idle, 2), Glyph::Mark);
        assert_eq!(Glyph::for_indicator(Indicator::Picking, 0), Glyph::Mark);
        assert_eq!(Glyph::for_indicator(Indicator::Recording, 3), Glyph::Disc);
        assert_eq!(Glyph::for_indicator(Indicator::Listening, 2), Glyph::Bars(2));
        assert_eq!(Glyph::for_indicator(Indicator::Listening, 9), Glyph::Bars(3), "clamped");
        assert_eq!(Glyph::all().len(), 2 + LEVEL_FRAMES);
    }

    #[test]
    fn menu_lines() {
        let t = |_: SystemTime| "14:57".to_string();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(755);
        let idle = menu_model(&Status::default(), now, &t, &Hotkeys::DEFAULT);
        assert_eq!(idle.info, "Not contextualizing");
        assert_eq!(idle.toggle, "Start contextualizing\tAlt+C");
        assert_eq!(idle.voice, "Speak a note\tAlt+V");
        assert_eq!(idle.skip, None);

        let picking = Status { picking: true, ..Default::default() };
        assert_eq!(menu_model(&picking, now, &t, &Hotkeys::DEFAULT).info, "Pick a window or screen…");
        assert!(menu_model(&picking, now, &t, &Hotkeys::DEFAULT).toggle.starts_with("Cancel"));
        assert_eq!(idle.add_screen, None, "hidden when not recording");

        let rec = Status {
            recording: true,
            session_id: Some("20261019-145700".into()),
            started_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1)),
            events: 12,
            skip: Some(Skip::NotPicked),
            listening: true,
            sources: 1,
            ..Default::default()
        };
        let m = menu_model(&rec, now, &t, &Hotkeys::DEFAULT);
        assert_eq!(m.info, "● Contextualizing 12:34 — since 14:57 — 12 moments");
        assert_eq!(m.add_screen, Some(("Add a screen".to_string(), true)));
        assert_eq!(m.skip.as_deref(), Some("Skipping: the front window is not in what you picked"));
        assert!(m.toggle.starts_with("Stop contextualizing"));
        assert!(m.voice.starts_with("Done speaking"));
        let one = Status { events: 1, skip: None, sources: 4, ..rec };
        assert_eq!(menu_model(&one, now, &t, &Hotkeys::DEFAULT).info, "● Contextualizing 12:34 — since 14:57 — 1 moment");
        assert_eq!(menu_model(&one, now, &t, &Hotkeys::DEFAULT).skip, None);
        assert_eq!(menu_model(&one, now, &t, &Hotkeys::DEFAULT).add_screen, Some(("Add a screen (4 shared)".to_string(), false)), "4 at most");

        assert_eq!(idle.project, "Project: Auto-detect");

        let stale = Status { skip: Some(Skip::Overlay), ..Default::default() };
        assert_eq!(menu_model(&stale, now, &t, &Hotkeys::DEFAULT).skip, None, "skip line only while recording");
    }

    #[test]
    fn project_menu_text() {
        let p = context_capsule_core::Project { id: "rd".into(), name: "R&D".into(), clues: vec![] };
        let s = Status { projects: vec![p], pinned: Some("rd".into()), project: Some("rd".into()), ..Default::default() };
        assert_eq!(menu_model(&s, SystemTime::now(), &|_| String::new(), &Hotkeys::DEFAULT).project, "Project: R&&D");
        assert_eq!(menu_text("a && b"), "a &&&& b");
    }

    #[test]
    fn tooltips_fit() {
        let start = SystemTime::UNIX_EPOCH;
        let rec = Status { recording: true, started_at: Some(start), ..Default::default() };
        for s in [Status::default(), rec.clone(), Status { listening: true, ..Default::default() }] {
            let tip = tooltip(&s, start + Duration::from_secs(36_000), &Hotkeys::DEFAULT);
            assert!(tip.chars().count() <= 127 && tip.contains("Alt+"), "{tip}");
        }
        assert_eq!(tooltip(&Status::default(), start, &Hotkeys::DEFAULT), "Karya Context Capsule (Alt+C to start contextualizing)");
        assert_eq!(
            tooltip(&rec, start + Duration::from_secs(754), &Hotkeys::DEFAULT),
            "● Contextualizing 12:34 — Alt+C to stop, Alt+V to talk"
        );
        let mine = Hotkeys::DEFAULT.with(context_capsule_core::shortcuts::Which::Voice, context_capsule_core::Shortcut::parse("Ctrl+Shift+N").unwrap());
        assert_eq!(tooltip(&rec, start + Duration::from_secs(754), &mine), "● Contextualizing 12:34 — Alt+C to stop, Ctrl+Shift+N to talk", "your own shortcut");
        assert_eq!(menu_model(&Status::default(), start, &|_| String::new(), &mine).voice, "Speak a note\tCtrl+Shift+N");
    }
}
