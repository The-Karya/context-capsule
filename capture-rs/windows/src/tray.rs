// context-capsule — windows: the tray icon (hollow circle idle, filled recording, mic listening, drawn in
// code to match the taskbar theme) and the words of its menu and tooltip.

use crate::hotkeys::{RECORD_LABEL, VOICE_LABEL};
use context_capsule_core::{Indicator, Status};
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Ring,
    Disc,
    Mic,
}

impl Glyph {
    pub fn for_indicator(i: Indicator) -> Glyph {
        match i {
            Indicator::Idle | Indicator::Picking => Glyph::Ring,
            Indicator::Recording => Glyph::Disc,
            Indicator::Listening => Glyph::Mic,
        }
    }

    /// Is (x, y) — 0…1 across the icon, top left origin — inside the shape? `t` = stroke width.
    fn covers(self, x: f64, y: f64, t: f64) -> bool {
        let d = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt();
        match self {
            Glyph::Disc => d <= 0.40,
            Glyph::Ring => d <= 0.40 && d >= 0.40 - t,
            Glyph::Mic => {
                // Capsule body, a U-shaped holder, a stem and a base.
                let (bx, top, bottom, r) = (0.5, 0.10 + 0.11, 0.56 - 0.11, 0.11);
                let cy = y.clamp(top, bottom);
                let body = ((x - bx).powi(2) + (y - cy).powi(2)).sqrt() <= r;
                let hd = ((x - 0.5).powi(2) + (y - 0.48).powi(2)).sqrt();
                let holder = y >= 0.48 && hd <= 0.25 + t / 2.0 && hd >= 0.25 - t / 2.0;
                let stem = (x - 0.5).abs() <= t / 2.0 && (0.72..=0.88).contains(&y);
                let base = (0.34..=0.66).contains(&x) && (0.86..=0.86 + t).contains(&y);
                body || holder || stem || base
            }
        }
    }
}

/// White glyph on a dark taskbar, near-black on a light one.
pub fn glyph_color(light_taskbar: bool) -> [u8; 3] {
    if light_taskbar { [0x20, 0x20, 0x20] } else { [0xFF, 0xFF, 0xFF] }
}

