// context-capsule — windows: a small message near the tray icon that fades by itself ("Voice note saved",
// "Start recording first (Ctrl+Alt+Shift+C)"). A topmost borderless window: no notification permission.

/// Seconds on screen before fading (like the Mac: short for good news, longer for problems).
pub fn seconds_for(text: &str) -> u32 {
    if text == "Voice note saved" { 2 } else { 6 }
}

/// Where a `w`×`h` toast goes: next to the tray icon on the taskbar's side, inside the work area.
/// Rects are (left, top, right, bottom). Without an icon rect: the work area's bottom-right corner.
pub fn origin(icon: Option<(i32, i32, i32, i32)>, work: (i32, i32, i32, i32), w: i32, h: i32, gap: i32) -> (i32, i32) {
    let (wl, wt, wr, wb) = work;
    let clamp_x = |x: i32| x.clamp(wl + gap, (wr - w - gap).max(wl + gap));
    let clamp_y = |y: i32| y.clamp(wt + gap, (wb - h - gap).max(wt + gap));
    let Some((il, it, ir, ib)) = icon else { return (clamp_x(wr - w - gap), clamp_y(wb - h - gap)) };
    let mid = (il + ir) / 2;
    if it >= wb {
        (clamp_x(mid - w / 2), clamp_y(wb - h - gap)) // taskbar at the bottom
    } else if ib <= wt {
        (clamp_x(mid - w / 2), clamp_y(wt + gap)) // at the top
    } else if il >= wr {
        (clamp_x(wr - w - gap), clamp_y((it + ib) / 2 - h / 2)) // on the right
    } else if ir <= wl {
        (clamp_x(wl + gap), clamp_y((it + ib) / 2 - h / 2)) // on the left
    } else {
        (clamp_x(mid - w / 2), clamp_y(it - h - gap)) // in the overflow flyout
    }
}

#[cfg(windows)]
pub use win::Toast;

#[cfg(windows)]
mod win {
    use super::*;
    use std::cell::RefCell;
    use windows::core::w;
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
    use windows::Win32::Graphics::Gdi::{
        BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC, GetMonitorInfoW,
        InvalidateRect, MonitorFromPoint, MonitorFromRect, ReleaseDC, SelectObject, SetBkMode, SetTextColor,
        DEFAULT_CHARSET, DT_CALCRECT, DT_NOPREFIX, DT_WORDBREAK, FONT_QUALITY, HFONT, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
        PAINTSTRUCT, TRANSPARENT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, KillTimer, RegisterClassW, SetLayeredWindowAttributes, SetTimer, SetWindowPos,
        ShowWindow, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WM_PAINT, WM_TIMER, WNDCLASSW,
        WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };

    const HOLD: usize = 1;
    const FADE: usize = 2;
    const OPAQUE: u8 = 240;

