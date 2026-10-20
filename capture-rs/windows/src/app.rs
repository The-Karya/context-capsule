// context-capsule — windows: the UI thread — a hidden owner window for the tray, hotkeys, menu, toasts,
// alerts, the setup window, the capture picker and the evening reminder; it starts the core engine on its own
// thread and relays messages. Start / stop cues (a short message and a soft sound) come from status changes;
// while recording or listening a timer keeps the tray's time and voice bars moving.

use crate::hotkeys::{self, RECORD_LABEL, VOICE_LABEL};
use crate::picker::Capture;
use crate::platform::WinPlatform;
use crate::toast::Toast;
use crate::tray::{self, menu_text, Tray};
use crate::uia::Uia;
use crate::{changes, input, reminder, setup, voice, win, windows_list};
use context_capsule_core::{
    look, recording_started_message, recording_stopped_message, AppLog, Config, Cue, Engine, Exclusion, Front, Indicator, Location,
    Msg, Options, Paths, PlatformText, ProjectItem, Projects, Redactor, Status, Voice,
};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Globalization::{GetTimeFormatEx, GetUserDefaultLocaleName, TIME_NOSECONDS};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Shell::{NINF_KEY, NIN_SELECT};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos,
    GetMessageW, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, TrackPopupMenu,
    IsDialogMessageW, TranslateMessage, HMENU, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_APP, WM_CONTEXTMENU, WM_DESTROY, WM_ENDSESSION, WM_HOTKEY, WM_NULL,
    WM_QUERYENDSESSION, WM_SETTINGCHANGE, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP, KillTimer, SetTimer,
};

const WM_TRAY: u32 = WM_APP + 1;
const WM_CMD: u32 = WM_APP + 2;

const ID_TOGGLE: u32 = 10;
const ID_VOICE: u32 = 11;
const ID_QUIT: u32 = 13;
const ID_SETUP: u32 = 14;
const ID_ADD_SCREEN: u32 = 15;
const ID_PROJECT_AUTO: u32 = 20;
const ID_PROJECT_NEW: u32 = 21;
/// Projects are 100, 101, … in list order.
const ID_PROJECT_FIRST: u32 = 100;
const MAX_PROJECTS: usize = 500;

/// While recording or listening: the tray's running time and voice bars, about 8 times a second.
const LOOK_TIMER: usize = 7;
const LOOK_EVERY_MS: u32 = 125;

/// User-facing words for Windows (our hotkeys use Ctrl too: Alt+Shift alone switches keyboard layouts).
pub const TEXT: PlatformText =
    PlatformText { record_hotkey: RECORD_LABEL, voice_hotkey: VOICE_LABEL, mic_privacy: voice::MIC_SETTINGS, ..PlatformText::WINDOWS };

/// What the engine thread asks the UI thread to do.
pub enum UiCmd {
    Indicator(Indicator),
    Status(Box<Status>),
    Toast(String),
    Alert(String, String),
    PresentPicker,
    /// "Add a screen": the picker for one more screen or window.
    AddSource,
    /// Evening: "Make today's capsules?"
    Remind,
    EngineDone,
}

/// A handle to the UI thread that any thread may hold.
#[derive(Clone)]
pub struct Ui {
    hwnd: usize,
}

impl Ui {
    fn of(hwnd: HWND) -> Ui {
        Ui { hwnd: hwnd.0 as usize }
    }
}

impl Ui {
    fn hwnd(&self) -> HWND {
        HWND(self.hwnd as *mut core::ffi::c_void)
    }

    pub fn post(&self, cmd: UiCmd) {
        let raw = Box::into_raw(Box::new(cmd));
        // SAFETY: the UI thread takes ownership of `raw` in `WM_CMD`; if posting fails we take it back.
        unsafe {
            if PostMessageW(Some(self.hwnd()), WM_CMD, WPARAM(0), LPARAM(raw as isize)).is_err() {
                drop(Box::from_raw(raw));
            }
        }
    }
}

