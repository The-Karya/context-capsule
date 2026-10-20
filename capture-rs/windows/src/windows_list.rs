// context-capsule — windows: the front app and every on-screen window, front to back (EnumWindows + DWM),
// with never-record apps, notification toasts and window overviews (Task View, Alt-Tab) marked.
//! Coordinates are physical pixels (the process is per-monitor DPI aware v2), the same space as
//! monitors and captured frames. UWP apps hosted by ApplicationFrameHost.exe are named after the
//! process that owns their CoreWindow.

/// A UWP app's frame window and the window inside it that belongs to the app itself.
pub const FRAME_CLASS: &str = "ApplicationFrameWindow";
pub const CORE_WINDOW_CLASS: &str = "Windows.UI.Core.CoreWindow";

const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;

/// The desktop, the taskbar and other shell surfaces: on screen, never "the app's window".
const SHELL_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "Xaml_WindowedPopupClass",
];

/// Task View, Alt-Tab and the Windows 11 switcher show every window at once.
const OVERLAY_CLASSES: &[&str] = &[
    "MultitaskingViewFrame",
    "XamlExplorerHostIslandWindow",
    "TaskSwitcherWnd",
    "TaskSwitcherOverlayWnd",
];

/// Windows' own UI that is never "the app you work in": the taskbar, Start, Search, the notification
/// and action centers, Quick Settings, the language and emoji pickers, Alt-Tab and Task View. When
/// one is in front (you clicked the taskbar or the tray), the work window is the top normal window of
/// a real app. File Explorer runs in explorer.exe too, so the taskbar is told apart by class.
const SHELL_UI_CLASSES: &[&str] = &[
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "Xaml_WindowedPopupClass",
    "Shell_InputSwitchTopLevelWindow",
    "ControlCenterWindow",
    "MultitaskingViewFrame",
    "XamlExplorerHostIslandWindow",
    "TaskSwitcherWnd",
    "TaskSwitcherOverlayWnd",
    "ForegroundStaging",
];

/// Processes that only draw Windows' own UI (Start, Search, flyouts, Quick Settings, the emoji panel).
const SHELL_UI_HOSTS: &[&str] = &[
    "startmenuexperiencehost.exe",
    "searchhost.exe",
    "searchapp.exe",
    "searchui.exe",
    "shellexperiencehost.exe",
    "shellhost.exe",
    "textinputhost.exe",
];

/// Is this window part of Windows' own UI (see `SHELL_UI_CLASSES`)? `exe` is the file name.
pub fn is_shell_ui(class: &str, exe: &str) -> bool {
    SHELL_UI_CLASSES.contains(&class) || SHELL_UI_HOSTS.contains(&exe.to_ascii_lowercase().as_str())
}

/// The window (id, pid) the user works in. Normally the foreground window; when that is Windows' own
/// UI, the top normal window that isn't (a private app on top is returned, so its sample is skipped).
/// None: only shell UI is on screen.
pub fn work_window(
    foreground: (u64, u32),
    foreground_is_shell: bool,
    list: &[context_capsule_core::ListedWindow],
    is_shell: &dyn Fn(u64) -> bool,
) -> Option<(u64, u32)> {
    if !foreground_is_shell {
        return Some(foreground);
    }
    context_capsule_core::WindowList::top_regular(list, &|w| is_shell(w.id)).map(|w| (w.id, w.pid))
}

/// Processes that draw notification toasts and the notification center.
const NOTIFICATION_HOSTS: &[&str] = &["shellexperiencehost.exe"];

/// 0 = a normal app window; 1 = tool windows, non-activating popups, shell surfaces and top-level
/// CoreWindows (Start, search, flyouts), which are never "the app's window" (like macOS layers > 0).
pub fn layer_for(class: &str, ex_style: u32) -> i32 {
    let tool = ex_style & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) != 0;
    if tool || SHELL_CLASSES.contains(&class) || class == CORE_WINDOW_CLASS {
        1
    } else {
        0
    }
}

pub fn is_overlay_class(class: &str) -> bool {
    OVERLAY_CLASSES.contains(&class)
}

/// A notification toast (it can show any app's messages, so it is never read and is painted out).
pub fn is_notification(class: &str, title: &str, exe: &str) -> bool {
    let exe = exe.to_ascii_lowercase();
    class == CORE_WINDOW_CLASS && (NOTIFICATION_HOSTS.contains(&exe.as_str()) || title == "New notification")
}