    /// Toast state lives with the toast window, on the UI thread.
    struct State {
        text: Vec<u16>,
        font: HFONT,
        pad: i32,
        alpha: u8,
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State { text: vec![], font: HFONT::default(), pad: 12, alpha: OPAQUE });
    }

    pub struct Toast {
        hwnd: HWND,
    }

    impl Toast {
        pub fn new() -> Option<Toast> {
            // SAFETY: registers our class and creates a hidden popup owned by this thread.
            unsafe {
                let instance = GetModuleHandleW(None).ok()?;
                let class = WNDCLASSW {
                    lpfnWndProc: Some(proc),
                    hInstance: instance.into(),
                    lpszClassName: w!("ContextCapsuleToast"),
                    ..Default::default()
                };
                RegisterClassW(&class);
                let hwnd = CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                    w!("ContextCapsuleToast"),
                    w!(""),
                    WS_POPUP,
                    0,
                    0,
                    10,
                    10,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )
                .ok()?;
                let round = DWMWCP_ROUND;
                let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &round as *const _ as *const _, 4);
                Some(Toast { hwnd })
            }
        }

        /// Show `text` near the tray icon for a few seconds, then fade.
        pub fn show(&self, text: &str, icon: Option<RECT>) {
            let seconds = seconds_for(text);
            let anchor = icon.map(|r| POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }).unwrap_or_default();
            // SAFETY: monitor and DPI queries; GDI objects are released or kept in STATE until replaced.
            unsafe {
                let monitor = match icon {
                    Some(r) => MonitorFromRect(&r, MONITOR_DEFAULTTOPRIMARY),
                    None => MonitorFromPoint(anchor, MONITOR_DEFAULTTOPRIMARY),
                };
                let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                let _ = GetMonitorInfoW(monitor, &mut info);
                let (mut dx, mut dy) = (96u32, 96u32);
                let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
                let scale = |v: i32| v * dx as i32 / 96;

                let font = CreateFontW(-scale(14), 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET, Default::default(), Default::default(),
                    FONT_QUALITY(5), 0, w!("Segoe UI"));
                let mut text: Vec<u16> = text.encode_utf16().collect();
                let mut measure = RECT { left: 0, top: 0, right: scale(320), bottom: 0 };
                let dc = GetDC(Some(self.hwnd));
                let old = SelectObject(dc, font.into());
                DrawTextW(dc, &mut text, &mut measure, DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX);
                SelectObject(dc, old);
                ReleaseDC(Some(self.hwnd), dc);

                let pad = scale(12);
                let (w, h) = (measure.right + pad * 2, measure.bottom + pad * 2);
                let work = info.rcWork;
                let icon_rect = icon.map(|r| (r.left, r.top, r.right, r.bottom));
                let (x, y) = origin(icon_rect, (work.left, work.top, work.right, work.bottom), w, h, scale(8));

                STATE.with(|s| {
                    let mut s = s.borrow_mut();
                    if !s.font.is_invalid() {
                        let _ = DeleteObject(s.font.into());
                    }
                    *s = State { text, font, pad, alpha: OPAQUE };
                });
                let _ = KillTimer(Some(self.hwnd), FADE);
                let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), OPAQUE, LWA_ALPHA);
                let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                let _ = InvalidateRect(Some(self.hwnd), None, true);
                SetTimer(Some(self.hwnd), HOLD, seconds * 1000, None);
            }
        }
    }

    extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_PAINT => {
                paint(hwnd);
                LRESULT(0)
            }
            WM_TIMER => {
                tick(hwnd, wp.0);
                LRESULT(0)
            }
            // SAFETY: default handling for everything else.
            _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
        }
    }

    fn paint(hwnd: HWND) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            let mut ps = PAINTSTRUCT::default();
            // SAFETY: standard WM_PAINT drawing into our own window.
            unsafe {
                let dc = BeginPaint(hwnd, &mut ps);
                let mut r = RECT::default();
                let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut r);
                let brush = CreateSolidBrush(COLORREF(0x0020_2020));
                FillRect(dc, &r, brush);
                let _ = DeleteObject(brush.into());
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, COLORREF(0x00FF_FFFF));
                let old = SelectObject(dc, s.font.into());
                let pad = s.pad;
                let mut inner = RECT { left: r.left + pad, top: r.top + pad, right: r.right - pad, bottom: r.bottom - pad };
                DrawTextW(dc, &mut s.text, &mut inner, DT_WORDBREAK | DT_NOPREFIX);
                SelectObject(dc, old);
                let _ = EndPaint(hwnd, &ps);
            }
        });
    }

    /// Hold, then fade out over about half a second.
    fn tick(hwnd: HWND, id: usize) {
        // SAFETY: timers and attributes of our own window.
        unsafe {
            if id == HOLD {
                let _ = KillTimer(Some(hwnd), HOLD);
                SetTimer(Some(hwnd), FADE, 30, None);
                return;
            }
            let alpha = STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.alpha = s.alpha.saturating_sub(16);
                s.alpha
            });
            if alpha == 0 {
                let _ = KillTimer(Some(hwnd), FADE);
                let _ = ShowWindow(hwnd, SW_HIDE);
            } else {
                let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: (i32, i32, i32, i32) = (0, 0, 1920, 1032); // a bottom taskbar 48 px high

    #[test]
    fn durations() {
        assert_eq!(seconds_for("Voice note saved"), 2);
        assert_eq!(seconds_for("Start recording first (Ctrl+Alt+Shift+C)"), 6);
    }

    #[test]
    fn above_a_bottom_taskbar_icon() {
        let (x, y) = origin(Some((1700, 1040, 1724, 1064)), WORK, 300, 60, 8);
        assert_eq!((x, y), (1712 - 150, 1032 - 60 - 8));
    }

    #[test]
    fn kept_on_screen() {
        let (x, _) = origin(Some((1900, 1040, 1920, 1064)), WORK, 300, 60, 8);
        assert_eq!(x, 1920 - 300 - 8, "does not run off the right edge");
        let (x, y) = origin(Some((10, 0, 30, 20)), (0, 40, 1920, 1080), 300, 60, 8);
        assert_eq!((x, y), (8, 48), "top taskbar, left edge");
        let (x, y) = origin(None, WORK, 300, 60, 8);
        assert_eq!((x, y), (1920 - 308, 1032 - 68), "no icon: bottom right");
        let (x, y) = origin(Some((1880, 500, 1900, 520)), (0, 0, 1872, 1080), 300, 60, 8);
        assert_eq!((x, y), (1872 - 308, 480), "right-hand taskbar");
    }
}