struct App {
    paths: Paths,
    tray: Tray,
    toast: Option<Toast>,
    capture: Arc<Capture>,
    tx: Sender<Msg>,
    status: Status,
    engine: Option<std::thread::JoinHandle<()>>,
    log: AppLog,
    /// For "New project from this window": the window in front when the menu opened.
    apps: windows_list::Apps,
    menu_front: Option<Front>,
    /// UI Automation on this thread, made the first time a browser's address is needed.
    uia: Option<Option<Uia>>,
    /// The tray's timer is running (recording or listening).
    look_timer: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    /// Alerts open right now, and whether the engine finished meanwhile (close once they are dismissed).
    static ALERTS: Cell<u32> = const { Cell::new(0) };
    static CLOSE_PENDING: Cell<bool> = const { Cell::new(false) };
}

/// Explorer restarted: the tray icon must be added again.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// Run `f` on the app, unless it is already in use further up the stack (a nested modal loop).
fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

pub fn run() {
    // SAFETY: process-wide setup calls before any window exists.
    unsafe {
        // Every coordinate (windows, monitors, capture) in physical pixels.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // Started from a terminal: show our log lines there.
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
    // One recorder at a time.
    // SAFETY: a named mutex held for the life of the process.
    let _single = unsafe { CreateMutexW(None, true, w!("Local\\ContextCapsuleRecorder")) };
    // SAFETY: reads this thread's last error.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        win::alert(None, "Context Capsule", "Context Capsule is already running. Look for its icon in the tray.");
        return;
    }
    win::init_com(true);

    let Some(hwnd) = create_window() else {
        eprintln!("Context Capsule: cannot create the tray window");
        return;
    };
    // SAFETY: registers a well-known message name.
    TASKBAR_CREATED.store(unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }, Ordering::Relaxed);

    let paths = Paths::standard();
    let _ = std::fs::create_dir_all(paths.sessions());
    let log = AppLog::new(paths.log());
    log.write(&format!("app started: version {}", env!("CARGO_PKG_VERSION")));
    let (tx, rx) = channel();
    let capture = Capture::new(tx.clone());
    let ui = Ui::of(hwnd);

    let mut tray = Tray::new(hwnd, WM_TRAY);
    tray.set_tip(tray::tooltip(&Status::default(), SystemTime::now()));
    tray.add();
    hotkeys::register(hwnd);
    changes::install();
    let engine = spawn_engine(paths.clone(), ui, capture.clone(), tx.clone(), rx, log.clone());
    setup::open_at_login_once(&log);
    if !setup::seen() {
        setup::show(&log); // first launch
    }

    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            paths,
            tray,
            toast: Toast::new(),
            capture,
            tx,
            status: Status::default(),
            engine: Some(engine),
            log,
            apps: Default::default(),
            menu_front: None,
            uia: None,
            look_timer: false,
        })
    });

    let mut msg = MSG::default();
    // SAFETY: the standard message loop for this thread; the setup window gets Tab / Enter / Esc.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if setup::window().is_some_and(|d| IsDialogMessageW(d, &msg).as_bool()) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn create_window() -> Option<HWND> {
    // SAFETY: registers our class and creates a never-shown tool window (no taskbar button). A real
    // (hidden) window rather than a message-only one: the tray menu and the picker need an owner that
    // can take the foreground.
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: instance.into(),
            lpszClassName: w!("ContextCapsuleTray"),
            ..Default::default()
        };
        RegisterClassW(&class);
        CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0),
            w!("ContextCapsuleTray"),
            w!("Context Capsule"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()
    }
}

fn spawn_engine(
    paths: Paths,
    ui: Ui,
    capture: Arc<Capture>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    log: AppLog,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("engine".into())
        .spawn(move || {
            win::init_com(false);
            let toast_ui = ui.clone();
            let notify: voice::Notify = Arc::new(move |text: String| toast_ui.post(UiCmd::Toast(text)));
            let voice = voice::engine(&paths.root, TEXT.voice_hotkey, notify, log);
            let opts = Options { own_app_id: own_app_id(), text: TEXT, locale: user_locale() };
            let mut engine = Engine::new(paths, WinPlatform::new(ui.clone(), capture), voice, opts);
            engine.set_voice_wake(Arc::new(move || {
                let _ = tx.send(Msg::VoiceReady);
            }));
            engine.run(rx);
            ui.post(UiCmd::EngineDone);
        })
        .expect("cannot start the engine thread")
}

