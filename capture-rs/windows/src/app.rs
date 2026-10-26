// context-capsule — windows: the UI thread — a hidden owner window for the tray, hotkeys, menu, toasts,
// alerts, the Context Capsule window, the capture picker, the evening reminder and the any-AI state (`ai`); it starts
// the core engine on its own thread and relays messages. Start / stop cues (a short message and a soft sound) come from status changes;
// while recording or listening a timer keeps the tray's time and voice bars moving.

use crate::hotkeys;
use crate::picker::Capture;
use crate::platform::WinPlatform;
use crate::toast::Toast;
use crate::tray::{self, menu_text, Tray};
use crate::uia::Uia;
use crate::{ai, changes, input, main_window, reminder, setup, voice, win, windows_list};
use context_capsule_core::{
    hotkeys_taken_message, look, AppLog, Config, Cue, Engine, Exclusion, Front, Indicator, Location,
    Msg, Options, Paths, PlatformText, ProjectItem, Projects, Redactor, Status, Voice,
};
use context_capsule_core::claude;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Globalization::{GetTimeFormatEx, GetUserDefaultLocaleName, TIME_NOSECONDS};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Shell::{NINF_KEY, NIN_BALLOONUSERCLICK, NIN_SELECT};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, FindWindowW, GetCursorPos, GetMessageW, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW,
    SendMessageW, SetForegroundWindow, TrackPopupMenu, TranslateMessage, ASFW_ANY, HMENU, MF_CHECKED, MF_GRAYED, MF_POPUP,
    MF_SEPARATOR, MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_APP, WM_CLOSE,
    WM_CONTEXTMENU, WM_COPYDATA, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ENDSESSION, WM_HOTKEY, WM_NULL,
    WM_QUERYENDSESSION, WM_SETTINGCHANGE, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP, KillTimer, SetTimer,
    PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, WM_POWERBROADCAST,
};

const WM_TRAY: u32 = WM_APP + 1;
const WM_CMD: u32 = WM_APP + 2;

const ID_TOGGLE: u32 = 10;
const ID_VOICE: u32 = 11;
const ID_QUIT: u32 = 13;
const ID_OPEN: u32 = 14;
const ID_MAKE: u32 = 17;
const ID_SAVE: u32 = 18;
const ID_ADD_SCREEN: u32 = 15;
const ID_UPDATES: u32 = 16;
const ID_PROJECT_AUTO: u32 = 20;
const ID_PROJECT_NEW: u32 = 21;
/// Projects are 100, 101, … in list order.
const ID_PROJECT_FIRST: u32 = 100;
const MAX_PROJECTS: usize = 500;

/// While recording or listening: the tray's running time and voice bars, about 8 times a second.
const LOOK_TIMER: usize = 7;
const LOOK_EVERY_MS: u32 = 125;

/// User-facing words for Windows, with the shortcuts in use (Alt+C and Alt+V unless changed).
pub fn text() -> PlatformText {
    PlatformText { mic_privacy: voice::MIC_SETTINGS, ..PlatformText::WINDOWS }.with_shortcuts(&hotkeys::current())
}

/// What the engine thread asks the UI thread to do.
pub enum UiCmd {
    Indicator(Indicator),
    Status(Box<Status>),
    Toast(String),
    Alert(String, String),
    PresentPicker,
    /// "All my screens": every monitor at once, no picker (the picker if that fails).
    ShareAll,
    /// "Add a screen": the picker for one more screen or window.
    AddSource,
    /// Evening: "Make today's capsules?"
    Remind,
    /// Fresh numbers and lists for the Context Capsule window, read off the UI thread.
    WindowData(Box<crate::layout::Data>),
    /// One day of the Timeline, read off the UI thread.
    Timeline(Box<crate::layout::TimelineData>),
    /// A capsule someone sent was opened (Open with → Context Capsule, maybe forwarded by a second copy).
    OpenCapsule(PathBuf),
    /// Make today's capsules with Your AI (the window, or the evening reminder's Make).
    Make,
    /// A tray notification (title, body): Automatic capsules finished. Clicking it opens the Capsules page.
    Notify(String, String),
    /// News from an any-AI worker thread (the AI apps, Copy my day, Save from clipboard…).
    Ai(ai::AiEvent),
    EngineDone,
    /// An answer or event for the shared window's page (JSON).
    Web(String),
    /// Something the shared window's bridge needs done on the UI thread (a dialog, the clipboard, hotkeys).
    Run(Box<dyn FnOnce() + Send>),
}