/// A `size`×`size` BGRA image (straight alpha, top row first), anti-aliased by 4×4 supersampling.
pub fn glyph_pixels(glyph: Glyph, size: usize, rgb: [u8; 3]) -> Vec<u8> {
    let t = (1.6 / size as f64).max(0.11);
    let mut out = Vec::with_capacity(size * size * 4);
    for py in 0..size {
        for px in 0..size {
            let mut hits = 0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let x = (px as f64 + (sx as f64 + 0.5) / 4.0) / size as f64;
                    let y = (py as f64 + (sy as f64 + 0.5) / 4.0) / size as f64;
                    hits += glyph.covers(x, y, t) as u32;
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
}

pub fn menu_model(s: &Status, short_time: &dyn Fn(SystemTime) -> String) -> MenuModel {
    let info = match (&s.session_id, s.started_at) {
        (Some(id), Some(at)) if s.recording => {
            let n = s.events;
            format!("Recording since {} — {n} event{} — {id}", short_time(at), if n == 1 { "" } else { "s" })
        }
        _ if s.picking => "Pick a window or screen to record…".into(),
        _ => "Not recording".into(),
    };
    let skip = s.skip.filter(|_| s.recording).map(|k| format!("Skipping: {}", k.message()));
    let toggle = if s.recording {
        format!("Stop recording\t{RECORD_LABEL}")
    } else if s.picking {
        format!("Cancel picking\t{RECORD_LABEL}")
    } else {
        format!("Start recording…\t{RECORD_LABEL}")
    };
    let voice = if s.listening { format!("Finish voice note\t{VOICE_LABEL}") } else { format!("Add voice note\t{VOICE_LABEL}") };
    MenuModel { info, skip, toggle, voice }
}

/// The hover text (at most 127 characters, the tray's limit).
pub fn tooltip(s: &Status) -> String {
    let t = if s.listening {
        format!("Listening — press {VOICE_LABEL} to finish the voice note")
    } else if s.recording {
        format!("Context Capsule is recording ({RECORD_LABEL} to stop, {VOICE_LABEL} for a voice note)")
    } else {
        format!("Context Capsule ({RECORD_LABEL} to record)")
    };
    t.chars().take(127).collect()
}

#[cfg(windows)]
pub use win::Tray;

#[cfg(windows)]
mod win {
    use super::*;
    use crate::win::reg_dword;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS};
    use windows::Win32::UI::HiDpi::GetSystemMetricsForDpi;
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconGetRect, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICONIDENTIFIER, NOTIFYICON_VERSION_4,
    };
    use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, DestroyIcon, HICON, ICONINFO, SM_CXSMICON};

    const ID: u32 = 1;
    const THEME_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

    pub struct Tray {
        hwnd: HWND,
        callback: u32,
        icons: Vec<(Glyph, HICON)>,
        indicator: Indicator,
        tip: String,
    }

    impl Tray {
        pub fn new(hwnd: HWND, callback: u32) -> Tray {
            let mut tray = Tray { hwnd, callback, icons: vec![], indicator: Indicator::Idle, tip: String::new() };
            tray.make_icons();
            tray
        }

        /// (Re)draw the three icons for the current taskbar theme and DPI.
        pub fn make_icons(&mut self) {
            self.destroy_icons();
            let light = reg_dword(THEME_KEY, "SystemUsesLightTheme") == Some(1);
            // SAFETY: plain metrics query.
            let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(self.hwnd) }.max(96);
            // SAFETY: as above.
            let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }.max(16) as usize;
            for glyph in [Glyph::Ring, Glyph::Disc, Glyph::Mic] {
                if let Some(icon) = make_icon(&glyph_pixels(glyph, size, glyph_color(light)), size) {
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
            let glyph = Glyph::for_indicator(self.indicator);
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
        let ring = glyph_pixels(Glyph::Ring, s, [255, 255, 255]);
        let disc = glyph_pixels(Glyph::Disc, s, [255, 255, 255]);
        let mic = glyph_pixels(Glyph::Mic, s, [255, 255, 255]);
        assert_eq!(ring.len(), s * s * 4);
        assert_eq!(alpha(&ring, s, 16, 16), 0, "hollow in the middle");
        assert_eq!(alpha(&disc, s, 16, 16), 255, "filled in the middle");
        assert_eq!(alpha(&ring, s, 16, 4), 255, "the ring's top edge");
        assert_eq!(alpha(&ring, s, 0, 0), 0, "corners clear");
        assert_eq!(alpha(&mic, s, 16, 8), 255, "mic body");
        assert_eq!(alpha(&mic, s, 16, 25), 255, "mic stem");
        assert_eq!(alpha(&mic, s, 3, 3), 0);
        let dark = glyph_pixels(Glyph::Disc, 16, glyph_color(true));
        assert_eq!(&dark[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 3], &[0x20, 0x20, 0x20]);
    }

    #[test]
    fn icons_per_indicator() {
        assert_eq!(Glyph::for_indicator(Indicator::Idle), Glyph::Ring);
        assert_eq!(Glyph::for_indicator(Indicator::Picking), Glyph::Ring);
        assert_eq!(Glyph::for_indicator(Indicator::Recording), Glyph::Disc);
        assert_eq!(Glyph::for_indicator(Indicator::Listening), Glyph::Mic);
    }

    #[test]
    fn menu_lines() {
        let t = |_: SystemTime| "14:57".to_string();
        let idle = menu_model(&Status::default(), &t);
        assert_eq!(idle.info, "Not recording");
        assert_eq!(idle.toggle, "Start recording…\tCtrl+Alt+Shift+C");
        assert_eq!(idle.voice, "Add voice note\tCtrl+Alt+Shift+V");
        assert_eq!(idle.skip, None);

        let picking = Status { picking: true, ..Default::default() };
        assert_eq!(menu_model(&picking, &t).info, "Pick a window or screen to record…");
        assert!(menu_model(&picking, &t).toggle.starts_with("Cancel picking"));

        let rec = Status {
            recording: true,
            session_id: Some("20261019-145700".into()),
            started_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1)),
            events: 12,
            skip: Some(Skip::NotPicked),
            listening: true,
            ..Default::default()
        };
        let m = menu_model(&rec, &t);
        assert_eq!(m.info, "Recording since 14:57 — 12 events — 20261019-145700");
        assert_eq!(m.skip.as_deref(), Some("Skipping: the front window is not in what you picked"));
        assert!(m.toggle.starts_with("Stop recording"));
        assert!(m.voice.starts_with("Finish voice note"));
        let one = Status { events: 1, skip: None, ..rec };
        assert_eq!(menu_model(&one, &t).info, "Recording since 14:57 — 1 event — 20261019-145700");
        assert_eq!(menu_model(&one, &t).skip, None);

        let stale = Status { skip: Some(Skip::Overlay), ..Default::default() };
        assert_eq!(menu_model(&stale, &t).skip, None, "skip line only while recording");
    }

    #[test]
    fn tooltips_fit() {
        for s in [Status::default(), Status { recording: true, ..Default::default() }, Status { listening: true, ..Default::default() }] {
            let tip = tooltip(&s);
            assert!(tip.chars().count() <= 127 && tip.contains("Ctrl+Alt+Shift"));
        }
    }
}
