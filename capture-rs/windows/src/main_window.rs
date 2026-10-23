// context-capsule — windows: the Context Capsule window — one place for everything: Home (start / stop, speak a
// note, add a screen, what you're working on, time per project, recent capsules, make today's capsules with your
// AI), Timeline (a day in stretches: day bar, where the time went, recordings to open; search, delete, make a capsule from a recording), Capsules (with Copy my day / Save from clipboard), Projects, Settings (set up, Connect your AI, Your AI and
// preferences) and Help. Opens from the tray ("Open Context Capsule"), and by
// itself on Settings the first time.
//! A plain Win32 window drawn in code with the Mac app's look (its colors and macOS's blue, Segoe UI Variable, light
//! and dark following Windows or the Appearance setting; the title bar stays Windows'). `layout` decides what goes where; `paint` draws it.
//! Text boxes are real edit controls; choices are standard popup menus; questions are standard dialogs.
//! Keyboard: Tab / Shift+Tab between controls, Enter or Space to press, arrows and Page Up / Down to scroll.

use crate::app::{Ui, UiCmd};
use crate::layout::{self, Action, Data, Field, Node, ShareHow, ShareView, TimelineData, TimelineView, View, HEADER, SIDEBAR};
use crate::paint::{self, Faces, Feel, Fonts, GdiMeasure, Theme};
use crate::setup::{self, Done};
use crate::{ai, hotkeys, input, tray, win, windows_list};
use context_capsule_core::settings::{self, Appearance, ConfigFile, ShareScreens};
use context_capsule_core::shortcuts::{self as shortcuts, Hotkeys, Pressed, Which};
use context_capsule_core::timeline as tl;
use context_capsule_core::window::{self as w, HomeNumbers, Keys, Section};
use context_capsule_core::{AppLog, CapsuleSummary, Config, Msg, Paths, Projects, Status, WorkStats};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime};
use windows::core::{w, BOOL, HSTRING, PCSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC, DeleteObject, EndPaint, GetDC,
    IntersectClipRect, InvalidateRect, ReleaseDC, RestoreDC, SaveDC, SelectObject, SetBkColor, SetTextColor, HBRUSH, HDC, PAINTSTRUCT,
    SRCCOPY,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_CONTROL, VK_DOWN, VK_END, VK_ESCAPE,
    VK_HOME, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DestroyWindow,
    GetClientRect, GetWindowTextLengthW, GetWindowTextW, KillTimer, LoadCursorW, MoveWindow, PostMessageW, RegisterClassW, SendMessageW,
    SetForegroundWindow, SetTimer, SetWindowPos, SetWindowTextW, ShowWindow, SystemParametersInfoW, TrackPopupMenu, HICON, HMENU,
    ICON_BIG, ICON_SMALL, IDC_ARROW, MF_CHECKED, MF_SEPARATOR, MF_STRING, MINMAXINFO, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOZORDER,
    SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWNOACTIVATE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_TOPALIGN,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_ACTIVATE, WM_APP, WM_CHAR, WM_CLOSE, WM_COMMAND, WM_CTLCOLOREDIT, WM_DESTROY, WM_DPICHANGED,
    WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_SYSCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT,
    WM_SETFONT, WM_SETICON, WM_SETTINGCHANGE, WM_SIZE, WM_TIMER, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_OVERLAPPEDWINDOW,
};

const CLASS: &str = "ContextCapsuleWindow";
const WM_THEMECHANGED: u32 = 0x031A;
const WM_SYSCOLORCHANGE: u32 = 0x0015;
const WM_DWMCOLORIZATIONCOLORCHANGED: u32 = 0x0320;
const TICK: usize = 1;
const STATS: usize = 2;
/// Home's numbers are read again this often while the window is open.
const STATS_EVERY_MS: u32 = 60_000;
/// Today's Timeline is read again this often (seconds) while recording.
const TIMELINE_EVERY: u32 = 5;
/// "this PC", in the Timeline's delete questions.
const THIS_PC: &str = "this PC";
/// From a text box: Tab (wParam 1 = backwards), Enter, Esc.
const WM_FIELD_TAB: u32 = WM_APP + 20;
const WM_FIELD_ENTER: u32 = WM_APP + 21;
const WM_FIELD_ESCAPE: u32 = WM_APP + 22;
const EN_SETFOCUS: u32 = 0x0100;
const EN_KILLFOCUS: u32 = 0x0200;
const EN_CHANGE: u32 = 0x0300;
const EM_LIMITTEXT: u32 = 0x00C5;
const EM_SETSEL: u32 = 0x00B1;
const ES_AUTOHSCROLL: u32 = 0x0080;
/// A grey hint in an empty text box (Common Controls 6).
const EM_SETCUEBANNER: u32 = 0x1501;
const FIELD_ID: usize = 300;
/// Scroll this far per mouse-wheel notch or arrow key.
const STEP: f64 = 48.0;
const NOTICE_FOR: Duration = Duration::from_millis(2600);