/// WM_COPYDATA from a second copy started with `--open <path>`: dwData says what the bytes are (UTF-16 path).
const COPYDATA_OPEN: usize = 0x4343_4F50; // "CCOP"

/// Started with `--open <path>` while Context Capsule already runs: hand the path to it. false: no running copy.
fn forward_open(path: &Path) -> bool {
    // SAFETY: looks up a window class by name.
    let Ok(hwnd) = (unsafe { FindWindowW(w!("ContextCapsuleTray"), None) }) else { return false };
    if hwnd.is_invalid() {
        return false;
    }
    let wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().collect();
    let data = COPYDATASTRUCT { dwData: COPYDATA_OPEN, cbData: (wide.len() * 2) as u32, lpData: wide.as_ptr() as *mut _ };
    // SAFETY: `data` and `wide` live until SendMessage returns (the receiver copies them). The running copy may
    // bring Claude or a message to the front.
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
        SendMessageW(hwnd, WM_COPYDATA, None, Some(LPARAM(&data as *const COPYDATASTRUCT as isize))).0 == 1
    }
}

/// The path in a WM_COPYDATA from `forward_open`.
///
/// # Safety
/// `lp` must be the LPARAM of a WM_COPYDATA message being handled now.
unsafe fn copied_path(lp: LPARAM) -> Option<PathBuf> {
    // SAFETY: Windows passes a COPYDATASTRUCT valid for this message; cbData bytes at lpData.
    let data = unsafe { (lp.0 as *const COPYDATASTRUCT).as_ref()? };
    if data.dwData != COPYDATA_OPEN || data.lpData.is_null() || data.cbData == 0 || data.cbData > 64 * 1024 {
        return None;
    }
    // SAFETY: as above.
    let wide = unsafe { std::slice::from_raw_parts(data.lpData as *const u16, data.cbData as usize / 2) };
    Some(PathBuf::from(String::from_utf16_lossy(wide)))
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
    /// The UI thread's hidden window (it owns the tray and the global shortcuts).
    pub fn hwnd(&self) -> HWND {
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
    // Opened with a capsule (Open with → Context Capsule, or `--open <path>`).
    let args: Vec<String> = std::env::args().collect();
    let open = claude::open_arg(&args, &std::env::current_dir().unwrap_or_default());
    // One recorder at a time.
    // SAFETY: a named mutex held for the life of the process.
    let _single = unsafe { CreateMutexW(None, true, w!("Local\\ContextCapsuleRecorder")) };
    // SAFETY: reads this thread's last error.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // The running one opens the capsule.
        if open.as_deref().is_some_and(forward_open) {
            return;
        }
        win::alert(None, context_capsule_core::window::APP_NAME, "Context Capsule is already running. Look for its icon in the tray.");
        return;
    }
    win::init_com(true);
    win::use_system_formats();

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
    // Your AI as saved; the AI apps are asked for once the app runs (below).
    ai::init(paths.config());
    let (tx, rx) = channel();
    let capture = Capture::new(tx.clone());
    let ui = Ui::of(hwnd);
    // Automatic capsules: the engine checks once a minute (the first time right away).
    let auto = crate::auto::start(paths.clone(), log.clone(), ui.clone());

    let taken = hotkeys::register(hwnd, context_capsule_core::Hotkeys::load(&paths.config()));
    let mut tray = Tray::new(hwnd, WM_TRAY);
    tray.set_tip(tray::tooltip(&Status::default(), SystemTime::now(), &hotkeys::current()));
    tray.add();
    changes::install();
    let engine = spawn_engine(paths.clone(), ui, capture.clone(), tx.clone(), rx, log.clone(), auto);
    setup::open_at_login_once(&log);
    crate::update::start(hwnd, log.clone());
    crate::web_window::apply_menu_mode(&paths);

    ai::refresh(&Ui::of(hwnd), &log);
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
    if let Some(path) = open {
        Ui::of(hwnd).post(UiCmd::OpenCapsule(path));
    } else if let Some(show) = crate::web_window::show_arg(&args) {
        // For testing: open the window on a page.
        crate::web_window::set_query(crate::web_window::show_query(show.date, show.open.as_deref()));
        open_window(hwnd, Some(show.section));
        main_window::show_day(show.date, show.open.as_deref());
    } else if !setup::seen() {
        open_window(hwnd, Some(context_capsule_core::window::Section::Settings)); // first launch
    }
    if !taken.is_empty() {
        // Once, at launch: the keys don't work, the tray menu does.
        with_app(|a| a.log.write(&format!("hotkeys taken by another app: {}", taken.join(", "))));
        let taken: Vec<&str> = taken.iter().map(String::as_str).collect();
        Ui::of(hwnd).post(UiCmd::Toast(hotkeys_taken_message(&taken)));
    }

    let mut msg = MSG::default();
    // SAFETY: the standard message loop for this thread.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
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
            w!("Karya Context Capsule"),
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
    auto: Arc<context_capsule_core::auto::Auto>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("engine".into())
        .spawn(move || {
            win::init_com(false);
            let toast_ui = ui.clone();
            let notify: voice::Notify = Arc::new(move |text: String| toast_ui.post(UiCmd::Toast(text)));
            let voice = voice::engine(&paths.root, notify, log);
            let opts = Options { own_app_id: own_app_id(), text: text(), locale: user_locale() };
            let mut engine = Engine::new(paths, WinPlatform::new(ui.clone(), capture), voice, opts);
            engine.set_voice_wake(Arc::new(move || {
                let _ = tx.send(Msg::VoiceReady);
            }));
            engine.set_auto(auto);
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
pub(crate) fn user_locale() -> String {
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
            } else if event == NIN_BALLOONUSERCLICK {
                // Our only notifications are Automatic capsules': their page.
                open_window(hwnd, Some(context_capsule_core::window::Section::Capsules));
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
        // Woke from sleep: Automatic capsules checks now (a run missed while asleep starts the same day).
        WM_POWERBROADCAST if wp.0 as u32 == PBT_APMRESUMEAUTOMATIC || wp.0 as u32 == PBT_APMRESUMESUSPEND => {
            if let Some(auto) = crate::auto::get() {
                auto.wake();
            }
            return LRESULT(1);
        }
        WM_TIMER if wp.0 == LOOK_TIMER => {
            with_app(|a| look_tick(hwnd, a));
        }
        // Light / dark taskbar, or a new display scale (the icons are drawn for the taskbar's icon size).
        WM_SETTINGCHANGE | WM_DISPLAYCHANGE | WM_DPICHANGED => {
            with_app(|a| a.tray.retheme());
        }
        // A second copy, opened with a capsule, hands it to us.
        WM_COPYDATA => {
            // SAFETY: this is a WM_COPYDATA being handled now.
            let Some(path) = (unsafe { copied_path(lp) }) else { return LRESULT(0) };
            Ui::of(hwnd).post(UiCmd::OpenCapsule(path)); // after the sender is released
            return LRESULT(1);
        }
        // Another copy (install, uninstall) or "Restart now" asks us to quit: end cleanly, like Quit.
        WM_CLOSE => quit(hwnd),
        WM_QUERYENDSESSION => return LRESULT(1),
        WM_ENDSESSION if wp.0 != 0 => quit_now(),
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            with_app(|a| {
                a.tray.make_icons(); // the taskbar may be on another monitor now
                a.tray.add();
            });
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
                a.tray.set_tip(tray::tooltip(&s, SystemTime::now(), &hotkeys::current()));
                crate::update::set_busy(s.recording || s.listening); // no update check meanwhile
                cues(a, &s);
                a.status = *s;
                look_tick(hwnd, a);
                main_window::set_status(&a.status);
                crate::web_window::set_status(&a.status);
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
        UiCmd::ShareAll => {
            if let Some((capture, log)) = with_app(|a| (a.capture.clone(), a.log.clone())) {
                capture.present_all(hwnd, &log);
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
        UiCmd::WindowData(data) => main_window::set_data(*data),
        UiCmd::Timeline(data) => main_window::set_timeline(*data),
        UiCmd::OpenCapsule(path) => {
            if let Some(log) = with_app(|a| a.log.clone()) {
                ai::open_received(&Ui::of(hwnd), &log, &path);
            }
        }
        UiCmd::Make => make_capsules(hwnd),
        UiCmd::Notify(title, body) => {
            with_app(|a| a.tray.notify(&title, &body));
        }
        // Outside the app borrow: Connect for Claude may show a dialog.
        UiCmd::Ai(ev) => {
            if matches!(ev, ai::AiEvent::Clients(_)) {
                crate::web_window::clients_changed();
            }
            if let Some(log) = with_app(|a| a.log.clone()) {
                ai::handle(ev, &Ui::of(hwnd), &log);
            }
        }
        // Quitting: let an open alert (e.g. "Nothing was saved") be read first.
        UiCmd::EngineDone if ALERTS.get() > 0 => CLOSE_PENDING.set(true),
        UiCmd::EngineDone => close(hwnd),
        UiCmd::Web(json) => crate::web_window::deliver(&json),
        UiCmd::Run(f) => f(),
    }
}

/// Clear cues when recording or listening starts or stops (Mac parity): a soft sound ("sounds": false turns
/// them off) and the tray icon. No text on screen.
fn cues(a: &App, next: &Status) {
    let changes = Cue::changes(&a.status, next);
    if !changes.is_empty() && Config::load(&a.paths.config()).sounds {
        changes.into_iter().for_each(win::play_cue);
    }
}

/// The tray while recording or listening: the time in the tooltip (the tray can't show text by its icon), and
/// the voice bars following the mic. The timer runs only then.
fn look_tick(hwnd: HWND, a: &mut App) {
    let busy = a.status.recording || a.status.listening;
    if a.status.listening {
        a.tray.set_level(look::level_frame(Voice::level()));
    }
    a.tray.set_tip(tray::tooltip(&a.status, SystemTime::now(), &hotkeys::current()));
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
    main_window::close();
    crate::web_window::close();
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
        (tray::menu_model(&a.status, SystemTime::now(), &short_time, &hotkeys::current()), a.status.clone())
    }) else {
        return;
    };
    let projects = status.project_items();
    // Save from clipboard: only when Your AI is a website or any other AI.
    let save = ai::with(|s| s.kind().uses_copy());
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
        item(menu, ID_OPEN, context_capsule_core::window::OPEN_WINDOW, true);
        item(menu, ID_MAKE, context_capsule_core::window::MAKE_TODAYS, true);
        if save {
            item(menu, ID_SAVE, context_capsule_core::anyai::SAVE_FROM_CLIPBOARD, true);
        }
        item(menu, ID_UPDATES, context_capsule_core::update::CHECK_LABEL, true);
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
        ID_OPEN => open_window(hwnd, None),
        ID_MAKE => make_capsules(hwnd),
        ID_SAVE => {
            if let Some(log) = with_app(|a| a.log.clone()) {
                ai::save_from_clipboard(&Ui::of(hwnd), &log);
            }
        }
        ID_UPDATES => crate::update::check_now(),
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

/// Make today's capsules with Your AI (Claude with the request typed in, another app with it on the clipboard, or
/// Copy my day); stop contextualizing first if it's on (unless a voice note is listening).
fn make_capsules(hwnd: HWND) {
    let Some(log) = with_app(|a| {
        if a.status.recording && !a.status.listening {
            a.log.write("stop: make today's capsules");
            let _ = a.tx.send(Msg::ToggleRecord);
        }
        a.log.clone()
    }) else {
        return;
    };
    ai::make(&Ui::of(hwnd), &log);
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

/// "New project" from the shared window: the latest sample's clue, then the name dialog.
pub fn new_project_now(hwnd: HWND) {
    if let Some(status) = with_app(|a| a.status.clone()) {
        new_project(hwnd, &status);
    }
}

/// The shortcuts changed (Settings → Shortcuts): the engine's messages and the tray name the new ones.
pub fn shortcuts_changed(h: context_capsule_core::Hotkeys) {
    with_app(|a| {
        a.log.write(&format!("shortcuts: {} and {}", h.record, h.voice));
        let _ = a.tx.send(Msg::Shortcuts(h));
        a.tray.set_tip(tray::tooltip(&a.status, SystemTime::now(), &h));
    });
}

/// Open the Context Capsule window (on `section`, when given), or bring it to the front.
fn open_window(hwnd: HWND, section: Option<context_capsule_core::window::Section>) {
    let Some((ctx, status)) = with_app(|a| {
        (crate::web_window::Ctx { ui: Ui::of(hwnd), tx: a.tx.clone(), log: a.log.clone(), paths: a.paths.clone() }, a.status.clone())
    }) else {
        return;
    };
    let config = std::fs::read(ctx.paths.config()).ok();
    if crate::web_window::wanted(&std::env::args().collect::<Vec<_>>(), config.as_deref()) && crate::web_window::show(section, ctx.clone(), &status) {
        return;
    }
    main_window::show(section, ctx, &status);
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