/// Our exe name: never recorded (our own menu and toasts).
fn own_app_id() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.file_name()?.to_string_lossy().into_owned())
}

/// The user's locale ("en-US") for voice notes.
fn user_locale() -> String {
    let mut buf = [0u16; 85];
    // SAFETY: writes at most buf.len() chars.
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n > 1 { String::from_utf16_lossy(&buf[..n as usize - 1]) } else { "en-US".into() }
}

/// "2:57 PM" / "14:57", as the user's clock shows it.
fn short_time(t: SystemTime) -> String {
    use chrono::{Datelike, Timelike};
    let local = chrono::DateTime::<chrono::Local>::from(t);
    let st = windows::Win32::Foundation::SYSTEMTIME {
        wYear: local.year() as u16,
        wMonth: local.month() as u16,
        wDay: local.day() as u16,
        wHour: local.hour() as u16,
        wMinute: local.minute() as u16,
        ..Default::default()
    };
    let mut buf = [0u16; 64];
    // SAFETY: writes at most buf.len() chars.
    let n = unsafe { GetTimeFormatEx(None, TIME_NOSECONDS, Some(&st), None, Some(&mut buf)) };
    if n > 1 { String::from_utf16_lossy(&buf[..n as usize - 1]) } else { local.format("%H:%M").to_string() }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => {
            let event = (lp.0 & 0xFFFF) as u32;
            if event == WM_CONTEXTMENU || event == NIN_SELECT || event == NIN_SELECT | NINF_KEY {
                show_menu(hwnd);
            }
        }
        WM_HOTKEY => {
            if let Some(m) = hotkeys::msg_for(wp.0) {
                with_app(|a| a.tx.send(m));
            }
        }
        WM_CMD => {
            // SAFETY: `Ui::post` boxed this command and handed it to us.
            let cmd = unsafe { Box::from_raw(lp.0 as *mut UiCmd) };
            handle(hwnd, *cmd);
        }
        WM_TIMER if wp.0 == LOOK_TIMER => {
            with_app(|a| look_tick(hwnd, a));
        }
        WM_SETTINGCHANGE => {
            with_app(|a| a.tray.retheme()); // light / dark taskbar
        }
        WM_QUERYENDSESSION => return LRESULT(1),
        WM_ENDSESSION if wp.0 != 0 => quit_now(),
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            with_app(|a| a.tray.add());
        }
        // SAFETY: default handling for everything else.
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
    LRESULT(0)
}

fn handle(hwnd: HWND, cmd: UiCmd) {
    match cmd {
        UiCmd::Indicator(i) => {
            with_app(|a| a.tray.set_indicator(i));
        }
        UiCmd::Status(s) => {
            with_app(|a| {
                a.tray.set_tip(tray::tooltip(&s, SystemTime::now()));
                cues(a, &s);
                a.status = *s;
                look_tick(hwnd, a);
            });
        }
        UiCmd::Toast(text) => {
            with_app(|a| {
                let icon = a.tray.rect();
                if let Some(t) = &a.toast {
                    t.show(&text, icon);
                }
            });
        }
        // Outside the app borrow: the message box runs a nested message loop.
        UiCmd::Alert(title, body) => {
            ALERTS.set(ALERTS.get() + 1);
            win::alert(Some(hwnd), &title, &body);
            ALERTS.set(ALERTS.get() - 1);
            if ALERTS.get() == 0 && CLOSE_PENDING.get() {
                close(hwnd);
            }
        }
        UiCmd::PresentPicker => {
            if let Some(capture) = with_app(|a| a.capture.clone()) {
                capture.present(hwnd);
            }
        }
        UiCmd::AddSource => {
            if let Some(capture) = with_app(|a| a.capture.clone()) {
                capture.present_add(hwnd);
            }
        }
        UiCmd::Remind => {
            if let Some(log) = with_app(|a| a.log.clone()) {
                reminder::ask_and_make(Ui::of(hwnd), log);
            }
        }
        // Quitting: let an open alert (e.g. "Nothing was saved") be read first.
        UiCmd::EngineDone if ALERTS.get() > 0 => CLOSE_PENDING.set(true),
        UiCmd::EngineDone => close(hwnd),
    }
}