/// What the window needs from the app.
#[derive(Clone)]
pub struct Ctx {
    pub ui: Ui,
    pub tx: Sender<Msg>,
    pub log: AppLog,
    pub paths: Paths,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Region {
    Sidebar,
    Header,
    Content,
}

struct Win {
    hwnd: HWND,
    ctx: Ctx,
    section: Section,
    status: Status,
    data: Data,
    loaded: bool,
    os_name: String,
    setup: Done,
    several: bool,
    placed: Vec<(Node, Region)>,
    content_h: f64,
    column: layout::Rect,
    scroll: f64,
    hover: Option<usize>,
    pressed: Option<usize>,
    focus: Option<usize>,
    keyboard: bool,
    tracking: bool,
    editing: Option<String>,
    fields: HashMap<Field, HWND>,
    setting_text: bool,
    theme: Theme,
    faces: Faces,
    fonts: Fonts,
    field_brush: HBRUSH,
    /// Search boxes sit on the page (their own color).
    search_brush: HBRUSH,
    notice: Option<(String, Instant)>,
    /// Capsules' search box.
    search: String,
    /// "Check before sharing", shown instead of the page while a capsule is about to be shared.
    share: Option<ShareView>,
    /// Dragging the scroll bar: (pointer y in DIPs, scroll) when it started.
    drag: Option<(f64, f64)>,
    scale: f64,
    /// The client area in DIPs.
    size: (f64, f64),
    icons: Vec<HICON>,
    /// Settings → Shortcuts: waiting for new keys for this one (the global shortcuts are off meanwhile).
    recording_keys: Option<Which>,
    /// Why the last new keys were refused, under that row.
    keys_note: Option<(Which, String)>,
    /// The any-AI state's version last laid out (`ai::AiState::version`).
    ai_seen: u64,
    /// Seconds on Settings: the AI apps are asked for again every `AI_EVERY` of them.
    ai_ticks: u32,
    /// The Timeline: the day, its search, what's open; and seconds since it was last read (while recording).
    timeline: TimelineView,
    timeline_ticks: u32,
}

/// While Settings is open, the AI apps are asked for again this often (seconds), as on the Mac.
const AI_EVERY: u32 = 10;

thread_local! {
    static WIN: RefCell<Option<Win>> = const { RefCell::new(None) };
}

/// Run `f` on the window's state, unless it's closed or already in use further up the stack (a nested modal loop).
fn with_win<R>(f: impl FnOnce(&mut Win) -> R) -> Option<R> {
    WIN.with(|s| s.try_borrow_mut().ok().and_then(|mut s| s.as_mut().map(f)))
}

/// Open the window (on `section`, when given), or bring it to the front.
pub fn show(section: Option<Section>, ctx: Ctx, status: &Status) {
    if let Some(hwnd) = with_win(|w| {
        if let Some(s) = section {
            w.go(s);
        }
        w.status = status.clone();
        w.request_load();
        w.rebuild();
        w.hwnd
    }) {
        // SAFETY: our own window.
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
        return;
    }
    paint::start_gdiplus();
    let Some(hwnd) = create() else {
        ctx.log.write("window: cannot open");
        return;
    };
    setup::mark_seen();
    // SAFETY: our own window.
    let scale = unsafe { GetDpiForWindow(hwnd) }.max(96) as f64 / 96.0;
    let faces = Faces::detect();
    let appearance = Appearance::load(&ctx.paths.config());
    let theme = theme_for(appearance);
    let mut state = Win {
        hwnd,
        section: section.unwrap_or(Section::Home),
        status: status.clone(),
        data: Data { config: Config::load(&ctx.paths.config()), appearance, ..Default::default() },
        loaded: false,
        os_name: win::display_name(),
        setup: setup::done(),
        several: windows_list::monitors().len() > 1,
        placed: vec![],
        content_h: 0.0,
        column: layout::Rect::default(),
        scroll: 0.0,
        hover: None,
        pressed: None,
        focus: None,
        keyboard: false,
        tracking: false,
        editing: None,
        fields: HashMap::new(),
        setting_text: false,
        theme,
        fonts: Fonts::new(scale, faces.clone()),
        faces,
        // SAFETY: a brush we delete when the window goes away.
        field_brush: unsafe { CreateSolidBrush(paint::colorref(theme.field)) },
        // SAFETY: as above.
        search_brush: unsafe { CreateSolidBrush(paint::colorref(theme.search)) },
        notice: None,
        search: String::new(),
        share: None,
        drag: None,
        scale,
        size: (0.0, 0.0),
        icons: vec![],
        recording_keys: None,
        keys_note: None,
        ai_seen: 0,
        ai_ticks: 0,
        timeline: TimelineView::new(context_capsule_core::capsules::today()),
        timeline_ticks: 0,
        ctx,
    };
    state.apply_theme();
    state.set_icons();
    state.measure_client();
    WIN.with(|s| *s.borrow_mut() = Some(state));
    with_win(|w| {
        w.request_load();
        w.rebuild();
        w.ctx.log.write("window: open");
        if w.section == Section::Settings {
            ai::refresh(&w.ctx.ui, &w.ctx.log);
        }
    });
    // SAFETY: timers on our own window; showing it.
    unsafe {
        SetTimer(Some(hwnd), TICK, 1000, None);
        SetTimer(Some(hwnd), STATS, STATS_EVERY_MS, None);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// For testing (`--show timeline [day] [recording]`): the Timeline on that day, with that recording open.
pub fn show_day(date: Option<chrono::NaiveDate>, open: Option<&str>) {
    with_win(|w| {
        if let Some(d) = date {
            w.timeline_day(d);
        }
        if let Some(session) = open {
            w.timeline.toggle(session);
            w.rebuild();
        }
    });
}

/// The recorder's state changed (recording, moments, projects…).
pub fn set_status(status: &Status) {
    with_win(|w| {
        let projects_changed = w.status.projects != status.projects;
        w.status = status.clone();
        w.several = windows_list::monitors().len() > 1;
        if projects_changed {
            w.request_load();
        }
        w.rebuild();
    });
}

/// Fresh numbers and lists from the files.
pub fn set_data(data: Data) {
    with_win(|w| {
        let name_was = w.data.config.author.clone();
        w.data = data;
        w.loaded = true;
        if w.focused_field() != Some(Field::Name) && name_was != w.data.config.author {
            w.fill_name();
        }
        w.rebuild();
    });
}

/// A day for the Timeline was read.
pub fn set_timeline(data: TimelineData) {
    with_win(|w| {
        w.timeline.set(data);
        w.rebuild();
    });
}

/// The any-AI state changed (the apps, Your AI, Copy my day…): lay out again. If the window is busy (a menu or
/// dialog is open over it), the next second's tick does it.
pub fn ai_changed() {
    with_win(|w| w.rebuild());
}

/// Capsules were saved (Save from clipboard): read the lists and numbers again.
pub fn reload() {
    with_win(|w| w.request_load());
}

/// The app is quitting.
pub fn close() {
    if let Some(hwnd) = with_win(|w| w.hwnd) {
        // SAFETY: our own window; WM_DESTROY clears the state.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

/// Popup menus (the tray's and the window's) in the window's light or dark look. Windows has no public switch
/// for menus: this uses uxtheme's SetPreferredAppMode (ordinal 135, Windows 10 1903 and later), like other apps.
pub fn apply_menu_mode(paths: &Paths) {
    let dark = theme_for(Appearance::load(&paths.config())).dark;
    let build: u32 = win::reg_string_machine(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber")
        .and_then(|b| b.trim().parse().ok())
        .unwrap_or(0);
    if build < 18362 {
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

/// Read every time (never cached): Windows may switch while the window is open.
fn system_dark() -> bool {
    crate::palette::system_dark_from(win::reg_dword(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "AppsUseLightTheme"))
}

/// The window's colors: light or dark as the setting (or Windows) says, with macOS's blue as the accent (the
/// window looks like the Mac app; Windows' own accent is not used).
fn theme_for(appearance: Appearance) -> Theme {
    Theme::for_appearance(appearance, system_dark(), |_| None)
}

fn create() -> Option<HWND> {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    // SAFETY: registering our class once and creating a top-level window of it.
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = HSTRING::from(CLASS);
        REGISTER.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                lpszClassName: windows::core::PCWSTR(class.as_ptr()),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                ..Default::default()
            };
            RegisterClassW(&wc);
        });
        let scale = GetDpiForSystem().max(96) as f64 / 96.0;
        let (ww, wh) = ((w::START_WIDTH * scale) as i32, (w::START_HEIGHT * scale) as i32);
        let mut work = RECT::default();
        let _ = SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut work as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
        let (ww, wh) = (ww.min(work.right - work.left), wh.min(work.bottom - work.top));
        let (x, y) = (work.left + (work.right - work.left - ww) / 2, work.top + (work.bottom - work.top - wh) / 2);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            &class,
            &HSTRING::from(w::APP_NAME),
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

impl Win {
    fn px(&self, v: f64) -> i32 {
        (v * self.scale).round() as i32
    }

    fn invalidate(&self) {
        // SAFETY: our own window.
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn measure_client(&mut self) {
        let mut r = RECT::default();
        // SAFETY: our own window.
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut r);
        }
        self.size = ((r.right - r.left) as f64 / self.scale, (r.bottom - r.top) as f64 / self.scale);
    }

    fn apply_theme(&mut self) {
        let dark: BOOL = self.theme.dark.into();
        let caption = paint::colorref(self.theme.bg);
        // SAFETY: documented DWM attributes on our own window (20: dark title bar; 35: caption color, Windows 11
        // only — older versions refuse it, which is fine).
        unsafe {
            let _ = DwmSetWindowAttribute(self.hwnd, DWMWINDOWATTRIBUTE(20), &dark as *const BOOL as *const _, 4);
            let _ = DwmSetWindowAttribute(self.hwnd, DWMWINDOWATTRIBUTE(35), &caption as *const COLORREF as *const _, 4);
            let _ = DeleteObject(self.field_brush.into());
            self.field_brush = CreateSolidBrush(paint::colorref(self.theme.field));
            let _ = DeleteObject(self.search_brush.into());
            self.search_brush = CreateSolidBrush(paint::colorref(self.theme.search));
        }
        apply_menu_mode(&self.ctx.paths);
    }

    /// The title bar, taskbar and Alt+Tab icon: the panther (white on its black tile, so it shows on light and dark
    /// title bars); the drawn ring only if the exe has no icon inside.
    fn set_icons(&mut self) {
        for (kind, size) in [(ICON_SMALL, 16.0), (ICON_BIG, 32.0)] {
            let px = (size * self.scale).round() as i32;
            if let Some(icon) = win::app_icon(px).or_else(|| tray::window_icon(px as usize)) {
                // SAFETY: our own window; the icon lives until the window is destroyed.
                unsafe {
                    SendMessageW(self.hwnd, WM_SETICON, Some(WPARAM(kind as usize)), Some(LPARAM(icon.0 as isize)));
                }
                self.icons.push(icon);
            }
        }
    }

    /// Stop waiting for new keys (the shortcuts in use work again).
    fn stop_recording_keys(&mut self) {
        if self.recording_keys.take().is_some() {
            hotkeys::resume(self.ctx.ui.hwnd());
            self.rebuild();
        }
    }

    /// One key press while waiting for new keys: Esc cancels; a whole shortcut is checked, then used and saved,
    /// or refused with a short line (the old one stays).
    fn record_key(&mut self, vk: u32) {
        let Some(which) = self.recording_keys else { return };
        // SAFETY: reads the keyboard state.
        let down = |k: u16| unsafe { GetKeyState(k as i32) } < 0;
        if down(VK_LWIN.0) || down(VK_RWIN.0) {
            return; // the Windows key is Windows' own
        }
        let pressed = shortcuts::pressed(down(VK_CONTROL.0), down(VK_MENU.0), down(VK_SHIFT.0), false, hotkeys::key_of_vk(vk), vk == VK_ESCAPE.0 as u32);
        let to = match pressed {
            Pressed::Wait => return,
            Pressed::Cancel => return self.stop_recording_keys(),
            Pressed::Done(s) => s,
        };
        self.recording_keys = None;
        self.change_shortcuts(Some(which), hotkeys::current().check(which, to, false));
    }

    /// Use new shortcuts (`which`: the row that asked; None: Reset), or say why not.
    fn change_shortcuts(&mut self, which: Option<Which>, checked: Result<Hotkeys, shortcuts::Refusal>) {
        let tray = self.ctx.ui.hwnd();
        let row = which.unwrap_or(Which::Record);
        let result = checked.map_err(|refusal| (row, refusal)).and_then(|new| {
            if new == hotkeys::current() {
                hotkeys::resume(tray);
                return Ok(new);
            }
            hotkeys::apply(tray, new).map(|()| new).map_err(|taken| {
                self.ctx.log.write(&format!("shortcuts: {} is taken by another app", new.get(taken)));
                (which.unwrap_or(taken), shortcuts::Refusal::Taken)
            })
        });
        match result {
            Ok(new) => {
                self.keys_note = None;
                if let Err(e) = new.save(&self.ctx.paths.config()) {
                    self.ctx.log.write(&format!("cannot save the shortcuts: {e}"));
                }
                self.data.config.hotkeys = new;
                crate::app::shortcuts_changed(new);
            }
            Err((row, refusal)) => {
                hotkeys::resume(tray);
                self.keys_note = Some((row, refusal.message(false)));
            }
        }
        self.rebuild();
    }

    fn go(&mut self, s: Section) {
        self.stop_recording_keys();
        if self.share.take().is_some() {
            self.scroll = 0.0;
        }
        if s != self.section {
            self.section = s;
            self.scroll = 0.0;
            self.editing = None;
            self.focus = None;
            self.hover = None;
            if s == Section::Settings {
                self.setup = setup::done();
                self.ai_ticks = 0;
                ai::refresh(&self.ctx.ui, &self.ctx.log);
            }
        }
        if s == Section::Timeline {
            self.request_timeline();
        }
    }

    /// Read the Timeline's day on another thread; the answer comes back as `UiCmd::Timeline`.
    fn request_timeline(&mut self) {
        self.timeline_ticks = 0;
        let (paths, ui, date) = (self.ctx.paths.clone(), self.ctx.ui.clone(), self.timeline.date);
        std::thread::spawn(move || {
            let day = tl::load_day(&paths, date);
            let recorded = tl::days_with_recordings(&paths.sessions());
            ui.post(UiCmd::Timeline(Box::new(TimelineData { date, day, recorded })));
        });
    }

    /// Show another day on the Timeline.
    fn timeline_day(&mut self, date: chrono::NaiveDate) {
        self.timeline.go_to(date);
        self.scroll = 0.0;
        self.request_timeline();
        self.rebuild();
    }

    /// Read the files on another thread; the answer comes back as `UiCmd::WindowData`.
    fn request_load(&self) {
        let (paths, ui, name) = (self.ctx.paths.clone(), self.ctx.ui.clone(), self.os_name.clone());
        std::thread::spawn(move || {
            let data = load(&paths, &name);
            ui.post(UiCmd::WindowData(Box::new(data)));
        });
    }

    fn reload_config(&mut self) {
        self.data.config = Config::load(&self.ctx.paths.config());
        let appearance = Appearance::load(&self.ctx.paths.config());
        if appearance != self.data.appearance {
            self.data.appearance = appearance;
            self.retheme();
        }
        self.rebuild();
    }

    fn retheme(&mut self) {
        self.theme = theme_for(self.data.appearance);
        self.apply_theme();
        for f in self.fields.values() {
            // SAFETY: our own child.
            unsafe {
                let _ = InvalidateRect(Some(*f), None, true);
            }
        }
        self.invalidate();
    }

    /// Lay the window out again (after any change), keeping keyboard focus on the same control.
    fn rebuild(&mut self) {
        let focused = self.focus.and_then(|i| self.placed.get(i)).and_then(|(n, _)| n.action().cloned());
        let date = win::long_date_today();
        let now = SystemTime::now();
        let keys = Keys::windows(&hotkeys::current());
        let ai = ai::snapshot();
        self.ai_seen = ai.version;
        let view = View {
            section: self.section,
            status: &self.status,
            data: &self.data,
            now,
            hour: WorkStats::hour(now),
            date: &date,
            today: context_capsule_core::capsules::today(),
            editing: self.editing.as_deref(),
            setup: self.setup,
            several_screens: self.several,
            voice: cfg!(feature = "voice"),
            auto_update: Some(crate::update::auto_update()),
            keys: &keys,
            search: &self.search,
            share: self.share.as_ref(),
            recording_keys: self.recording_keys,
            keys_note: self.keys_note.as_ref().map(|(which, text)| (*which, text.as_str())),
            ai: &ai,
            timeline: &self.timeline,
        };
        // SAFETY: the screen DC, only for measuring text; released below.
        let dc = unsafe { GetDC(Some(self.hwnd)) };
        let l = {
            let m = GdiMeasure { dc, fonts: RefCell::new(&mut self.fonts) };
            layout::layout(&view, self.size.0, self.size.1, &m)
        };
        // SAFETY: as above.
        unsafe { ReleaseDC(Some(self.hwnd), dc) };
        let mut placed: Vec<(Node, Region)> = l.sidebar.into_iter().map(|n| (n, Region::Sidebar)).collect();
        placed.extend(l.header.into_iter().map(|n| (n, Region::Header)));
        placed.extend(l.content.into_iter().map(|n| (n, Region::Content)));
        self.placed = placed;
        self.content_h = l.height;
        self.column = l.column;
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        self.focus = focused.and_then(|a| self.placed.iter().position(|(n, _)| n.action() == Some(&a)));
        if self.hover.is_some_and(|i| i >= self.placed.len()) {
            self.hover = None;
        }
        self.pressed = None;
        self.place_fields();
        self.invalidate();
    }

    fn view_h(&self) -> f64 {
        (self.size.1 - HEADER).max(0.0)
    }

    fn max_scroll(&self) -> f64 {
        (self.content_h - self.view_h()).max(0.0)
    }

    fn scroll_to(&mut self, y: f64) {
        let y = y.clamp(0.0, self.max_scroll());
        if (y - self.scroll).abs() > 0.1 {
            self.scroll = y;
            self.place_fields();
            self.invalidate();
        }
    }

    /// Where a placed node is in the window, in DIPs.
    fn on_screen(&self, i: usize) -> layout::Rect {
        let (n, region) = &self.placed[i];
        match region {
            Region::Content => n.rect().moved(HEADER - self.scroll),
            _ => n.rect(),
        }
    }

    fn visible(&self, i: usize) -> bool {
        let r = self.on_screen(i);
        self.placed[i].1 != Region::Content || (r.y >= HEADER - 0.5 && r.bottom() <= self.size.1 + 0.5)
    }

    fn controls(&self) -> Vec<usize> {
        self.placed.iter().enumerate().filter(|(_, (n, _))| n.is_control()).map(|(i, _)| i).collect()
    }

    fn hit(&self, x: f64, y: f64) -> Option<usize> {
        self.controls().into_iter().find(|&i| {
            let r = self.on_screen(i);
            r.contains(x, y) && (self.placed[i].1 != Region::Content || y >= HEADER)
        })
    }

    /// The scroll bar's thumb (DIPs), when the content is taller than the window.
    fn thumb(&self) -> Option<layout::Rect> {
        let (view, total) = (self.view_h(), self.content_h);
        if total <= view + 0.5 || view <= 0.0 {
            return None;
        }
        let h = (view * view / total).max(32.0);
        let y = HEADER + (view - h) * (self.scroll / self.max_scroll());
        Some(layout::Rect::new(self.size.0 - 10.0, y, 6.0, h))
    }

    fn fill_name(&mut self) {
        let name = self.data.config.author.clone().unwrap_or_else(|| self.os_name.clone());
        if let Some(h) = self.fields.get(&Field::Name).copied() {
            self.set_field_text(h, &name);
        }
    }

    fn set_field_text(&mut self, h: HWND, text: &str) {
        self.setting_text = true;
        // SAFETY: our own child control.
        unsafe {
            let _ = SetWindowTextW(h, &HSTRING::from(text));
        }
        self.setting_text = false;
    }

    fn field(&mut self, f: Field) -> HWND {
        if let Some(h) = self.fields.get(&f) {
            return *h;
        }
        // SAFETY: a child edit control of our window, subclassed for Tab / Enter / Esc; destroyed with the window.
        let h = unsafe {
            let instance = GetModuleHandleW(None).ok();
            let id = FIELD_ID + Field::ALL.iter().position(|x| *x == f).unwrap_or(0);
            let h = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                WS_CHILD | WINDOW_STYLE(ES_AUTOHSCROLL),
                0,
                0,
                10,
                10,
                Some(self.hwnd),
                Some(HMENU(id as *mut _)),
                instance.map(|i| i.into()),
                None,
            )
            .unwrap_or_default();
            SendMessageW(h, EM_LIMITTEXT, Some(WPARAM(200)), None);
            let hint = match f {
                Field::Search => Some(w::SEARCH_CAPSULES),
                Field::TimelineSearch => Some(tl::SEARCH_DAY),
                _ => None,
            };
            if let Some(hint) = hint.map(HSTRING::from) {
                SendMessageW(h, EM_SETCUEBANNER, Some(WPARAM(1)), Some(LPARAM(hint.as_ptr() as isize)));
            }
            let _ = SetWindowSubclass(h, Some(field_proc), 1, 0);
            h
        };
        self.fields.insert(f, h);
        self.font_fields();
        if f == Field::Name {
            self.fill_name();
        }
        h
    }

    fn font_fields(&mut self) {
        let font = self.fonts.get(layout::Font::Body);
        for h in self.fields.values() {
            // SAFETY: our own child controls.
            unsafe {
                SendMessageW(*h, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
            }
        }
    }

    /// Put each text box where the layout says (hidden when scrolled out of view or not on this page).
    fn place_fields(&mut self) {
        let mut shown: Vec<(Field, layout::Rect)> = vec![];
        for i in 0..self.placed.len() {
            if let (Node::Field { field, .. }, _) = &self.placed[i] {
                if self.visible(i) {
                    shown.push((*field, self.on_screen(i)));
                }
            }
        }
        let line = 20.0;
        for (f, r) in &shown {
            let h = self.field(*f);
            // Search boxes have a magnifier on the left (drawn by `paint`).
            let left = if matches!(f, Field::Search | Field::TimelineSearch) { 28.0 } else { 8.0 };
            let (x, y) = (self.px(r.x + left), self.px(r.y + (r.h - line) / 2.0));
            // SAFETY: our own child control.
            unsafe {
                let _ = MoveWindow(h, x, y, self.px(r.w - left - 12.0), self.px(line), true);
                let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
            }
        }
        for (f, h) in &self.fields {
            if !shown.iter().any(|(s, _)| s == f) {
                // SAFETY: our own child control.
                unsafe {
                    let _ = ShowWindow(*h, SW_HIDE);
                }
            }
        }
    }

    /// The text box with the keyboard focus, if any (asked from Windows: focus changes made while the state is
    /// in use don't reach `proc`).
    fn focused_field(&self) -> Option<Field> {
        // SAFETY: reads this thread's focus window.
        let now = unsafe { GetFocus() };
        self.fields.iter().find(|(_, h)| **h == now).map(|(f, _)| *f)
    }

    fn feel(&self, i: usize) -> Feel {
        let focused = match &self.placed[i].0 {
            Node::Field { field, .. } => self.focused_field() == Some(*field),
            _ => self.keyboard && self.focus == Some(i),
        };
        Feel { hover: self.hover == Some(i), pressed: self.pressed == Some(i) && self.hover == Some(i), focused }
    }

    fn paint(&mut self) {
        let mut ps = PAINTSTRUCT::default();
        let (cw, ch) = (self.px(self.size.0), self.px(self.size.1));
        // SAFETY: standard double-buffered painting into our own memory DC and bitmap, all freed below.
        unsafe {
            let screen = BeginPaint(self.hwnd, &mut ps);
            let mem = CreateCompatibleDC(Some(screen));
            let bmp = CreateCompatibleBitmap(screen, cw.max(1), ch.max(1));
            let old = SelectObject(mem, bmp.into());
            let t = self.theme;
            // The whole window (sidebar, header and page) in the theme's page color: the new bitmap starts black.
            paint::fill_rect(mem, RECT { left: 0, top: 0, right: cw.max(1), bottom: ch.max(1) }, t.bg);

            // The page, scrolled, under the header.
            let saved = SaveDC(mem);
            IntersectClipRect(mem, self.px(SIDEBAR), self.px(HEADER), cw, ch);
            for i in 0..self.placed.len() {
                if self.placed[i].1 != Region::Content {
                    continue;
                }
                let r = self.on_screen(i);
                if r.bottom() < HEADER - 8.0 || r.y > self.size.1 + 8.0 {
                    continue;
                }
                let feel = self.feel(i);
                paint::node(mem, &mut self.fonts, &t, &self.placed[i].0, HEADER - self.scroll, feel);
            }
            let _ = RestoreDC(mem, saved);
            for i in 0..self.placed.len() {
                if self.placed[i].1 == Region::Content {
                    continue;
                }
                let feel = self.feel(i);
                paint::node(mem, &mut self.fonts, &t, &self.placed[i].0, 0.0, feel);
            }
            if let Some(th) = self.thumb() {
                let k = self.scale as f32;
                let wide = self.drag.is_some();
                let p = [(th.x * self.scale) as f32 + if wide { 0.0 } else { 2.0 * k }, (th.y * self.scale) as f32, if wide { 6.0 } else { 3.0 } * k, (th.h * self.scale) as f32];
                paint::Shapes::on(mem).fill(p, p[2] / 2.0, t.text3);
            }
            if let Some((text, _)) = &self.notice {
                let m = GdiMeasure { dc: mem, fonts: RefCell::new(&mut self.fonts) };
                let tw = layout::Measure::text(&m, text, layout::Font::Body, 600.0, 1).0 + 32.0;
                let r = layout::Rect::new(self.column.x + (self.column.w - tw) / 2.0, self.size.1 - 64.0, tw, 36.0);
                let note = Node::Pill { r, text: text.clone(), live: false, icon: None };
                paint::node(mem, &mut self.fonts, &t, &note, 0.0, Feel::default());
            }
            let _ = BitBlt(screen, 0, 0, cw, ch, Some(mem), 0, 0, SRCCOPY);
            SelectObject(mem, old);
            let _ = DeleteObject(bmp.into());
            let _ = DeleteDC(mem);
            let _ = EndPaint(self.hwnd, &ps);
        }
    }

    fn notice(&mut self, text: &str) {
        self.notice = Some((text.to_string(), Instant::now()));
        self.invalidate();
    }

    /// Keyboard focus to the next (or previous) control, scrolling it into view.
    fn tab(&mut self, back: bool) {
        let list = self.controls();
        if list.is_empty() {
            return;
        }
        let at = self.focus.and_then(|f| list.iter().position(|i| *i == f));
        let next = match (at, back) {
            (None, false) => 0,
            (None, true) => list.len() - 1,
            (Some(a), false) => (a + 1) % list.len(),
            (Some(a), true) => (a + list.len() - 1) % list.len(),
        };
        let i = list[next];
        self.focus = Some(i);
        self.keyboard = true;
        if self.placed[i].1 == Region::Content {
            let r = self.placed[i].0.rect();
            if r.y < self.scroll {
                self.scroll_to(r.y - 16.0);
            } else if r.bottom() > self.scroll + self.view_h() {
                self.scroll_to(r.bottom() - self.view_h() + 16.0);
            }
        }
        if let Node::Field { field, .. } = self.placed[i].0 {
            let h = self.field(field);
            // SAFETY: our own child control.
            unsafe {
                let _ = SetFocus(Some(h));
                SendMessageW(h, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
            }
        } else {
            // SAFETY: our own window.
            unsafe {
                let _ = SetFocus(Some(self.hwnd));
            }
        }
        self.invalidate();
    }

    /// What pressing control `i` does, and where to put a menu for it (screen pixels).
    fn press(&self, i: usize) -> Option<(Action, POINT)> {
        let action = self.placed.get(i)?.0.action()?.clone();
        let r = self.on_screen(i);
        let mut pt = POINT { x: self.px(r.x), y: self.px(r.bottom()) };
        // SAFETY: our own window.
        unsafe {
            let _ = ClientToScreen(self.hwnd, &mut pt);
        }
        Some((action, pt))
    }

    fn field_text(&self, f: Field) -> String {
        self.fields.get(&f).map(|h| text_of(*h)).unwrap_or_default()
    }

    fn start_edit(&mut self, id: &str) {
        let Some(p) = self.data.projects.iter().find(|p| p.id == id).cloned() else { return };
        self.editing = Some(id.to_string());
        let (name, clues) = (self.field(Field::ProjectName), self.field(Field::ProjectClues));
        self.set_field_text(name, &p.name);
        self.set_field_text(clues, &p.clues.join(", "));
        self.rebuild();
        // SAFETY: our own child control.
        unsafe {
            let _ = SetFocus(Some(name));
            SendMessageW(name, EM_SETSEL, Some(WPARAM(0)), Some(LPARAM(-1)));
        }
    }

    fn save_edit(&mut self) {
        let Some(id) = self.editing.take() else { return };
        let (name, clues) = (self.field_text(Field::ProjectName), self.field_text(Field::ProjectClues));
        match settings::save_project_edit(&self.ctx.paths.projects(), &id, &name, &clues) {
            Ok(list) => {
                self.ctx.log.write(&format!("projects edited: {} project(s)", list.len()));
                self.data.projects = list;
            }
            Err(e) => self.ctx.log.write(&format!("cannot save projects.json: {e}")),
        }
        // SAFETY: our own window.
        unsafe {
            let _ = SetFocus(Some(self.hwnd));
        }
        self.request_load();
        self.rebuild();
    }

    fn set(&mut self, key: &str, value: Option<serde_json::Value>) {
        if let Err(e) = ConfigFile::set(&self.ctx.paths.config(), key, value) {
            self.ctx.log.write(&format!("cannot save a setting: {e}"));
        }
        self.reload_config();
    }
}

/// Everything the window shows from the files. Runs off the UI thread.
fn load(paths: &Paths, os_name: &str) -> Data {
    let now = SystemTime::now();
    Data {
        home: HomeNumbers::load(paths, now, win::sunday_first(), os_name),
        capsules: CapsuleSummary::all(&paths.capsules()),
        projects: Projects::load(&paths.projects()),
        config: Config::load(&paths.config()),
        appearance: Appearance::load(&paths.config()),
    }
}

fn text_of(h: HWND) -> String {
    // SAFETY: reads our own child control's text into a buffer of its length.
    unsafe {
        let n = GetWindowTextLengthW(h).max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        let got = GetWindowTextW(h, &mut buf).max(0) as usize;
        String::from_utf16_lossy(&buf[..got.min(n)])
    }
}

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

fn send(msg: Msg) {
    with_win(|w| {
        let _ = w.ctx.tx.send(msg);
    });
}

/// Do what a control says. Runs with nothing borrowed: menus and dialogs have their own message loops.
fn perform(action: Action, at: POINT) {
    let Some((hwnd, ctx, status, config)) = with_win(|w| (w.hwnd, w.ctx.clone(), w.status.clone(), w.data.config.clone())) else { return };
    match action {
        Action::Nav(s) => {
            with_win(|w| {
                w.go(s);
                w.request_load();
                w.rebuild();
            });
        }
        Action::SeeAll => perform(Action::Nav(Section::Capsules), at),
        Action::ToggleRecord => send(Msg::ToggleRecord),
        Action::Voice => send(Msg::ToggleVoice),
        Action::AddScreen => send(Msg::AddScreen),
        Action::Pick(id) => send(Msg::PickProject(id)),
        Action::NewProject => {
            let clue = status.new_project_clue.clone();
            if let Some(name) = input::ask(hwnd, "New project", &status.new_project_prompt(), clue.as_deref().unwrap_or("")) {
                send(Msg::NewProject { name, clue });
                with_win(|w| w.request_load());
            }
        }
        // Follows Your AI; stops contextualizing first (the UI thread's app does both).
        Action::Make => ctx.ui.post(crate::app::UiCmd::Make),
        Action::CopyMyDay => ai::copy_my_day(&ctx.ui, &ctx.log),
        Action::SaveClipboard => ai::save_from_clipboard(&ctx.ui, &ctx.log),
        Action::AiConnect(id) => ai::connect(&ctx.ui, &ctx.log, &id),
        Action::AiMenu(id) => {
            if menu(hwnd, &[context_capsule_core::anyai::DISCONNECT.to_string()], None, at) == Some(0) {
                ai::disconnect(&ctx.ui, &ctx.log, &id);
            }
        }
        Action::AiMore => {
            ai::with(|s| {
                s.more = !s.more;
                s.changed();
            });
            with_win(|w| w.rebuild());
        }
        Action::YourAi => {
            let items = ai::choice_menu(&ai::snapshot());
            let labels: Vec<String> = items.iter().map(|(l, _)| l.clone()).collect();
            let current = ai::with(|s| s.choice.clone());
            let checked = items.iter().position(|(_, id)| id.as_deref() == Some(current.as_str()));
            if let Some(Some(id)) = menu(hwnd, &labels, checked, at).and_then(|i| items.get(i)).map(|(_, id)| id.clone()) {
                if id != current {
                    ai::set_choice(&id, &ctx.log);
                }
            }
        }
        Action::Open(path) => {
            // No app for .md files yet: Notepad reads it.
            let file = path.to_string_lossy();
            if !win::open(&file) && !win::open_with("notepad.exe", Some(&format!("\"{file}\""))) {
                win::alert(Some(hwnd), "Couldn't open the capsule", "Try Copy text, then paste it anywhere.");
            }
        }
        Action::More(path) => more(hwnd, &path, at),
        Action::Edit(id) => {
            with_win(|w| w.start_edit(&id));
        }
        Action::Delete(id) => {
            let Some(p) = with_win(|w| w.data.projects.iter().find(|p| p.id == id).cloned()).flatten() else { return };
            if input::ask_choice(&w::delete_question(&p), w::DELETE_DETAIL, w::DELETE, w::CANCEL) {
                if let Err(e) = settings::delete_project(&ctx.paths.projects(), &id) {
                    ctx.log.write(&format!("cannot save projects.json: {e}"));
                }
                with_win(|w| {
                    w.data.projects.retain(|p| p.id != id);
                    w.request_load();
                    w.rebuild();
                });
            }
        }
        Action::SaveEdit(_) => {
            with_win(|w| w.save_edit());
        }
        Action::CancelEdit => {
            with_win(|w| {
                w.editing = None;
                // SAFETY: our own window.
                unsafe {
                    let _ = SetFocus(Some(w.hwnd));
                }
                w.rebuild();
            });
        }
        Action::Setup(step) => {
            setup::act(step, &ctx.log);
            with_win(|w| {
                w.setup = setup::done();
                w.rebuild();
            });
        }
        Action::Remind => {
            let value = if config.remind_at.is_some() { false.into() } else { Config::REMIND_AT.into() };
            with_win(|w| w.set(settings::KEY_REMIND_AT, Some(value)));
        }
        Action::RemindAt => {
            let list = settings::remind_options(config.remind_at.as_deref());
            let current = config.remind_at.as_deref().map(settings::normalize_time);
            let checked = list.iter().position(|t| Some(t) == current.as_ref());
            if let Some(i) = menu(hwnd, &list, checked, at) {
                with_win(|w| w.set(settings::KEY_REMIND_AT, Some(list[i].clone().into())));
            }
        }
        Action::Keep => {
            let options = settings::keep_options(config.keep_days);
            let labels: Vec<String> = options.iter().map(|(_, s)| s.clone()).collect();
            let checked = options.iter().position(|(d, _)| *d == config.keep_days);
            if let Some(i) = menu(hwnd, &labels, checked, at) {
                with_win(|w| w.set(settings::KEY_KEEP_DAYS, Some(options[i].0.into())));
            }
        }
        Action::Sounds => {
            with_win(|w| w.set(settings::KEY_SOUNDS, Some((!config.sounds).into())));
        }
        Action::ShareScreens => {
            let labels: Vec<String> = ShareScreens::ALL.iter().map(|s| s.label().to_string()).collect();
            let checked = ShareScreens::ALL.iter().position(|s| *s == config.share_screens);
            if let Some(i) = menu(hwnd, &labels, checked, at) {
                with_win(|w| w.set(settings::KEY_SHARE_SCREENS, Some(ShareScreens::ALL[i].value().into())));
            }
        }
        Action::Language => {
            let options = settings::language_options(config.voice_locale.as_deref());
            let labels: Vec<String> = options.iter().map(|(_, s)| s.clone()).collect();
            let checked = Some(settings::language_index(config.voice_locale.as_deref()));
            if let Some(i) = menu(hwnd, &labels, checked, at) {
                let id = options[i].0.clone();
                with_win(|w| w.set(settings::KEY_VOICE_LOCALE, (!id.is_empty()).then(|| id.into())));
            }
        }
        Action::Appearance => {
            let labels: Vec<String> = Appearance::ALL.iter().map(|a| a.label().to_string()).collect();
            let current = with_win(|w| w.data.appearance).unwrap_or_default();
            let checked = Appearance::ALL.iter().position(|a| *a == current);
            if let Some(i) = menu(hwnd, &labels, checked, at) {
                with_win(|w| w.set(settings::KEY_APPEARANCE, Some(Appearance::ALL[i].value().into())));
            }
        }
        Action::RemoveApp(app) => {
            let list = settings::remove_excluded(&config.exclude_apps, &app);
            with_win(|w| w.set(settings::KEY_EXCLUDE_APPS, Some(settings::strings(&list))));
        }
        Action::AutoUpdate => {
            if !crate::update::set_auto_update(!crate::update::auto_update()) {
                win::alert(Some(hwnd), w::UPDATES, w::UPDATES_NOT_SAVED);
            }
            with_win(|w| w.rebuild());
        }
        Action::CheckUpdates => crate::update::check_now(),
        Action::OpenCapsule => {
            if let Some(path) = win::open_capsule_dialog(hwnd) {
                ai::open_received(&ctx.ui, &ctx.log, &path);
            }
        }
        Action::Ask => {
            let Some(question) = with_win(|w| w.search.trim().to_string()).filter(|q| !q.is_empty()) else { return };
            ai::ask(&ctx.ui, &ctx.log, &question);
        }
        Action::ShareToggle(i) => {
            with_win(|w| {
                if let Some(on) = w.share.as_mut().and_then(|s| s.checked.get_mut(i)) {
                    *on = !*on;
                }
                w.rebuild();
            });
        }
        Action::ShareGo => {
            let taken = with_win(|w| {
                let s = w.share.take();
                w.scroll = 0.0;
                w.rebuild();
                s
            });
            let Some(share) = taken.flatten() else { return };
            let removed = share.checked.iter().filter(|c| **c).count();
            ctx.log.write(&format!("share: {removed} of {} item(s) removed", share.items.len()));
            share_now(hwnd, share.how, &share.path, Some(&share.shared_text()));
        }
        Action::Shortcut(which) => {
            with_win(|w| {
                if w.recording_keys == Some(which) {
                    return w.stop_recording_keys();
                }
                w.keys_note = None;
                w.recording_keys = Some(which);
                hotkeys::suspend(w.ctx.ui.hwnd());
                // SAFETY: our own window takes the keys (off any text box).
                unsafe {
                    let _ = SetFocus(Some(w.hwnd));
                }
                w.rebuild();
            });
        }
        Action::ResetShortcuts => {
            with_win(|w| {
                w.recording_keys = None;
                w.change_shortcuts(None, Ok(Hotkeys::DEFAULT));
            });
        }
        Action::ShareCancel => {
            with_win(|w| {
                w.share = None;
                w.scroll = 0.0;
                w.rebuild();
            });
        }
        Action::SeeTimeline => {
            with_win(|w| {
                w.timeline.go_to(context_capsule_core::capsules::today());
                w.go(Section::Timeline);
                w.rebuild();
            });
        }
        Action::DayBefore => {
            with_win(|w| w.timeline_day(tl::day_before(w.timeline.date)));
        }
        Action::DayAfter => {
            let today = context_capsule_core::capsules::today();
            with_win(|w| {
                if let Some(d) = tl::day_after(w.timeline.date, today) {
                    w.timeline_day(d);
                }
            });
        }
        Action::DayToday => {
            with_win(|w| w.timeline_day(context_capsule_core::capsules::today()));
        }
        Action::Calendar => {
            use chrono::Datelike;
            with_win(|w| {
                let d = w.timeline.date;
                w.timeline.calendar = if w.timeline.calendar.is_some() { None } else { Some((d.year(), d.month())) };
                w.rebuild();
            });
        }
        Action::CalendarMonth(delta) => {
            with_win(|w| {
                w.timeline.calendar_month(delta, context_capsule_core::capsules::today());
                w.rebuild();
            });
        }
        Action::PickDay(date) => {
            with_win(|w| w.timeline_day(date));
        }
        Action::ToggleBlock(session) => {
            with_win(|w| {
                w.timeline.toggle(&session);
                w.rebuild();
            });
        }
        Action::OpenStretch(key) => {
            with_win(|w| {
                w.timeline.open_stretch(&key);
                w.rebuild();
                // Scroll so the stretch is near the top.
                let at = w.placed.iter().find(|(n, r)| *r == Region::Content && n.action() == Some(&Action::DeleteStretch(key.clone()))).map(|(n, _)| n.rect().y);
                if let Some(y) = at {
                    w.scroll_to(y - 16.0);
                }
            });
        }
        Action::ShowReads(key) => {
            with_win(|w| {
                if !w.timeline.reads_open.remove(&key) {
                    w.timeline.reads_open.insert(key);
                }
                w.rebuild();
            });
        }
        Action::MoreReads(key) => {
            with_win(|w| {
                *w.timeline.reads_limit.entry(key).or_insert(tl::PAGE) += tl::PAGE;
                w.rebuild();
            });
        }
        Action::ShowAll(key) => {
            with_win(|w| {
                if !w.timeline.expanded.remove(&key) {
                    w.timeline.expanded.insert(key);
                }
                w.rebuild();
            });
        }
        Action::MakeFrom(session) => ai::make_from_session(&ctx.ui, &ctx.log, &session),
        Action::DeleteRead(key) => {
            // From the whole day (a search shows only part of it).
            let Some(m) = with_win(|w| w.timeline.day().and_then(|d| d.find(&key)).cloned()).flatten() else { return };
            let (item, question) = if m.is_note() { (tl::delete_note_menu(&m), tl::delete_note_question(&m)) } else { (tl::DELETE_READ_MENU, tl::DELETE_READ) };
            if menu(hwnd, &[item.to_string()], None, at) != Some(0) || still_recording(hwnd, &status, &m.session) {
                return;
            }
            if input::ask_choice(question, &tl::delete_read_detail(THIS_PC), tl::DELETE, tl::CANCEL) {
                let done = tl::delete_read(&ctx.paths, &m);
                timeline_deleted(hwnd, &ctx, done, if m.is_note() { "note" } else { "read" });
            }
        }
        Action::DeleteStretch(key) => {
            let Some(s) = with_win(|w| w.timeline.day().and_then(|d| d.stretch(&key)).cloned()).flatten() else { return };
            if menu(hwnd, &[tl::DELETE_STRETCH_MENU.to_string()], None, at) != Some(0) || still_recording(hwnd, &status, &s.session) {
                return;
            }
            if input::ask_choice(tl::DELETE_STRETCH, &tl::delete_stretch_detail(&s, THIS_PC), tl::DELETE, tl::CANCEL) {
                let done = tl::delete_stretch(&ctx.paths, &s);
                timeline_deleted(hwnd, &ctx, done, "stretch");
            }
        }
        Action::DeleteBlock(session) => {
            let Some(block) = with_win(|w| w.timeline.day().and_then(|d| d.block(&session)).cloned()).flatten() else { return };
            if menu(hwnd, &[tl::DELETE_BLOCK_MENU.to_string()], None, at) != Some(0) || still_recording(hwnd, &status, &session) {
                return;
            }
            if input::ask_choice(tl::DELETE_BLOCK, &tl::delete_block_detail(&block, THIS_PC), tl::DELETE, tl::CANCEL) {
                let done = tl::delete_session(&ctx.paths, &session);
                timeline_deleted(hwnd, &ctx, done, "recording");
            }
        }
        Action::AddApp => {
            let apps = open_apps(&config.exclude_apps);
            let mut labels: Vec<String> = apps.iter().map(|(_, n)| n.clone()).collect();
            if !labels.is_empty() {
                labels.push("-".into());
            }
            labels.push(BROWSE.into());
            let Some(i) = menu(hwnd, &labels, None, at) else { return };
            let exe = if i < apps.len() {
                Some(apps[i].0.clone())
            } else {
                win::open_app_dialog(hwnd).and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
            };
            if let Some(exe) = exe {
                let list = settings::add_excluded(&config.exclude_apps, &exe);
                with_win(|w| w.set(settings::KEY_EXCLUDE_APPS, Some(settings::strings(&list))));
            }
        }
    }
}

/// That recording is still going: say "Stop first" (true), before a delete is asked about.
fn still_recording(hwnd: HWND, status: &Status, session: &str) -> bool {
    let going = status.recording && status.session_id.as_deref() == Some(session);
    if going {
        let (title, body) = tl::Refused::Recording.message();
        win::alert(Some(hwnd), &title, &body);
    }
    going
}

/// After a delete on the Timeline: say why it didn't happen, or read the day (and Home's numbers) again.
fn timeline_deleted(hwnd: HWND, ctx: &Ctx, done: Result<(), tl::Refused>, what: &str) {
    match done {
        Ok(()) => ctx.log.write(&format!("timeline: a {what} deleted")),
        Err(why) => {
            ctx.log.write(&format!("timeline: {what} not deleted: {why:?}"));
            let (title, body) = why.message();
            win::alert(Some(hwnd), &title, &body);
        }
    }
    with_win(|w| {
        w.request_timeline();
        w.request_load();
    });
}

/// "Choose…" → an app that isn't open right now.
const BROWSE: &str = "Find an app…";

/// Copy file, Copy text, Save a copy…, Show in folder. Sharing first shows "Check before sharing" when
/// something in the capsule might be private.
fn more(hwnd: HWND, path: &Path, at: POINT) {
    let items = [w::COPY_FILE, w::COPY_TEXT, w::SAVE_COPY, w::SHOW_IN_FOLDER].map(String::from);
    let how = match menu(hwnd, &items, None, at) {
        Some(0) => ShareHow::CopyFile,
        Some(1) => ShareHow::CopyText,
        Some(2) => ShareHow::SaveCopy,
        Some(3) => {
            win::show_in_folder(path);
            return;
        }
        _ => return,
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        win::alert(Some(hwnd), "Couldn't open the capsule", &path.display().to_string());
        return;
    };
    match ShareView::new(path.to_path_buf(), how, text) {
        Some(view) => {
            with_win(|w| {
                w.share = Some(view);
                w.scroll = 0.0;
                w.focus = None;
                w.rebuild();
            });
        }
        // Nothing that looks private: straight through, the file as it is.
        None => share_now(hwnd, how, path, None),
    }
}

/// Copy the file, copy the text, or save a copy. `text`: the checked items already removed (a temporary copy
/// is shared; the saved capsule never changes); None: the capsule as it is.
fn share_now(hwnd: HWND, how: ShareHow, path: &Path, text: Option<&str>) {
    let note = |s: &str| {
        with_win(|w| w.notice(s));
    };
    let name = path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    match how {
        ShareHow::CopyFile => {
            let file = match text {
                None => Some(path.to_path_buf()),
                Some(t) => temp_copy(&name, t),
            };
            if file.is_some_and(|f| win::copy_file(&f)) {
                note(w::COPIED_FILE);
            }
        }
        ShareHow::CopyText => {
            let copied = match text {
                Some(t) => win::copy_text(t),
                None => std::fs::read_to_string(path).is_ok_and(|t| win::copy_text(&t)),
            };
            if copied {
                note(w::COPIED_TEXT);
            }
        }
        ShareHow::SaveCopy => {
            if let Some(dest) = win::save_dialog(hwnd, &name) {
                let dest: PathBuf = dest;
                let saved = match text {
                    Some(t) => std::fs::write(&dest, t),
                    None => std::fs::copy(path, &dest).map(|_| ()),
                };
                match saved {
                    Ok(()) => note(&format!("Saved to {}.", dest.parent().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().into_owned()).unwrap_or_default())),
                    Err(e) => win::alert(Some(hwnd), "Couldn't save the copy", &e.to_string()),
                }
            }
        }
    }
}

/// The shared copy: a file with the capsule's name in a temporary folder (it stays there to be pasted; the next
/// share replaces it).
fn temp_copy(name: &str, text: &str) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join("Context Capsule").join("shared");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let file = dir.join(if name.is_empty() { "capsule.capsule.md" } else { name });
    std::fs::write(&file, text).ok()?;
    Some(file)
}

fn wheel_delta(wp: WPARAM) -> f64 {
    ((wp.0 >> 16) & 0xFFFF) as u16 as i16 as f64
}

fn point(lp: LPARAM) -> (i32, i32) {
    ((lp.0 & 0xFFFF) as u16 as i16 as i32, ((lp.0 >> 16) & 0xFFFF) as u16 as i16 as i32)
}

unsafe extern "system" fn field_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    let parent = with_win(|w| w.hwnd);
    let post = |m: u32, wp: usize| {
        if let Some(p) = parent {
            // SAFETY: posting to our own window.
            unsafe {
                let _ = PostMessageW(Some(p), m, WPARAM(wp), LPARAM(h.0 as isize));
            }
        }
    };
    match msg {
        WM_KEYDOWN if wp.0 == VK_TAB.0 as usize => {
            // SAFETY: reads the keyboard state.
            let back = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
            post(WM_FIELD_TAB, back as usize);
            return LRESULT(0);
        }
        WM_KEYDOWN if wp.0 == VK_RETURN.0 as usize => {
            post(WM_FIELD_ENTER, 0);
            return LRESULT(0);
        }
        WM_KEYDOWN if wp.0 == VK_ESCAPE.0 as usize => {
            post(WM_FIELD_ESCAPE, 0);
            return LRESULT(0);
        }
        // No beep for the keys handled above.
        WM_CHAR if matches!(wp.0, 0x09 | 0x0D | 0x1B) => return LRESULT(0),
        _ => {}
    }
    // SAFETY: the default handling of the subclassed edit control.
    unsafe { DefSubclassProc(h, msg, wp, lp) }
}

fn field_of(id: usize) -> Option<Field> {
    Field::ALL.get(id.checked_sub(FIELD_ID)?).copied()
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            if with_win(|w| w.paint()).is_none() {
                // Busy (a menu or dialog is open over us) or not ready: let Windows validate the area.
                // SAFETY: default handling.
                return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
            }
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_SIZE => {
            with_win(|w| {
                w.measure_client();
                w.rebuild();
            });
        }
        WM_GETMINMAXINFO => {
            // SAFETY: Windows passes a MINMAXINFO for this message; our own window's DPI.
            unsafe {
                let scale = GetDpiForWindow(hwnd).max(96) as f64 / 96.0;
                let info = &mut *(lp.0 as *mut MINMAXINFO);
                info.ptMinTrackSize = POINT { x: (w::MIN_WIDTH * scale) as i32, y: (w::MIN_HEIGHT * scale) as i32 };
            }
        }
        WM_DPICHANGED => {
            let dpi = (wp.0 & 0xFFFF) as f64;
            // SAFETY: Windows passes the suggested window rect for this message.
            let r = unsafe { *(lp.0 as *const RECT) };
            with_win(|w| {
                w.scale = dpi.max(96.0) / 96.0;
                w.fonts = Fonts::new(w.scale, w.faces.clone());
                w.font_fields();
            });
            // SAFETY: our own window.
            unsafe {
                let _ = SetWindowPos(hwnd, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
            }
            with_win(|w| {
                w.measure_client();
                w.rebuild();
            });
        }
        WM_MOUSEMOVE => {
            let (x, y) = point(lp);
            with_win(|w| {
                let (x, y) = (x as f64 / w.scale, y as f64 / w.scale);
                if let Some((y0, s0)) = w.drag {
                    let view = w.view_h();
                    let thumb = w.thumb().map_or(view, |t| t.h);
                    let per = if view > thumb { w.max_scroll() / (view - thumb) } else { 0.0 };
                    w.scroll_to(s0 + (y - y0) * per);
                    return;
                }
                let hover = w.hit(x, y);
                if hover != w.hover {
                    w.hover = hover;
                    w.invalidate();
                }
                if !w.tracking {
                    w.tracking = true;
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    // SAFETY: asks for WM_MOUSELEAVE on our own window.
                    unsafe {
                        let _ = TrackMouseEvent(&mut t);
                    }
                }
            });
        }
        WM_MOUSELEAVE => {
            with_win(|w| {
                w.tracking = false;
                if w.hover.take().is_some() {
                    w.invalidate();
                }
            });
        }
        WM_LBUTTONDOWN => {
            let (x, y) = point(lp);
            with_win(|w| {
                let (x, y) = (x as f64 / w.scale, y as f64 / w.scale);
                // A click anywhere but a shortcut's button stops waiting for keys.
                if w.recording_keys.is_some() && !w.hit(x, y).is_some_and(|i| matches!(w.placed[i].0.action(), Some(Action::Shortcut(_)))) {
                    w.stop_recording_keys();
                }
                w.keyboard = false;
                // SAFETY: our own window takes the focus (off any text box) and the mouse.
                unsafe {
                    let _ = SetFocus(Some(hwnd));
                    SetCapture(hwnd);
                }
                if let Some(t) = w.thumb() {
                    if x >= w.size.0 - 14.0 && y >= HEADER {
                        if y >= t.y && y < t.bottom() {
                            w.drag = Some((y, w.scroll));
                            w.invalidate();
                        } else {
                            let page = w.view_h() - STEP;
                            w.scroll_to(w.scroll + if y < t.y { -page } else { page });
                        }
                        return;
                    }
                }
                w.pressed = w.hit(x, y);
                if let Some(i) = w.pressed {
                    w.focus = Some(i);
                }
                w.invalidate();
            });
        }
        WM_LBUTTONUP => {
            let (x, y) = point(lp);
            // SAFETY: releases our capture.
            unsafe {
                let _ = ReleaseCapture();
            }
            let act = with_win(|w| {
                if w.drag.take().is_some() {
                    w.invalidate();
                    return None;
                }
                let (x, y) = (x as f64 / w.scale, y as f64 / w.scale);
                let pressed = w.pressed.take();
                w.invalidate();
                pressed.filter(|p| w.hit(x, y) == Some(*p)).and_then(|p| w.press(p))
            })
            .flatten();
            if let Some((action, at)) = act {
                perform(action, at);
            }
        }
        WM_MOUSEWHEEL => {
            let notches = wheel_delta(wp) / 120.0;
            with_win(|w| w.scroll_to(w.scroll - notches * STEP));
        }
        // Waiting for new keys (Settings → Shortcuts): every key goes there, Alt combinations too.
        WM_KEYDOWN | WM_SYSKEYDOWN if with_win(|w| w.recording_keys.is_some()) == Some(true) => {
            with_win(|w| w.record_key(wp.0 as u32));
        }
        WM_CHAR | WM_SYSCHAR | WM_SYSKEYUP if with_win(|w| w.recording_keys.is_some()) == Some(true) => {}
        WM_KEYDOWN => {
            let key = wp.0 as u16;
            let act = with_win(|w| {
                // SAFETY: reads the keyboard state.
                let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) } < 0;
                match key {
                    k if k == VK_TAB.0 => w.tab(shift),
                    k if k == VK_RETURN.0 || k == VK_SPACE.0 => {
                        w.keyboard = true;
                        return w.focus.and_then(|f| w.press(f));
                    }
                    k if k == VK_ESCAPE.0 && w.share.is_some() => return Some((Action::ShareCancel, POINT::default())),
                    k if k == VK_ESCAPE.0 && w.editing.is_some() => return Some((Action::CancelEdit, POINT::default())),
                    k if k == VK_DOWN.0 => w.scroll_to(w.scroll + STEP),
                    k if k == VK_UP.0 => w.scroll_to(w.scroll - STEP),
                    k if k == VK_NEXT.0 => w.scroll_to(w.scroll + w.view_h() - STEP),
                    k if k == VK_PRIOR.0 => w.scroll_to(w.scroll - w.view_h() + STEP),
                    k if k == VK_HOME.0 => w.scroll_to(0.0),
                    k if k == VK_END.0 => w.scroll_to(f64::MAX),
                    _ => {}
                }
                None
            })
            .flatten();
            if let Some((action, at)) = act {
                perform(action, at);
            }
        }
        WM_FIELD_TAB => {
            let back = wp.0 == 1;
            with_win(|w| {
                let from = field_of_hwnd(w, HWND(lp.0 as *mut _));
                if let Some(f) = from {
                    w.focus = w.placed.iter().position(|(n, _)| matches!(n, Node::Field { field, .. } if *field == f));
                }
                w.tab(back);
            });
        }
        WM_FIELD_ENTER => {
            let ask = with_win(|w| {
                if field_of_hwnd(w, HWND(lp.0 as *mut _)) == Some(Field::Search) {
                    return true; // Enter in the search box: Ask
                }
                if w.editing.is_some() {
                    w.save_edit();
                } else {
                    // SAFETY: our own window: Enter in the name box just leaves it.
                    unsafe {
                        let _ = SetFocus(Some(w.hwnd));
                    }
                }
                false
            });
            if ask == Some(true) {
                perform(Action::Ask, POINT::default());
            }
        }
        WM_FIELD_ESCAPE => {
            let from = HWND(lp.0 as *mut _);
            if matches!(with_win(|w| field_of_hwnd(w, from)), Some(Some(Field::Search | Field::TimelineSearch))) {
                // Esc in the search box clears it. Outside the borrow: EN_CHANGE comes back at once and shows
                // every capsule again.
                // SAFETY: our own child control.
                unsafe {
                    let _ = SetWindowTextW(from, &HSTRING::from(""));
                }
            } else {
                perform(Action::CancelEdit, POINT::default());
            }
        }
        WM_COMMAND => {
            let (id, code) = (wp.0 & 0xFFFF, (wp.0 >> 16) as u32 & 0xFFFF);
            if let Some(f) = field_of(id) {
                with_win(|w| match code {
                    EN_SETFOCUS => {
                        w.focus = w.placed.iter().position(|(n, _)| matches!(n, Node::Field { field, .. } if *field == f));
                        w.invalidate();
                    }
                    EN_KILLFOCUS => w.invalidate(),
                    EN_CHANGE if f == Field::Search => {
                        w.search = w.field_text(Field::Search);
                        w.rebuild();
                    }
                    EN_CHANGE if f == Field::TimelineSearch => {
                        let text = w.field_text(Field::TimelineSearch);
                        w.timeline.set_search(&text);
                        w.rebuild();
                    }
                    EN_CHANGE if f == Field::Name && !w.setting_text => {
                        let typed = w.field_text(Field::Name);
                        let _ = ConfigFile::set(&w.ctx.paths.config(), settings::KEY_AUTHOR, settings::author_value(&typed));
                        w.data.config.author = settings::author_value(&typed).and_then(|v| v.as_str().map(String::from));
                        w.data.home.first_name = settings::first_name(w.data.config.author.as_deref().unwrap_or(&w.os_name));
                    }
                    _ => {}
                });
            }
        }
        WM_CTLCOLOREDIT => {
            let r = with_win(|w| {
                let dc = HDC(wp.0 as *mut _);
                let search = matches!(field_of_hwnd(w, HWND(lp.0 as *mut _)), Some(Field::Search | Field::TimelineSearch));
                let (color, brush) = if search { (w.theme.search, w.search_brush) } else { (w.theme.field, w.field_brush) };
                // SAFETY: the edit control's DC, given by Windows for this message.
                unsafe {
                    SetTextColor(dc, paint::colorref(w.theme.text));
                    SetBkColor(dc, paint::colorref(color));
                }
                LRESULT(brush.0 as isize)
            });
            if let Some(r) = r {
                return r;
            }
        }
        WM_TIMER if wp.0 == TICK => {
            with_win(|w| {
                let mut again = w.status.recording;
                if w.section == Section::Settings {
                    let done = setup::done();
                    again |= done != w.setup;
                    w.setup = done;
                    ai::with(|s| s.claude_seen(done.claude));
                    w.ai_ticks += 1;
                    if w.ai_ticks % AI_EVERY == 0 {
                        ai::refresh(&w.ctx.ui, &w.ctx.log);
                    }
                }
                again |= ai::with(|s| s.version) != w.ai_seen;
                // Today's Timeline follows the recording.
                if w.section == Section::Timeline && w.status.recording && w.timeline.date == context_capsule_core::capsules::today() {
                    w.timeline_ticks += 1;
                    if w.timeline_ticks >= TIMELINE_EVERY {
                        w.request_timeline();
                    }
                }
                if w.notice.as_ref().is_some_and(|(_, at)| at.elapsed() > NOTICE_FOR) {
                    w.notice = None;
                    w.invalidate();
                }
                if again {
                    w.rebuild();
                }
            });
        }
        WM_TIMER if wp.0 == STATS => {
            with_win(|w| w.request_load());
        }
        // Switched to another window while waiting for keys: stop waiting.
        WM_ACTIVATE if wp.0 & 0xFFFF == 0 => {
            with_win(|w| w.stop_recording_keys());
            // SAFETY: default handling too.
            return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
        }
        WM_ACTIVATE if wp.0 & 0xFFFF != 0 => {
            with_win(|w| {
                if w.loaded {
                    w.request_load();
                }
                if w.section == Section::Timeline {
                    w.request_timeline();
                }
            });
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_SYSCOLORCHANGE | WM_DWMCOLORIZATIONCOLORCHANGED => {
            // Light / dark changed in Windows: the theme is read again and everything repainted.
            with_win(|w| w.retheme());
            // SAFETY: default handling too (it passes these on as Windows expects).
            return unsafe { DefWindowProcW(hwnd, msg, wp, lp) };
        }
        WM_CLOSE => {
            // SAFETY: our own window.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
        WM_DESTROY => {
            // SAFETY: our own timers.
            unsafe {
                let _ = KillTimer(Some(hwnd), TICK);
                let _ = KillTimer(Some(hwnd), STATS);
            }
            if let Some(w) = WIN.with(|s| s.try_borrow_mut().ok().and_then(|mut s| s.take())) {
                if w.recording_keys.is_some() {
                    hotkeys::resume(w.ctx.ui.hwnd());
                }
                w.ctx.log.write("window: closed");
                // SAFETY: the brush and icons we made; the window is going away.
                unsafe {
                    let _ = DeleteObject(w.field_brush.into());
                    let _ = DeleteObject(w.search_brush.into());
                    for i in &w.icons {
                        let _ = DestroyIcon(*i);
                    }
                }
            }
        }
        // SAFETY: default handling for everything else.
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
    LRESULT(0)
}

fn field_of_hwnd(w: &Win, h: HWND) -> Option<Field> {
    w.fields.iter().find(|(_, x)| **x == h).map(|(f, _)| *f)
}
