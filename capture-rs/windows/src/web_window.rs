// context-capsule — windows: the shared window (ui/, the same page as the Mac's) in WebView2 — served from inside
// the exe (https://capsule.localhost/, no network), its bridge (core::bridge) answered off the UI thread, and the
// app's events pushed to it (state, appearance, material, data-changed, navigate, open-capsule). Windows 11 shows
// Mica behind the transparent page ("glass"); Windows 10 is flat. The title bar and caption buttons stay Windows'.
//! The only window. Without the WebView2 runtime (some Windows 10 PCs) a plain message says where to get it. The
//! tray, hotkeys, picker and dialogs stay native.

use context_capsule_core::window::Section;

/// Mica (the system backdrop) needs Windows 11 22H2.
pub const MICA_BUILD: u32 = 22621;
/// The page's address (wry maps the `capsule` scheme to https://capsule.localhost/ on WebView2).
pub const ORIGIN: &str = "https://capsule.localhost/";

/// Where to get WebView2: Microsoft's small installer for the Evergreen runtime.
pub const WEBVIEW2_LINK: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
/// Said when the window can't open because WebView2 is missing (or broken).
pub const NEEDS_WEBVIEW2: &str = "Context Capsule's window needs Microsoft Edge WebView2, which isn't working on this PC. \
It's free from Microsoft:\n\nhttps://go.microsoft.com/fwlink/p/?LinkId=2124703\n\nRecording and the tray menu work without it.";
pub const DOWNLOAD: &str = "Download";
pub const NOT_NOW: &str = "Not now";

/// The glass look: Windows 11 22H2 or later with transparency effects on (Personalize "EnableTransparency"; missing
/// = on).
pub fn glass(build: u32, enable_transparency: Option<u32>) -> bool {
    build >= MICA_BUILD && enable_transparency != Some(0)
}

/// Control Panel → Region "iTime": "0" = 12-hour clock, "1" = 24-hour. None: unknown.
pub fn hour12_from(i_time: Option<&str>) -> Option<bool> {
    match i_time.map(str::trim) {
        Some("0") => Some(true),
        Some("1") => Some(false),
        _ => None,
    }
}

/// Region "iFirstDayOfWeek" (0 = Monday … 6 = Sunday) as the page's 1 = Sunday … 7 = Saturday.
pub fn first_weekday_from(i_first: Option<&str>) -> Option<u8> {
    let d: u8 = i_first?.trim().parse().ok().filter(|d| *d <= 6)?;
    Some((d + 1) % 7 + 1)
}

/// May the page go to `url`? Only its own files.
pub fn may_navigate(url: &str) -> bool {
    url.starts_with(ORIGIN) || url == "about:blank"
}

/// The name shown for an exe on the Never record list: "slack.exe" → "slack".
pub fn exe_name(id: &str) -> String {
    let lower = id.to_ascii_lowercase();
    if lower.ends_with(".exe") { id[..id.len() - 4].to_string() } else { id.to_string() }
}

/// For testing (`--show timeline 2026-10-23 <recording>`): the page's clock on that day (evening) and the first
/// recording open, as the page's URL query ("?now=2026-10-23T19:00:00&open=first").
pub fn show_query(date: Option<chrono::NaiveDate>, open: Option<&str>) -> String {
    let mut q = vec![];
    if let Some(d) = date {
        q.push(format!("now={}T19:00:00", d.format("%Y-%m-%d")));
    }
    if open.is_some() {
        q.push("open=first".to_string());
    }
    if q.is_empty() { String::new() } else { format!("?{}", q.join("&")) }
}

/// The page id for a section.
pub fn page_of(section: Section) -> &'static str {
    section.id()
}

/// Windows is dark for apps when "AppsUseLightTheme" is 0 (missing: light, as on Windows 10 before dark mode).
pub fn system_dark_from(apps_use_light_theme: Option<u32>) -> bool {
    apps_use_light_theme == Some(0)
}

/// For testing: `--show <page> [YYYY-MM-DD] [<recording id>]` opens the window at launch on that page (the
/// Timeline on that day, with that recording open), like the Mac's `--show-…` flags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowArg {
    pub section: Section,
    pub date: Option<chrono::NaiveDate>,
    pub open: Option<String>,
}

pub fn show_arg(args: &[String]) -> Option<ShowArg> {
    let i = args.iter().position(|a| a == "--show")?;
    let page = args.get(i + 1)?;
    let section = Section::ALL.into_iter().find(|s| page.eq_ignore_ascii_case(s.title()))?;
    let mut show = ShowArg { section, date: None, open: None };
    for a in &args[i + 2..] {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(a, "%Y-%m-%d") {
            show.date = Some(d);
        } else if a.len() == 15 && chrono::NaiveDateTime::parse_from_str(a, "%Y%m%d-%H%M%S").is_ok() {
            show.open = Some(a.clone());
        } else {
            break;
        }
    }
    Some(show)
}