/// The name people know an app by: the exe's file description, else its file name without ".exe".
pub fn app_name(description: Option<&str>, exe: &str) -> String {
    if let Some(d) = description.map(str::trim).filter(|d| !d.is_empty()) {
        return d.to_string();
    }
    let stem = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    match stem.len().checked_sub(4) {
        Some(cut) if stem[cut..].eq_ignore_ascii_case(".exe") => stem[..cut].to_string(),
        _ => stem.to_string(),
    }
}

/// The file name of a full exe path.
pub fn exe_file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

#[cfg(windows)]
pub use win::{front, monitors, on_screen, title, Apps};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::picker::MonitorCandidate;
    use context_capsule_core::{Front, ListedWindow, Rect, WindowList};
    use std::collections::{HashMap, HashSet};
    use windows::core::{BOOL, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GetClassNameW, GetForegroundWindow, GetLayeredWindowAttributes, GetWindowLongW,
        GetWindowRect, GetWindowThreadProcessId, InternalGetWindowText, IsIconic, IsWindowVisible,
        GWL_EXSTYLE, LAYERED_WINDOW_ATTRIBUTES_FLAGS, LWA_ALPHA, WS_EX_LAYERED,
    };

    /// One visible top-level window as Windows reports it.
    #[derive(Clone, Debug)]
    pub struct RawWindow {
        pub id: u64,
        /// The app's process (for UWP: the CoreWindow's, not ApplicationFrameHost's).
        pub pid: u32,
        pub class: String,
        pub title: String,
        pub frame: Rect,
        pub ex_style: u32,
        pub alpha: f64,
    }

    pub fn hwnd(id: u64) -> HWND {
        HWND(id as usize as *mut core::ffi::c_void)
    }

    fn id_of(h: HWND) -> u64 {
        h.0 as usize as u64
    }

    /// Visible, not cloaked (other virtual desktops, suspended UWP apps), not minimized; front to back.
    pub fn on_screen() -> Vec<RawWindow> {
        let mut handles: Vec<HWND> = vec![];
        // SAFETY: the callback only pushes into `handles`, which outlives the call.
        unsafe {
            let _ = EnumWindows(Some(collect), LPARAM(&mut handles as *mut Vec<HWND> as isize));
        }
        handles.into_iter().filter_map(describe).collect()
    }

    unsafe extern "system" fn collect(h: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the `&mut Vec<HWND>` passed by `on_screen`/`core_window_pid`.
        let list = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        list.push(h);
        BOOL(1)
    }

    fn describe(h: HWND) -> Option<RawWindow> {
        // SAFETY: read-only queries on a window handle; a stale handle just fails.
        let shown = unsafe { IsWindowVisible(h).as_bool() && !IsIconic(h).as_bool() };
        if !shown || is_cloaked(h) {
            return None;
        }
        let frame = frame_of(h)?;
        if frame.is_empty() {
            return None;
        }
        // SAFETY: as above.
        let ex_style = unsafe { GetWindowLongW(h, GWL_EXSTYLE) } as u32;
        let class = class_name(h);
        let mut pid = pid_of(h);
        if class == FRAME_CLASS {
            pid = core_window_pid(h, pid).unwrap_or(pid);
        }
        Some(RawWindow { id: id_of(h), pid, class, title: title(id_of(h)), frame, ex_style, alpha: alpha_of(h, ex_style) })
    }

    fn is_cloaked(h: HWND) -> bool {
        let mut cloaked = 0u32;
        // SAFETY: DWMWA_CLOAKED writes one u32.
        let ok = unsafe {
            DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, std::mem::size_of::<u32>() as u32)
        };
        ok.is_ok() && cloaked != 0
    }

    /// The visible frame (no invisible resize borders), else the window rect.
    fn frame_of(h: HWND) -> Option<Rect> {
        let mut r = RECT::default();
        // SAFETY: DWMWA_EXTENDED_FRAME_BOUNDS writes one RECT.
        let dwm = unsafe {
            DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut _, std::mem::size_of::<RECT>() as u32)
        };
        // SAFETY: GetWindowRect writes one RECT.
        if dwm.is_err() && unsafe { GetWindowRect(h, &mut r) }.is_err() {
            return None;
        }
        Some(rect(&r))
    }

    pub fn rect(r: &RECT) -> Rect {
        Rect::new(r.left as f64, r.top as f64, (r.right - r.left) as f64, (r.bottom - r.top) as f64)
    }

    fn alpha_of(h: HWND, ex_style: u32) -> f64 {
        if ex_style & WS_EX_LAYERED.0 == 0 {
            return 1.0;
        }
        let (mut alpha, mut flags) = (255u8, LAYERED_WINDOW_ATTRIBUTES_FLAGS(0));
        // SAFETY: writes a byte and a flags value; fails for UpdateLayeredWindow windows (then opaque).
        let ok = unsafe { GetLayeredWindowAttributes(h, None, Some(&mut alpha), Some(&mut flags)) };
        if ok.is_ok() && flags.contains(LWA_ALPHA) { alpha as f64 / 255.0 } else { 1.0 }
    }

    fn class_name(h: HWND) -> String {
        let mut buf = [0u16; 256];
        // SAFETY: writes at most buf.len() chars.
        let n = unsafe { GetClassNameW(h, &mut buf) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    /// The window title, as the taskbar shows it (no permission needed on Windows). Read without
    /// sending the window a message, so a hung app cannot stall a sample.
    pub fn title(id: u64) -> String {
        let mut buf = vec![0u16; 1024];
        // SAFETY: writes at most buf.len() chars.
        let n = unsafe { InternalGetWindowText(hwnd(id), &mut buf) };
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }

    fn pid_of(h: HWND) -> u32 {
        let mut pid = 0u32;
        // SAFETY: writes one u32.
        unsafe { GetWindowThreadProcessId(h, Some(&mut pid)) };
        pid
    }

    /// The UWP app inside an ApplicationFrameWindow: its CoreWindow child from another process.
    fn core_window_pid(frame: HWND, frame_pid: u32) -> Option<u32> {
        let mut children: Vec<HWND> = vec![];
        // SAFETY: the callback only pushes into `children`, which outlives the call.
        unsafe {
            let _ = EnumChildWindows(Some(frame), Some(collect), LPARAM(&mut children as *mut Vec<HWND> as isize));
        }
        children
            .into_iter()
            .filter(|c| class_name(*c) == CORE_WINDOW_CLASS)
            .map(pid_of)
            .find(|p| *p != 0 && *p != frame_pid)
    }

    /// The pid behind the foreground window (resolving UWP frames).
    fn foreground() -> Option<(u64, u32, String)> {
        // SAFETY: no arguments.
        let h = unsafe { GetForegroundWindow() };
        if h.is_invalid() {
            return None;
        }
        let class = class_name(h);
        let pid = pid_of(h);
        let pid = if class == FRAME_CLASS { core_window_pid(h, pid).unwrap_or(pid) } else { pid };
        Some((id_of(h), pid, class))
    }

    /// Exe names and descriptions per process; descriptions are cached by exe path.
    #[derive(Default)]
    pub struct Apps {
        names: HashMap<String, String>,
    }

    impl Apps {
        /// (exe file name, display name). None/"" when the process cannot be opened.
        pub fn info(&mut self, pid: u32) -> (Option<String>, String) {
            let Some(path) = exe_path(pid) else { return (None, String::new()) };
            let exe = exe_file_name(&path).to_string();
            let name = self.names.entry(path.clone()).or_insert_with(|| app_name(file_description(&path).as_deref(), &exe));
            (Some(exe), name.clone())
        }
    }

    fn exe_path(pid: u32) -> Option<String> {
        if pid == 0 {
            return None;
        }
        // SAFETY: a query-only handle, closed below.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: writes at most `len` chars into `buf`.
        let ok = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) };
        // SAFETY: closing the handle we opened.
        unsafe {
            let _ = CloseHandle(process);
        }
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }

    /// The exe's FileDescription ("Google Chrome", "Microsoft Teams").
    fn file_description(path: &str) -> Option<String> {
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let file = PCWSTR(wide.as_ptr());
        // SAFETY: `file` is NUL-terminated; `data` is sized by GetFileVersionInfoSizeW.
        let size = unsafe { GetFileVersionInfoSizeW(file, None) };
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        // SAFETY: as above.
        unsafe { GetFileVersionInfoW(file, None, size, data.as_mut_ptr() as *mut _) }.ok()?;
        let translation = query(&data, "\\VarFileInfo\\Translation")
            .filter(|(_, len)| *len >= 4)
            // SAFETY: at least 4 bytes: a (language, code page) pair of u16s.
            .map(|(p, _)| unsafe { (*(p as *const u16), *(p as *const u16).add(1)) });
        let mut keys = vec![];
        if let Some((lang, cp)) = translation {
            keys.push(format!("\\StringFileInfo\\{lang:04x}{cp:04x}\\FileDescription"));
        }
        keys.push("\\StringFileInfo\\040904b0\\FileDescription".into());
        keys.into_iter().find_map(|k| {
            let (p, len) = query(&data, &k)?;
            // SAFETY: VerQueryValueW returns `len` u16 chars (with the NUL) inside `data`.
            let chars = unsafe { std::slice::from_raw_parts(p as *const u16, len as usize) };
            let s = String::from_utf16_lossy(chars).trim_end_matches('\0').trim().to_string();
            (!s.is_empty()).then_some(s)
        })
    }

    fn query(data: &[u8], key: &str) -> Option<(*const core::ffi::c_void, u32)> {
        let wide: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
        let (mut p, mut len) = (std::ptr::null_mut(), 0u32);
        // SAFETY: `data` is a version-info block; the pointer returned points inside it.
        let ok = unsafe { VerQueryValueW(data.as_ptr() as *const _, PCWSTR(wide.as_ptr()), &mut p, &mut len) };
        (ok.as_bool() && !p.is_null()).then_some((p as *const _, len))
    }

    /// Monitors in physical pixels, with their device names ("\\.\DISPLAY1").
    pub fn monitors() -> Vec<MonitorCandidate> {
        let mut handles: Vec<HMONITOR> = vec![];
        // SAFETY: the callback only pushes into `handles`, which outlives the call.
        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(collect_monitor), LPARAM(&mut handles as *mut Vec<HMONITOR> as isize));
        }
        handles
            .into_iter()
            .filter_map(|m| {
                let mut info = MONITORINFOEXW::default();
                info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
                // SAFETY: MONITORINFOEXW with cbSize set, as GetMonitorInfoW expects.
                unsafe { GetMonitorInfoW(m, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) }.ok().ok()?;
                let end = info.szDevice.iter().position(|c| *c == 0).unwrap_or(info.szDevice.len());
                Some(MonitorCandidate {
                    device: String::from_utf16_lossy(&info.szDevice[..end]),
                    rect: rect(&info.monitorInfo.rcMonitor),
                })
            })
            .collect()
    }

    unsafe extern "system" fn collect_monitor(m: HMONITOR, _: HDC, _: *mut RECT, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the `&mut Vec<HMONITOR>` passed by `monitors`.
        let list = unsafe { &mut *(lparam.0 as *mut Vec<HMONITOR>) };
        list.push(m);
        BOOL(1)
    }

    /// The front app and the window list, for the engine.
    pub fn front(apps: &mut Apps, is_excluded: &dyn Fn(Option<&str>, &str) -> bool) -> Option<Front> {
        let (fg_id, fg_pid, fg_class) = foreground()?;
        let raws = on_screen();
        let mut shell = HashSet::new();
        let list: Vec<ListedWindow> = raws
            .iter()
            .map(|r| {
                let (exe, name) = apps.info(r.pid);
                if is_shell_ui(&r.class, exe.as_deref().unwrap_or("")) {
                    shell.insert(r.id);
                }
                ListedWindow {
                    id: r.id,
                    pid: r.pid,
                    owner: name.clone(),
                    layer: layer_for(&r.class, r.ex_style),
                    alpha: r.alpha,
                    frame: r.frame,
                    name: (!r.title.is_empty()).then(|| r.title.clone()),
                    excluded: is_excluded(exe.as_deref(), &name),
                    notification: is_notification(&r.class, &r.title, exe.as_deref().unwrap_or("")),
                }
            })
            .collect();
        // An overview is in front: it is the foreground window or tops the z-order.
        let overlay = is_overlay_class(&fg_class) || raws.first().is_some_and(|r| is_overlay_class(&r.class));
        // The taskbar, Start, a flyout…: use the window of the app the user works in.
        let fg_exe = apps.info(fg_pid).0.unwrap_or_default();
        let (fg_id, fg_pid) = work_window((fg_id, fg_pid), is_shell_ui(&fg_class, &fg_exe), &list, &|id| shell.contains(&id))?;
        let (app_id, app_name) = apps.info(fg_pid);
        // The foreground window itself when it is a normal one, else the app's front window.
        let window = list
            .iter()
            .find(|w| w.id == fg_id && WindowList::is_normal(w))
            .or_else(|| WindowList::front(fg_pid, &list))
            .cloned();
        Some(Front { app_name, app_id, pid: fg_pid, window, list, overlay })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers() {
        assert_eq!(layer_for("Chrome_WidgetWin_1", 0x0000_0100), 0, "a normal app window");
        assert_eq!(layer_for("ApplicationFrameWindow", 0), 0, "a UWP app frame");
        assert_eq!(layer_for("SomePalette", WS_EX_TOOLWINDOW), 1, "tool window");
        assert_eq!(layer_for("SomePopup", WS_EX_NOACTIVATE), 1, "non-activating popup");
        for shell in ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"] {
            assert_eq!(layer_for(shell, 0), 1, "{shell} is the shell, not an app window");
        }
        assert_eq!(layer_for(CORE_WINDOW_CLASS, 0), 1, "top-level CoreWindows are shell UI");
    }

    #[test]
    fn overlays() {
        for c in ["MultitaskingViewFrame", "XamlExplorerHostIslandWindow", "TaskSwitcherWnd"] {
            assert!(is_overlay_class(c), "{c}");
        }
        assert!(!is_overlay_class("Chrome_WidgetWin_1"));
        assert!(!is_overlay_class("multitaskingviewframe"), "class names are exact");
    }

    #[test]
    fn shell_ui() {
        for class in ["Shell_TrayWnd", "Shell_SecondaryTrayWnd", "XamlExplorerHostIslandWindow", "MultitaskingViewFrame", "ControlCenterWindow"] {
            assert!(is_shell_ui(class, "explorer.exe"), "{class}");
        }
        for exe in ["StartMenuExperienceHost.exe", "SearchHost.exe", "ShellExperienceHost.exe", "ShellHost.exe", "SearchApp.exe"] {
            assert!(is_shell_ui(CORE_WINDOW_CLASS, exe), "{exe}");
        }
        assert!(!is_shell_ui("CabinetWClass", "explorer.exe"), "File Explorer is a real app");
        assert!(!is_shell_ui("Chrome_WidgetWin_1", "chrome.exe"));
        assert!(!is_shell_ui("Progman", "explorer.exe"), "the desktop is not skipped over");
        assert!(!is_shell_ui(CORE_WINDOW_CLASS, "LockApp.exe"), "the lock screen stays in front (and private)");
    }

    #[test]
    fn work_window_skips_shell_ui() {
        use context_capsule_core::{ListedWindow, Rect};
        let w = |id: u64, pid: u32, layer: i32| ListedWindow { layer, ..ListedWindow::new(id, pid, Rect::new(0.0, 0.0, 800.0, 600.0)) };
        let taskbar = w(1, 10, 1);
        let start = w(2, 11, 0); // a top-level CoreWindow reported as normal by mistake
        let whatsapp = ListedWindow { excluded: true, ..w(3, 20, 0) };
        let chrome = w(4, 30, 0);
        let shell = |id: u64| id == 1 || id == 2;
        let list = [taskbar.clone(), start.clone(), chrome.clone()];
        assert_eq!(work_window((4, 30), false, &list, &shell), Some((4, 30)), "a real app in front is kept");
        assert_eq!(work_window((1, 10), true, &list, &shell), Some((4, 30)), "taskbar in front: the top real app");
        assert_eq!(work_window((2, 11), true, &list, &shell), Some((4, 30)), "Start in front: the top real app");
        let with_private = [taskbar.clone(), whatsapp, chrome];
        assert_eq!(work_window((1, 10), true, &with_private, &shell), Some((3, 20)), "a private app on top is kept (then skipped)");
        assert_eq!(work_window((1, 10), true, &[taskbar, start], &shell), None, "only shell UI: none");
    }

    #[test]
    fn notifications() {
        assert!(is_notification(CORE_WINDOW_CLASS, "New notification", "ShellExperienceHost.exe"));
        assert!(is_notification(CORE_WINDOW_CLASS, "Neue Benachrichtigung", "ShellExperienceHost.exe"), "any language");
        assert!(is_notification(CORE_WINDOW_CLASS, "New notification", "SomeOtherHost.exe"));
        assert!(!is_notification(CORE_WINDOW_CLASS, "Start", "StartMenuExperienceHost.exe"));
        assert!(!is_notification("Chrome_WidgetWin_1", "New notification", "chrome.exe"), "a page titled so is not a toast");
    }

    #[test]
    fn names() {
        assert_eq!(app_name(Some("Google Chrome"), "chrome.exe"), "Google Chrome");
        assert_eq!(app_name(Some("  "), "Code.EXE"), "Code");
        assert_eq!(app_name(None, "notepad.exe"), "notepad");
        assert_eq!(app_name(None, "weird"), "weird");
        assert_eq!(exe_file_name(r"C:\Program Files\Google\Chrome\Application\chrome.exe"), "chrome.exe");
        assert_eq!(exe_file_name("chrome.exe"), "chrome.exe");
    }
}