/// Clear cues when recording or listening starts or stops (Mac parity): a soft sound ("sounds": false turns
/// them off) and, for recording, a short message. Listening shows in the tray icon only: nothing on screen.
fn cues(a: &App, next: &Status) {
    let changes = Cue::changes(&a.status, next);
    if !changes.is_empty() && Config::load(&a.paths.config()).sounds {
        changes.into_iter().for_each(win::play_cue);
    }
    let Some(toast) = &a.toast else { return };
    if next.recording != a.status.recording {
        let several = windows_list::monitors().len() > 1;
        let text = if next.recording { recording_started_message(&TEXT, several) } else { recording_stopped_message(next.events) };
        toast.show(&text, a.tray.rect());
    }
}

/// The tray while recording or listening: the time in the tooltip (the tray can't show text by its icon), and
/// the voice bars following the mic. The timer runs only then.
fn look_tick(hwnd: HWND, a: &mut App) {
    let busy = a.status.recording || a.status.listening;
    if a.status.listening {
        a.tray.set_level(look::level_frame(Voice::level()));
    }
    a.tray.set_tip(tray::tooltip(&a.status, SystemTime::now()));
    if busy != a.look_timer {
        a.look_timer = busy;
        // SAFETY: a timer on our own window.
        unsafe {
            if busy {
                SetTimer(Some(hwnd), LOOK_TIMER, LOOK_EVERY_MS, None);
            } else {
                let _ = KillTimer(Some(hwnd), LOOK_TIMER);
            }
        }
    }
}

fn close(hwnd: HWND) {
    with_app(|a| a.tray.remove());
    // SAFETY: our own window; WM_DESTROY ends the loop.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

fn show_menu(hwnd: HWND) {
    let Some((model, status)) = with_app(|a| {
        // Clicking the tray puts the taskbar in front; this finds the app the user works in.
        a.menu_front = front_now(a);
        (tray::menu_model(&a.status, SystemTime::now(), &short_time), a.status.clone())
    }) else {
        return;
    };
    let projects = status.project_items();
    // SAFETY: a popup menu we build, show and destroy here. TrackPopupMenu runs a nested loop, so the
    // app is not borrowed meanwhile.
    let chosen = unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let item = |menu: HMENU, id: u32, text: &str, enabled: bool| {
            let flags = if enabled { MF_STRING } else { MF_STRING | MF_GRAYED };
            let _ = AppendMenuW(menu, flags, id as usize, &HSTRING::from(text));
        };
        item(menu, 0, &model.info, false);
        if let Some(skip) = &model.skip {
            item(menu, 0, skip, false);
        }
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        item(menu, ID_TOGGLE, &model.toggle, true);
        item(menu, ID_VOICE, &model.voice, true);
        if let Some((label, enabled)) = &model.add_screen {
            item(menu, ID_ADD_SCREEN, label, *enabled);
        }
        if let Ok(sub) = CreatePopupMenu() {
            let mut n = 0;
            for p in &projects {
                let check = |on: bool| if on { MF_STRING | MF_CHECKED } else { MF_STRING };
                let _ = match p {
                    ProjectItem::Auto { checked } => AppendMenuW(sub, check(*checked), ID_PROJECT_AUTO as usize, &HSTRING::from(ProjectItem::AUTO)),
                    ProjectItem::Project { name, checked, .. } if n < MAX_PROJECTS => {
                        n += 1;
                        let id = ID_PROJECT_FIRST as usize + n - 1;
                        AppendMenuW(sub, check(*checked), id, &HSTRING::from(menu_text(name)))
                    }
                    ProjectItem::Project { .. } => Ok(()),
                    ProjectItem::Separator => AppendMenuW(sub, MF_SEPARATOR, 0, None),
                    ProjectItem::New => AppendMenuW(sub, MF_STRING, ID_PROJECT_NEW as usize, &HSTRING::from(ProjectItem::NEW)),
                };
            }
            // The submenu is destroyed with the menu.
            let _ = AppendMenuW(menu, MF_STRING | MF_POPUP, sub.0 as usize, &HSTRING::from(model.project.as_str()));
        }
        item(menu, ID_SETUP, "Settings", true);
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        item(menu, ID_QUIT, "Quit", true);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // The menu closes when clicking elsewhere only if our window is in front.
        let _ = SetForegroundWindow(hwnd);
        let flags = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN;
        let chosen = TrackPopupMenu(menu, flags, pt.x, pt.y, None, hwnd, None).0 as u32;
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        chosen
    };
    match chosen {
        ID_TOGGLE => send(Msg::ToggleRecord),
        ID_VOICE => send(Msg::ToggleVoice),
        ID_ADD_SCREEN => send(Msg::AddScreen),
        ID_SETUP => {
            if let Some(log) = with_app(|a| a.log.clone()) {
                setup::show(&log);
            }
        }
        ID_QUIT => quit(hwnd),
        ID_PROJECT_AUTO => send(Msg::PickProject(None)),
        ID_PROJECT_NEW => new_project(hwnd, &status),
        id if id >= ID_PROJECT_FIRST => {
            if let Some(p) = status.projects.get((id - ID_PROJECT_FIRST) as usize) {
                send(Msg::PickProject(Some(p.id.clone())));
            }
        }
        _ => {}
    }
}