/// A Windows long-date pattern without its year: "dddd, d MMMM yyyy" → "dddd, d MMMM",
/// "dddd, MMMM d, yyyy" → "dddd, MMMM d", "yyyy年M月d日" → "M月d日".
pub fn without_year(pattern: &str) -> String {
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        if c == 'y' {
            while chars.peek() == Some(&'y') {
                chars.next();
            }
            if matches!(chars.peek(), Some('年') | Some('년')) {
                chars.next();
            }
            continue;
        }
        out.push(c);
    }
    out.trim_matches(|c: char| c == ' ' || c == ',' || c == '.' || c == '/' || c == '-').replace(" ,", ",").replace("  ", " ")
}

#[cfg(windows)]
pub use win::*;

#[cfg(windows)]
mod win {
    use super::*;
    use crate::app::{Ui, UiCmd};
    use crate::{ai, hotkeys, setup, win, windows_list};
    use context_capsule_core::anyai::Launch;
    use context_capsule_core::bridge::{self, Hello, Host, Transport};
    use context_capsule_core::settings::Appearance;
    use context_capsule_core::{AppLog, Hotkeys, Msg, Paths, Status};
    use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
    use serde_json::{json, Map, Value};
    use std::cell::RefCell;
    use std::num::NonZeroIsize;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc::Sender;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Instant, SystemTime};
    use windows::core::{w, BOOL, HSTRING, PCSTR, PCWSTR, PWSTR};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Dwm::{DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
    use windows::Win32::UI::Controls::MARGINS;
    use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, TrackPopupMenu, MF_CHECKED, MF_SEPARATOR, MF_STRING, TPM_LEFTALIGN, TPM_RETURNCMD,
        TPM_TOPALIGN,
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, KillTimer, LoadCursorW, RegisterClassW,
        SendMessageW, SetForegroundWindow, SetTimer, SetWindowPos, ShowWindow, SystemParametersInfoW, HICON, ICON_BIG, ICON_SMALL,
        IDC_ARROW, MINMAXINFO, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOZORDER, SW_RESTORE, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
        WINDOW_EX_STYLE, WS_EX_NOREDIRECTIONBITMAP, WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_SETFOCUS, WM_SETICON, WM_SETTINGCHANGE,
        WM_SIZE, WM_TIMER, WNDCLASSW, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW,
    };
    use wry::dpi::{PhysicalPosition, PhysicalSize};
    use wry::{DragDropEvent, Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

    const CLASS: &str = "ContextCapsuleWebWindow";
    /// Files that changed outside (the MCP saved a capsule, made a project…) are looked for this often.
    const WATCH: usize = 1;
    const WATCH_EVERY_MS: u32 = 2000;
    /// While recording, `data-changed: sessions` at most this often.
    const SESSIONS_EVERY: std::time::Duration = std::time::Duration::from_secs(4);
    const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    const DWMWA_USE_IMMERSIVE_DARK_MODE: i32 = 20;
    const DWMWA_SYSTEMBACKDROP_TYPE: i32 = 38;
    const DWMSBT_MAINWINDOW: i32 = 2;

    /// What the window needs from the app.
    #[derive(Clone)]
    pub struct Ctx {
        pub ui: Ui,
        pub tx: Sender<Msg>,
        pub log: AppLog,
        pub paths: Paths,
    }

    struct Web {
        hwnd: HWND,
        webview: WebView,
        status: Status,
        dark: bool,
        glass: bool,
        mica: bool,
        sessions_sent: Instant,
        events_sent: usize,
        seen: Signature,
        icons: Vec<HICON>,
    }

    thread_local! {
        static WEB: RefCell<Option<Web>> = const { RefCell::new(None) };
        /// WebView2's data folder; lives for the app's life.
        static CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
    }

    /// `--show`'s URL query for the first page load.
    static QUERY: Mutex<String> = Mutex::new(String::new());

    /// For testing: the first load shows this (`show_query`).
    pub fn set_query(q: String) {
        *QUERY.lock().unwrap() = q;
    }

    /// The bridge's host, shared with the worker threads that answer the page.
    static HOST: OnceLock<Arc<WinHost>> = OnceLock::new();

    fn with_web<R>(f: impl FnOnce(&mut Web) -> R) -> Option<R> {
        WEB.with(|w| w.try_borrow_mut().ok().and_then(|mut w| w.as_mut().map(f)))
    }

    /// Is the WebView2 runtime here? (Built into Windows 11; most Windows 10 PCs have it through Edge.)
    pub fn runtime() -> Option<String> {
        let mut out = PWSTR::null();
        // SAFETY: asks the WebView2 loader for the installed runtime's version; it allocates `out` (CoTaskMem).
        unsafe {
            webview2_com::Microsoft::Web::WebView2::Win32::GetAvailableCoreWebView2BrowserVersionString(PCWSTR::null(), &mut out).ok()?;
            if out.is_null() {
                return None;
            }
            let v = out.to_string().ok();
            windows::Win32::System::Com::CoTaskMemFree(Some(out.0 as *const _));
            v.filter(|v| !v.is_empty())
        }
    }

    /// Windows' build number (22631…).
    pub fn build() -> u32 {
        win::reg_string_machine(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber")
            .and_then(|b| b.trim().parse().ok())
            .unwrap_or(0)
    }

    fn glass_now() -> bool {
        glass(build(), win::reg_dword(PERSONALIZE, "EnableTransparency"))
    }

    fn system_dark() -> bool {
        system_dark_from(win::reg_dword(PERSONALIZE, "AppsUseLightTheme"))
    }

    /// The window's look: the Appearance setting, else Windows'.
    fn dark_for(paths: &Paths) -> bool {
        Appearance::load(&paths.config()).is_dark(system_dark())
    }

    /// Popup menus (the tray's and the window's) in the window's light or dark look. Windows has no public switch
    /// for menus: this uses uxtheme's SetPreferredAppMode (ordinal 135, Windows 10 1903 and later), like other apps.
    pub fn apply_menu_mode(paths: &Paths) {
        let dark = dark_for(paths);
        if build() < 18362 {
            return;
        }
        // SAFETY: uxtheme is a system DLL; ordinals 135 (SetPreferredAppMode(int) -> int) and 136 (FlushMenuThemes())
        // have these signatures on every build from 1903 on.
        unsafe {
            let Ok(lib) = LoadLibraryW(w!("uxtheme.dll")) else { return };
            if let Some(set) = GetProcAddress(lib, PCSTR(135 as *const u8)) {
                let set: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(set);
                set(if dark { 2 } else { 3 }); // ForceDark / ForceLight
            }
            if let Some(flush) = GetProcAddress(lib, PCSTR(136 as *const u8)) {
                let flush: unsafe extern "system" fn() = std::mem::transmute(flush);
                flush();
            }
        }
    }

    /// "Choose…" → an app that isn't open right now.
    const BROWSE: &str = "Find an app…";

    /// A popup menu of `items` ("-" = a line; `checked` marks one) at `at`. The chosen index, if any.
    fn menu(hwnd: HWND, items: &[String], checked: Option<usize>, at: POINT) -> Option<usize> {
        // SAFETY: a popup menu we build, show and destroy here (TrackPopupMenu runs a nested loop; nothing is borrowed).
        unsafe {
            let m = CreatePopupMenu().ok()?;
            for (i, item) in items.iter().enumerate() {
                if item == "-" {
                    let _ = AppendMenuW(m, MF_SEPARATOR, 0, None);
                } else {
                    let flags = if checked == Some(i) { MF_STRING | MF_CHECKED } else { MF_STRING };
                    let _ = AppendMenuW(m, flags, i + 1, &HSTRING::from(crate::tray::menu_text(item)));
                }
            }
            let chosen = TrackPopupMenu(m, TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN, at.x, at.y, None, hwnd, None).0 as usize;
            let _ = DestroyMenu(m);
            chosen.checked_sub(1)
        }
    }

    /// Apps with a window open now (exe file name, display name), for "Never record".
    fn open_apps(listed: &[String]) -> Vec<(String, String)> {
        let own = std::env::current_exe().ok().and_then(|e| e.file_name().map(|f| f.to_string_lossy().into_owned())).unwrap_or_default();
        let mut apps = windows_list::Apps::default();
        let mut out: Vec<(String, String)> = vec![];
        for raw in windows_list::on_screen() {
            if raw.title.trim().is_empty() {
                continue;
            }
            let (Some(exe), name) = apps.info(raw.pid) else { continue };
            let same = |a: &str| a.eq_ignore_ascii_case(&exe);
            if same(&own) || windows_list::is_shell_ui(&raw.class, &exe) || listed.iter().any(|l| same(l)) || out.iter().any(|(e, _)| same(e)) {
                continue;
            }
            out.push((exe, name));
        }
        out.sort_by_key(|(_, n)| n.to_lowercase());
        out
    }

    /// Without WebView2 there is no window: say so, and offer Microsoft's download.
    fn needs_webview2() {
        if crate::input::ask_choice(context_capsule_core::window::APP_NAME, NEEDS_WEBVIEW2, DOWNLOAD, NOT_NOW) {
            win::open(WEBVIEW2_LINK);
        }
    }

    /// Open the window (on `section`), or bring it to the front. Without WebView2, a message says where to get it.
    pub fn show(section: Option<Section>, ctx: Ctx, status: &Status) {
        if let Some(hwnd) = with_web(|w| w.hwnd) {
            if let Some(s) = section {
                emit("navigate", json!({ "page": page_of(s) }));
            }
            // SAFETY: our own window.
            unsafe {
                let _ = ShowWindow(hwnd, SW_RESTORE);
                let _ = SetForegroundWindow(hwnd);
            }
            return;
        }
        if runtime().is_none() {
            ctx.log.write("window: no WebView2 runtime");
            needs_webview2();
            return;
        }
        let first_run = !setup::seen();
        setup::mark_seen();
        let host = HOST.get_or_init(|| Arc::new(WinHost::new(&ctx))).clone();
        *host.start.lock().unwrap() = Start { page: section.map(|s| page_of(s).to_string()).or(first_run.then(|| "settings".into())), first_run };
        let mica = build() >= MICA_BUILD;
        let Some(hwnd) = create(mica) else {
            ctx.log.write("window: cannot open");
            return;
        };
        let dark = dark_for(&ctx.paths);
        frame(hwnd, dark, mica);
        let webview = match make_webview(hwnd, &ctx, host, dark, mica) {
            Ok(w) => w,
            Err(e) => {
                ctx.log.write(&format!("window: WebView2 failed ({e})"));
                // SAFETY: our own window, never shown.
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                needs_webview2();
                return;
            }
        };
        let mut web = Web {
            hwnd,
            webview,
            status: status.clone(),
            dark,
            glass: glass_now(),
            mica,
            sessions_sent: Instant::now(),
            events_sent: status.events,
            seen: Signature::of(&ctx.paths),
            icons: vec![],
        };
        set_icons(&mut web);
        WEB.with(|w| *w.borrow_mut() = Some(web));
        fit();
        let glass = with_web(|w| w.glass).unwrap_or(false);
        ctx.log.write(&format!("window: open (shared), WebView2 {}, Windows build {}, mica {mica}, glass {glass}", runtime().unwrap_or_default(), build()));
        // SAFETY: a timer on our own window; showing it.
        unsafe {
            SetTimer(Some(hwnd), WATCH, WATCH_EVERY_MS, None);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
    }

    fn make_webview(hwnd: HWND, ctx: &Ctx, host: Arc<WinHost>, dark: bool, mica: bool) -> wry::Result<WebView> {
        let data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Context Capsule").join("WebView2");
        CONTEXT.with(|c| {
            let mut c = c.borrow_mut();
            let context = c.get_or_insert_with(|| WebContext::new(Some(data)));
            let paths = ctx.paths.clone();
            let ui = ctx.ui.clone();
            let builder = WebViewBuilder::new_with_web_context(context)
                .with_url(format!("{ORIGIN}index.html{}", std::mem::take(&mut *QUERY.lock().unwrap())))
                .with_custom_protocol("capsule".into(), |_, req| {
                    let path = req.uri().path().to_string();
                    let resp = match bridge::asset(&path) {
                        Some((bytes, mime)) => wry::http::Response::builder()
                            .header("Content-Type", mime)
                            .header("Cache-Control", "no-store")
                            .body(std::borrow::Cow::Borrowed(bytes)),
                        None => wry::http::Response::builder().status(404).body(std::borrow::Cow::Borrowed(&b""[..])),
                    };
                    resp.unwrap_or_else(|_| wry::http::Response::new(std::borrow::Cow::Borrowed(&b""[..])))
                })
                .with_https_scheme(true)
                .with_initialization_script(bridge::shim_js(Transport::Ipc))
                .with_ipc_handler(move |req| {
                    let body = req.into_body();
                    let (paths, host, ui) = (paths.clone(), host.clone(), ui.clone());
                    std::thread::spawn(move || {
                        if let Some(reply) = bridge::handle(&paths, &*host, &body) {
                            ui.post(UiCmd::Web(reply));
                        }
                    });
                })
                .with_navigation_handler(|url| may_navigate(&url))
                .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
                .with_drag_drop_handler({
                    let ui = ctx.ui.clone();
                    move |e| {
                        if let DragDropEvent::Drop { paths, .. } = e {
                            if let Some(p) = paths.into_iter().find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md"))) {
                                let text = std::fs::read(&p).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                                let msg = bridge::event_json("open-capsule", json!({ "path": p.to_string_lossy(), "text": text }));
                                ui.post(UiCmd::Web(msg));
                            }
                        }
                        true
                    }
                })
                .with_devtools(cfg!(debug_assertions))
                .with_hotkeys_zoom(false)
                .with_browser_accelerator_keys(false)
                .with_default_context_menus(false)
                .with_bounds(bounds(hwnd));
            let builder = if mica {
                builder.with_transparent(true)
            } else if dark {
                builder.with_background_color((0x1e, 0x1e, 0x1e, 0xff))
            } else {
                builder.with_background_color((0xff, 0xff, 0xff, 0xff))
            };
            builder.build_as_child(&Parent(hwnd))
        })
    }

    struct Parent(HWND);

    impl HasWindowHandle for Parent {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let raw = NonZeroIsize::new(self.0 .0 as isize).ok_or(HandleError::Unavailable)?;
            // SAFETY: our own window, alive while the webview is built.
            Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(Win32WindowHandle::new(raw))) })
        }
    }

    fn bounds(hwnd: HWND) -> Rect {
        let mut r = RECT::default();
        // SAFETY: our own window.
        unsafe {
            let _ = GetClientRect(hwnd, &mut r);
        }
        Rect {
            position: PhysicalPosition::new(0, 0).into(),
            size: PhysicalSize::new((r.right - r.left).max(1) as u32, (r.bottom - r.top).max(1) as u32).into(),
        }
    }

    fn fit() {
        with_web(|w| {
            let _ = w.webview.set_bounds(bounds(w.hwnd));
        });
    }

    /// The title bar light or dark; on Windows 11, Mica behind the client area (the page is transparent).
    fn frame(hwnd: HWND, dark: bool, mica: bool) {
        let on: BOOL = dark.into();
        // SAFETY: documented DWM attributes on our own window; older Windows refuse the ones they don't know.
        unsafe {
            let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(DWMWA_USE_IMMERSIVE_DARK_MODE), &on as *const BOOL as *const _, 4);
            if mica {
                let kind = DWMSBT_MAINWINDOW;
                let _ = DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(DWMWA_SYSTEMBACKDROP_TYPE), &kind as *const i32 as *const _, 4);
                let all = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
                let _ = DwmExtendFrameIntoClientArea(hwnd, &all);
            }
        }
        apply_menu_mode(&Paths::standard());
    }

    fn set_icons(w: &mut Web) {
        // SAFETY: our own window.
        let scale = unsafe { GetDpiForWindow(w.hwnd) }.max(96) as f64 / 96.0;
        for (kind, size) in [(ICON_SMALL, 16.0), (ICON_BIG, 32.0)] {
            let px = (size * scale).round() as i32;
            if let Some(icon) = win::app_icon(px).or_else(|| crate::tray::window_icon(px as usize)) {
                // SAFETY: our own window; the icon lives until the window is destroyed.
                unsafe {
                    SendMessageW(w.hwnd, WM_SETICON, Some(WPARAM(kind as usize)), Some(LPARAM(icon.0 as isize)));
                }
                w.icons.push(icon);
            }
        }
    }

    /// `mica`: no GDI surface (WS_EX_NOREDIRECTIONBITMAP), so the backdrop shows wherever the page is transparent.
    fn create(mica: bool) -> Option<HWND> {
        static REGISTER: std::sync::Once = std::sync::Once::new();
        // SAFETY: registering our class once and creating a top-level window of it.
        unsafe {
            let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).ok()?;
            let class = HSTRING::from(CLASS);
            REGISTER.call_once(|| {
                let wc = WNDCLASSW {
                    lpfnWndProc: Some(proc),
                    hInstance: instance.into(),
                    lpszClassName: PCWSTR(class.as_ptr()),
                    hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                    ..Default::default()
                };
                RegisterClassW(&wc);
            });
            let scale = GetDpiForSystem().max(96) as f64 / 96.0;
            let (ww, wh) = ((context_capsule_core::window::START_WIDTH * scale) as i32, (context_capsule_core::window::START_HEIGHT * scale) as i32);
            let mut work = RECT::default();
            let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
            let (ww, wh) = (ww.min(work.right - work.left), wh.min(work.bottom - work.top));
            let (x, y) = (work.left + (work.right - work.left - ww) / 2, work.top + (work.bottom - work.top - wh) / 2);
            CreateWindowExW(
                if mica { WS_EX_NOREDIRECTIONBITMAP } else { WINDOW_EX_STYLE(0) },
                &class,
                &HSTRING::from(context_capsule_core::window::APP_NAME),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
                x,
                y,
                ww,
                wh,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .ok()
        }
    }

    extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_SIZE => fit(),
            // The page paints everything; Mica shows through where it is transparent.
            WM_ERASEBKGND => return LRESULT(1),
            WM_GETMINMAXINFO => {
                // SAFETY: lParam is a MINMAXINFO for this message.
                let scale = unsafe { GetDpiForWindow(hwnd) }.max(96) as f64 / 96.0;
                if let Some(info) = unsafe { (lp.0 as *mut MINMAXINFO).as_mut() } {
                    info.ptMinTrackSize = POINT { x: (560.0 * scale) as i32, y: (420.0 * scale) as i32 };
                }
            }
            WM_DPICHANGED => {
                // SAFETY: lParam is the suggested RECT for this message.
                if let Some(r) = unsafe { (lp.0 as *const RECT).as_ref() } {
                    // SAFETY: our own window.
                    unsafe {
                        let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
                    }
                }
                fit();
            }
            WM_SETTINGCHANGE => look_changed(),
            // Keys go to the page.
            WM_SETFOCUS => {
                with_web(|w| {
                    let _ = w.webview.focus();
                });
            }
            WM_TIMER if wp.0 == WATCH => watch(),
            WM_CLOSE => {
                // The webview first, then the window. The app keeps running in the tray.
                let gone = WEB.with(|w| w.try_borrow_mut().ok().and_then(|mut w| w.take()));
                if let Some(web) = gone {
                    drop(web.webview);
                    // SAFETY: our own window and icons.
                    unsafe {
                        let _ = KillTimer(Some(hwnd), WATCH);
                        for i in web.icons {
                            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(i);
                        }
                    }
                }
                // SAFETY: our own window.
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
            WM_DESTROY => {}
            // SAFETY: default handling for everything else.
            _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
        }
        LRESULT(0)
    }

    /// Light / dark or transparency effects changed in Windows (or the Appearance setting): the title bar, the
    /// page's `appearance` and `material`.
    fn look_changed() {
        let paths = Paths::standard();
        let (dark_sys, glass) = (system_dark(), glass_now());
        let dark = dark_for(&paths);
        let changes = with_web(|w| {
            let out = (w.dark != dark, w.glass != glass, w.hwnd, w.mica);
            w.dark = dark;
            w.glass = glass;
            out
        });
        let Some((dark_changed, glass_changed, hwnd, mica)) = changes else { return };
        if dark_changed {
            frame(hwnd, dark, mica);
        }
        emit("appearance", json!({ "dark": dark_sys }));
        if glass_changed {
            emit("material", json!({ "material": bridge::material(glass) }));
        }
    }

    /// What the files looked like last time (modified times), to tell the page what changed outside.
    #[derive(Clone, Copy, PartialEq, Eq, Default)]
    struct Signature {
        capsules: Option<SystemTime>,
        projects: Option<SystemTime>,
        config: Option<SystemTime>,
        sessions: Option<SystemTime>,
    }

    impl Signature {
        fn of(paths: &Paths) -> Signature {
            let m = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
            let newest = |dir: &Path| {
                let own = m(dir);
                std::fs::read_dir(dir).ok().into_iter().flatten().flatten().filter_map(|e| e.metadata().and_then(|m| m.modified()).ok()).chain(own).max()
            };
            Signature { capsules: newest(&paths.capsules()), projects: m(&paths.projects()), config: m(&paths.config()), sessions: m(&paths.sessions()) }
        }
    }

    fn watch() {
        let paths = Paths::standard();
        let now = Signature::of(&paths);
        let Some(was) = with_web(|w| std::mem::replace(&mut w.seen, now)) else { return };
        let mut what = vec![];
        if now.capsules != was.capsules {
            what.push("capsules");
        }
        if now.projects != was.projects {
            what.push("projects");
        }
        if now.config != was.config {
            what.push("config");
            look_changed();
        }
        if now.sessions != was.sessions {
            what.push("sessions");
        }
        if !what.is_empty() {
            emit("data-changed", json!({ "what": what }));
        }
    }

    /// The recorder's state changed: `state`, and while recording `data-changed: sessions` every few seconds.
    pub fn set_status(status: &Status) {
        let sessions = with_web(|w| {
            w.status = status.clone();
            let due = status.recording && status.events != w.events_sent && w.sessions_sent.elapsed() >= SESSIONS_EVERY;
            if due {
                w.events_sent = status.events;
                w.sessions_sent = Instant::now();
            }
            due
        });
        let Some(sessions) = sessions else { return };
        send_state();
        if sessions {
            emit("data-changed", json!({ "what": ["sessions"] }));
        }
    }

    fn send_state() {
        let several = windows_list::monitors().len() > 1;
        if let Some(state) = with_web(|w| bridge::state_json(&w.status, several, "", false, false)) {
            emit("state", state);
        }
    }

    /// The AI apps or Your AI changed outside the page (tray, a connect that finished).
    pub fn clients_changed() {
        emit("data-changed", json!({ "what": ["clients"] }));
    }

    /// Send an event to the page (UI thread).
    pub fn emit(event: &str, payload: Value) {
        deliver(&bridge::event_json(event, payload));
    }

    /// An answer or event for the page (UI thread).
    pub fn deliver(json: &str) {
        with_web(|w| {
            let _ = w.webview.evaluate_script(&format!("window.capsuleHost&&window.capsuleHost._receive({json})"));
        });
    }

    /// The app is quitting.
    pub fn close() {
        if let Some(hwnd) = with_web(|w| w.hwnd) {
            // SAFETY: our own window.
            unsafe {
                SendMessageW(hwnd, WM_CLOSE, None, None);
            }
        }
    }

    // MARK: the bridge's host

    #[derive(Default)]
    struct Start {
        page: Option<String>,
        first_run: bool,
    }

    struct WinHost {
        ui: Ui,
        tx: Mutex<Sender<Msg>>,
        log: AppLog,
        start: Mutex<Start>,
        launch: OnceLock<Option<Launch>>,
    }

    impl WinHost {
        fn new(ctx: &Ctx) -> WinHost {
            WinHost { ui: ctx.ui.clone(), tx: Mutex::new(ctx.tx.clone()), log: ctx.log.clone(), start: Mutex::default(), launch: OnceLock::new() }
        }

        fn send(&self, msg: Msg) -> Result<(), String> {
            self.tx.lock().unwrap().send(msg).map_err(|_| "Context Capsule is quitting.".to_string())
        }

        /// Run `f` on the UI thread (dialogs, menus, the clipboard, hotkeys) and wait for it.
        fn on_ui<R: Send + 'static>(&self, f: impl FnOnce() -> R + Send + 'static) -> Option<R> {
            let (tx, rx) = std::sync::mpsc::channel();
            self.ui.post(UiCmd::Run(Box::new(move || {
                let _ = tx.send(f());
            })));
            rx.recv().ok()
        }

        fn window(&self) -> Option<HWND> {
            // Only read on the UI thread; workers pass the handle as a number.
            self.on_ui(|| with_web(|w| w.hwnd.0 as usize)).flatten().map(|h| HWND(h as *mut core::ffi::c_void))
        }
    }

    fn registry(subkey: &str, value: &str) -> Option<String> {
        win::reg_string(subkey, value)
    }

    impl Host for WinHost {
        fn hello(&self) -> Hello {
            let start = std::mem::take(&mut *self.start.lock().unwrap());
            // `state` once after hello (the page keeps the latest for when it listens).
            self.on_ui(send_state);
            Hello {
                platform: "windows",
                locale: crate::app::user_locale(),
                hour12: hour12_from(registry(r"Control Panel\International", "iTime").as_deref()),
                first_weekday: first_weekday_from(registry(r"Control Panel\International", "iFirstDayOfWeek").as_deref()),
                user_name: win::display_name(),
                dark: system_dark(),
                first_run: start.first_run,
                page: start.page,
                glass: glass_now(),
            }
        }

        fn config_changed(&self, key: &str, value: &Value) {
            if key == "appearance" {
                self.on_ui(look_changed);
            }
            // Your AI, for the tray and the evening reminder.
            if key == "ai" {
                let id = value.as_str().unwrap_or("").to_string();
                self.on_ui(move || {
                    ai::with(|s| {
                        s.choice = id;
                        s.day_line = None;
                        s.changed();
                    })
                });
            }
        }

        fn recorder(&self, action: &str, id: Option<&str>) -> Result<(), String> {
            match action {
                "toggle" => self.send(Msg::ToggleRecord),
                "voice" => self.send(Msg::ToggleVoice),
                "add-screen" => self.send(Msg::AddScreen),
                "project" => self.send(Msg::PickProject(id.map(String::from))),
                _ => {
                    let tray = self.ui.hwnd().0 as usize;
                    self.on_ui(move || crate::app::new_project_now(HWND(tray as *mut _)));
                    Ok(())
                }
            }
        }

        fn set_hotkeys(&self, keys: Hotkeys) -> Result<(), String> {
            let ui = self.ui.clone();
            self.on_ui(move || match hotkeys::apply(ui.hwnd(), keys) {
                Ok(()) => {
                    crate::app::shortcuts_changed(keys);
                    Ok(())
                }
                Err(_) => Err("taken".to_string()),
            })
            .unwrap_or(Err("taken".into()))
        }

        fn listen_keys(&self, on: bool) {
            let ui = self.ui.clone();
            self.on_ui(move || if on { hotkeys::suspend(ui.hwnd()) } else { hotkeys::resume(ui.hwnd()) });
        }

        fn setup_status(&self) -> Vec<(&'static str, bool)> {
            let d = setup::done();
            vec![("voice", d.voice), ("ai", d.claude), ("login", d.login)]
        }

        fn setup_do(&self, id: &str) -> Result<(), String> {
            let step = match id {
                "voice" => setup::Step::Voice,
                "ai" => setup::Step::Claude,
                "login" => setup::Step::Login,
                _ => return Err(format!("no step {id} here")),
            };
            let log = self.log.clone();
            self.on_ui(move || setup::act(step, &log));
            Ok(())
        }

        fn mcp(&self) -> Option<Launch> {
            let log = self.log.clone();
            self.launch.get_or_init(|| crate::program::launch(&|l: &str| log.write(l))).clone()
        }

        fn auto(&self) -> Option<Arc<context_capsule_core::auto::Auto>> {
            crate::auto::get()
        }

        fn ai_open(&self, id: &str, prompt: &str) -> Result<bool, String> {
            if id == "claude" {
                // Blocking (it may ask): this worker thread is its own.
                crate::reminder::ask_claude(&self.ui, &self.log, prompt, "the window");
                return Ok(true);
            }
            self.clipboard_write(prompt)?;
            Ok(false)
        }

        fn clipboard_read(&self) -> Result<String, String> {
            Ok(self.on_ui(win::clipboard_text).flatten().unwrap_or_default())
        }

        fn clipboard_write(&self, text: &str) -> Result<(), String> {
            let text = text.to_string();
            if self.on_ui(move || win::copy_text(&text)) == Some(true) {
                Ok(())
            } else {
                Err("Windows didn't let Context Capsule use the clipboard. Please try again.".into())
            }
        }

        fn copy_file(&self, path: &Path) -> Result<(), String> {
            let path = path.to_path_buf();
            if self.on_ui(move || win::copy_file(&path)) == Some(true) {
                Ok(())
            } else {
                Err("Couldn't copy the file. Try Copy text.".into())
            }
        }

        fn open_url(&self, url: &str) -> Result<(), String> {
            if win::open(url) {
                Ok(())
            } else {
                Err("Couldn't open the link.".into())
            }
        }

        fn open_file(&self, path: &Path) -> Result<(), String> {
            let file = path.to_string_lossy();
            if win::open(&file) || win::open_with("notepad.exe", Some(&format!("\"{file}\""))) {
                Ok(())
            } else {
                Err("Couldn't open the capsule. Try Copy text, then paste it anywhere.".into())
            }
        }

        fn reveal(&self, path: &Path) -> Result<(), String> {
            if win::show_in_folder(path) {
                Ok(())
            } else {
                Err("Couldn't show it in File Explorer.".into())
            }
        }

        fn save_panel(&self, name: &str) -> Option<PathBuf> {
            let owner = self.window()?.0 as usize;
            let name = name.to_string();
            self.on_ui(move || win::save_dialog(HWND(owner as *mut _), &name)).flatten()
        }

        fn pick_capsule(&self) -> Option<PathBuf> {
            let owner = self.window()?.0 as usize;
            self.on_ui(move || win::open_capsule_dialog(HWND(owner as *mut _))).flatten()
        }

        fn pick_app(&self) -> Option<(String, String)> {
            let owner = self.window()?.0 as usize;
            self.on_ui(move || {
                let hwnd = HWND(owner as *mut _);
                let listed = context_capsule_core::Config::load(&Paths::standard().config()).exclude_apps;
                let apps = open_apps(&listed);
                let mut labels: Vec<String> = apps.iter().map(|(_, n)| n.clone()).collect();
                if !labels.is_empty() {
                    labels.push("-".into());
                }
                labels.push(BROWSE.into());
                let mut at = POINT::default();
                // SAFETY: reads the pointer position.
                unsafe {
                    let _ = GetCursorPos(&mut at);
                }
                let i = menu(hwnd, &labels, None, at)?;
                if let Some((exe, name)) = apps.get(i) {
                    return Some((exe.clone(), name.clone()));
                }
                let exe = win::open_app_dialog(hwnd)?.file_name()?.to_string_lossy().into_owned();
                Some((exe.clone(), exe_name(&exe)))
            })
            .flatten()
        }

        fn app_names(&self, ids: &[String]) -> Map<String, Value> {
            ids.iter().map(|id| (id.clone(), Value::String(exe_name(id)))).collect()
        }

        fn app_icon(&self, app: &str, bundle: &str) -> Option<Vec<u8>> {
            crate::app_icon::icon(app, bundle)
        }

        fn updates(&self, action: &str) -> Result<(), String> {
            if action == "check" {
                crate::update::check_now();
                Ok(())
            } else {
                Err("There's no update waiting.".into())
            }
        }

        fn emit(&self, event: &str, payload: Value) {
            self.ui.post(UiCmd::Web(bridge::event_json(event, payload)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webview2_message_has_the_link() {
        assert!(NEEDS_WEBVIEW2.contains(WEBVIEW2_LINK));
        assert!(NEEDS_WEBVIEW2.contains("Microsoft Edge WebView2"));
    }

    #[test]
    fn show_flag() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(show_arg(&a(&["cc.exe"])), None);
        assert_eq!(show_arg(&a(&["cc.exe", "--show", "nowhere"])), None);
        assert_eq!(show_arg(&a(&["cc.exe", "--show", "help"])), Some(ShowArg { section: Section::Help, date: None, open: None }));
        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 19);
        assert_eq!(
            show_arg(&a(&["cc.exe", "--show", "Timeline", "2026-10-19", "20261019-100103"])),
            Some(ShowArg { section: Section::Timeline, date: day, open: Some("20261019-100103".into()) })
        );
        assert_eq!(show_arg(&a(&["cc.exe", "--show", "timeline", "--login", "2026-10-19"])).unwrap().date, None);
    }

    #[test]
    fn dark_and_dates() {
        assert!(system_dark_from(Some(0)));
        assert!(!system_dark_from(Some(1)));
        assert!(!system_dark_from(None), "Windows 10 before dark mode");
        assert_eq!(without_year("dddd, d MMMM yyyy"), "dddd, d MMMM");
        assert_eq!(without_year("dddd, MMMM d, yyyy"), "dddd, MMMM d");
        assert_eq!(without_year("yyyy年M月d日"), "M月d日");
        assert_eq!(without_year("dddd d. MMMM yyyy"), "dddd d. MMMM");
    }

    #[test]
    fn glass_only_on_windows_11_with_transparency() {
        assert!(glass(22631, None));
        assert!(glass(22621, Some(1)));
        assert!(!glass(22631, Some(0)), "transparency effects off");
        assert!(!glass(22000, Some(1)), "Windows 11 21H2: no Mica backdrop type");
        assert!(!glass(19045, Some(1)), "Windows 10");
    }

    #[test]
    fn region_settings() {
        assert_eq!(hour12_from(Some("0")), Some(true));
        assert_eq!(hour12_from(Some("1")), Some(false));
        assert_eq!(hour12_from(None), None);
        assert_eq!(first_weekday_from(Some("0")), Some(2), "Monday");
        assert_eq!(first_weekday_from(Some("6")), Some(1), "Sunday");
        assert_eq!(first_weekday_from(Some("5")), Some(7), "Saturday");
        assert_eq!(first_weekday_from(Some("9")), None);
    }

    #[test]
    fn small_rules() {
        assert!(may_navigate("https://capsule.localhost/index.html#home"));
        assert!(!may_navigate("https://capsule.localhost.evil.com/"));
        assert!(!may_navigate("https://chatgpt.com/"));
        assert!(!may_navigate("file:///C:/Windows/win.ini"));
        assert_eq!(exe_name("slack.exe"), "slack");
        assert_eq!(exe_name("Code.EXE"), "Code");
        assert_eq!(exe_name("zoom"), "zoom");
        assert_eq!(page_of(Section::Settings), "settings");
        let d = chrono::NaiveDate::from_ymd_opt(2026, 10, 23);
        assert_eq!(show_query(d, Some("20261023-112957")), "?now=2026-10-23T19:00:00&open=first");
        assert_eq!(show_query(d, None), "?now=2026-10-23T19:00:00");
        assert_eq!(show_query(None, None), "");
    }
}