/// The front app and window right now (the taskbar and other shell UI skipped), never-record apps marked.
fn front_now(a: &mut App) -> Option<Front> {
    let own = own_app_id();
    let user = Config::load(&a.paths.config()).exclude_apps;
    let is_excluded = |id: Option<&str>, name: &str| (id.is_some() && id == own.as_deref()) || Exclusion::is_excluded(id, Some(name), &user);
    windows_list::front(&mut a.apps, &is_excluded)
        .filter(|f| f.window.is_some() && !is_excluded(f.app_id.as_deref(), &f.app_name))
}

/// A clue from the window that was in front when the menu opened (works without a recording); else
/// the latest sample's. Same cleaning as recorded events.
fn current_clue(a: &mut App) -> Option<String> {
    let Some(front) = a.menu_front.take() else { return a.status.new_project_clue.clone() };
    let window = front.window.as_ref()?;
    let title = Redactor::clean(&windows_list::title(window.id));
    let address = crate::uia::browser_for(front.app_id.as_deref()).and_then(|browser| {
        let uia = a.uia.get_or_insert_with(Uia::new).as_ref()?;
        uia.address(HWND(window.id as usize as *mut core::ffi::c_void), browser)
    });
    Projects::clue(&title, &Location::clean(address.as_deref())).or_else(|| a.status.new_project_clue.clone())
}

/// Ask for a name (prefilled with the window's clue); the engine saves it and remembers the clue.
fn new_project(hwnd: HWND, status: &Status) {
    let clue = with_app(current_clue).flatten();
    let shown = Status { new_project_clue: clue.clone(), ..status.clone() };
    let initial = clue.clone().unwrap_or_default();
    // Outside the app borrow: the dialog runs a nested message loop.
    if let Some(name) = input::ask(hwnd, "New project", &shown.new_project_prompt(), &initial) {
        send(Msg::NewProject { name, clue });
    }
}

fn send(msg: Msg) {
    with_app(|a| a.tx.send(msg));
}

/// Quit: the engine ends any session and writes state.json, then tells us to close (`EngineDone`).
fn quit(hwnd: HWND) {
    let sent = with_app(|a| a.tx.send(Msg::Quit).is_ok()).unwrap_or(false);
    if !sent {
        close(hwnd);
    }
}

/// Windows is logging off: stop now and wait (briefly) for the engine to write its files.
fn quit_now() {
    let engine = with_app(|a| {
        a.log.write("windows is signing out");
        let _ = a.tx.send(Msg::Quit);
        a.tray.remove();
        a.engine.take()
    });
    if let Some(Some(engine)) = engine {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !engine.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
