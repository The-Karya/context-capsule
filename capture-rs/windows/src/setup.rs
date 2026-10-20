// context-capsule — windows: the setup window — Voice notes, Connect Claude and Open at login, each with one
// button or a check mark when done, and a short "Use" list. Shown on first launch and from the tray menu.
//! A plain Win32 dialog (system font, standard buttons) built from an in-memory template, like the Mac's
//! welcome screen. It checks again every second while open. No "Read your apps" row: UI Automation
//! needs no permission on Windows.

use crate::hotkeys::{RECORD_LABEL, VOICE_LABEL};
use crate::input::{
    push_item, push_str, push_u32, BS_DEFPUSHBUTTON, BUTTON, DS_CENTER, DS_MODALFRAME, DS_SETFONT, SS_NOPREFIX, STATIC,
    WS_CAPTION, WS_POPUP, WS_SYSMENU, WS_TABSTOP,
};

/// Done = the dialog's IDOK, so Enter and Esc (IDCANCEL) both close it.
pub const ID_DONE: u16 = 1;
pub const ID_CLOSE: u16 = 2;
pub const ID_TITLE: u16 = 200;
pub const ID_SETUP_LABEL: u16 = 201;
pub const ID_USE_LABEL: u16 = 202;
/// Per setup row: 210 + row (icon), 220 + row (label), 230 + row (button), 240 + row (check mark).
pub const ID_ICON: u16 = 210;
pub const ID_LABEL: u16 = 220;
pub const ID_BUTTON: u16 = 230;
pub const ID_CHECK: u16 = 240;
/// Per "Use" row: 260 + row (what), 270 + row (keys).
pub const ID_USE: u16 = 260;
pub const ID_KEYS: u16 = 270;
const ID_LINE: u16 = 290;

const SS_RIGHT: u32 = 0x2;
const SS_CENTER: u32 = 0x1;
const SS_ETCHEDHORZ: u32 = 0x10;
const WS_MINIMIZEBOX: u32 = 0x0002_0000;
/// A taskbar button, so the window is easy to find again.
const WS_EX_APPWINDOW: u32 = 0x0004_0000;

/// The registry key and value that say the setup window was shown once.
pub const APP_KEY: &str = r"Software\Context Capsule";
pub const SEEN_VALUE: &str = "SetupShown";
/// Set once the app has turned "open at login" on by itself.
pub const LOGIN_SET_VALUE: &str = "LoginSet";

/// One setup step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Voice,
    Claude,
    Login,
}

impl Step {
    pub const ALL: [Step; 3] = [Step::Voice, Step::Claude, Step::Login];

    pub fn label(self) -> &'static str {
        match self {
            Step::Voice => "Voice notes",
            Step::Claude => "Connect Claude",
            Step::Login => "Open at login",
        }
    }

    pub fn button(self) -> &'static str {
        match self {
            Step::Voice => "Allow",
            Step::Claude => "Connect",
            Step::Login => "Turn On",
        }
    }

    /// A quiet icon from Segoe MDL2 Assets (on every Windows 10 and 11): microphone, link, power.
    pub fn icon(self) -> &'static str {
        match self {
            Step::Voice => "\u{E720}",
            Step::Claude => "\u{E71B}",
            Step::Login => "\u{E7E8}",
        }
    }

    pub fn index(self) -> u16 {
        Step::ALL.iter().position(|s| *s == self).unwrap_or(0) as u16
    }

    pub fn from_button(id: u16) -> Option<Step> {
        Step::ALL.get(id.checked_sub(ID_BUTTON)? as usize).copied()
    }
}

/// The check mark (Segoe MDL2 Assets).
pub const CHECK: &str = "\u{E73E}";
pub const ICON_FONT: &str = "Segoe MDL2 Assets";

/// What's done, checked every second while the window is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Done {
    pub voice: bool,
    pub claude: bool,
    pub login: bool,
}

impl Done {
    pub fn get(&self, step: Step) -> bool {
        match step {
            Step::Voice => self.voice,
            Step::Claude => self.claude,
            Step::Login => self.login,
        }
    }
}

/// The "Use" list: what, then the keys (or where).
pub fn uses() -> [(&'static str, &'static str); 3] {
    [("Start contextualizing", RECORD_LABEL), ("Talk", VOICE_LABEL), ("Make my capsules", "in Claude")]
}

// MARK: open at login (HKCU\...\Run)

pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Task Manager's Startup tab turns an entry off here without removing it.
pub const STARTUP_APPROVED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
pub const RUN_VALUE: &str = "Context Capsule";

/// The Run value for this exe.
pub fn run_command(exe: &str) -> String {
    format!("\"{exe}\"")
}

/// Does the Run value start this exe?
pub fn run_value_matches(value: &str, exe: &str) -> bool {
    let v = value.trim();
    let v = v.strip_prefix('"').and_then(|v| v.split_once('"')).map_or(v, |(path, _)| path);
    v.trim().eq_ignore_ascii_case(exe.trim())
}

/// StartupApproved data: none = on; else the first byte's lowest bit set = turned off.
pub fn startup_approved(data: Option<&[u8]>) -> bool {
    data.and_then(|d| d.first()).is_none_or(|b| b & 1 == 0)
}

// MARK: the dialog template

/// Places, in dialog units (x, y, width, height).
const W: i16 = 200;
const ROW: i16 = 22;
const ROWS_TOP: i16 = 42;
const USE_ROW: i16 = 15;

fn rows_bottom() -> i16 {
    ROWS_TOP + ROW * Step::ALL.len() as i16
}
fn use_top() -> i16 {
    rows_bottom() + 20
}
fn done_top() -> i16 {
    use_top() + USE_ROW * 3 + 8
}
pub fn height() -> i16 {
    done_top() + 15 + 9
}

/// Every control: (style, rect, id, class, text).
pub fn controls() -> Vec<(u32, [i16; 4], u16, u16, String)> {
    let mut c = vec![
        (SS_NOPREFIX, [10, 8, W - 20, 16], ID_TITLE, STATIC, "Context Capsule".to_string()),
        (SS_NOPREFIX, [10, 30, W - 20, 9], ID_SETUP_LABEL, STATIC, "Set up".to_string()),
    ];
    for step in Step::ALL {
        let (i, top) = (step.index(), ROWS_TOP + ROW * step.index() as i16);
        c.push((SS_NOPREFIX | SS_CENTER, [10, top + 4, 12, 12], ID_ICON + i, STATIC, step.icon().into()));
        c.push((SS_NOPREFIX, [28, top + 6, 100, 10], ID_LABEL + i, STATIC, step.label().into()));
        c.push((WS_TABSTOP, [W - 60, top + 3, 50, 15], ID_BUTTON + i, BUTTON, step.button().into()));
        c.push((SS_NOPREFIX | SS_RIGHT, [W - 30, top + 5, 20, 12], ID_CHECK + i, STATIC, CHECK.into()));
        if (i as usize) + 1 < Step::ALL.len() {
            c.push((SS_ETCHEDHORZ, [28, top + ROW - 1, W - 38, 1], ID_LINE + i, STATIC, String::new()));
        }
    }
    c.push((SS_NOPREFIX, [10, use_top() - 12, W - 20, 9], ID_USE_LABEL, STATIC, "Use".into()));
    for (j, (what, keys)) in uses().into_iter().enumerate() {
        let top = use_top() + USE_ROW * j as i16;
        c.push((SS_NOPREFIX, [10, top + 2, 90, 10], ID_USE + j as u16, STATIC, what.into()));
        c.push((SS_NOPREFIX | SS_RIGHT, [100, top + 2, W - 110, 10], ID_KEYS + j as u16, STATIC, keys.into()));
        if j < 2 {
            c.push((SS_ETCHEDHORZ, [10, top + USE_ROW - 1, W - 20, 1], ID_LINE + 10 + j as u16, STATIC, String::new()));
        }
    }
    c.push((WS_TABSTOP | BS_DEFPUSHBUTTON, [W - 60, done_top(), 50, 15], ID_DONE, BUTTON, "Done".into()));
    c
}

/// The DLGTEMPLATE, as 16-bit words (the caller places it DWORD aligned).
pub fn template() -> Vec<u16> {
    let controls = controls();
    let mut t = Vec::with_capacity(2048);
    push_u32(&mut t, WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | DS_MODALFRAME | DS_SETFONT | DS_CENTER);
    push_u32(&mut t, WS_EX_APPWINDOW);
    t.push(controls.len() as u16);
    t.extend([0u16, 0, W as u16, height() as u16]);
    t.extend([0, 0]); // no menu, default class
    push_str(&mut t, "Context Capsule");
    t.push(9);
    push_str(&mut t, "Segoe UI");
    for (style, rect, id, class, text) in &controls {
        push_item(&mut t, *style, *rect, *id, *class, text);
    }
    t
}

#[cfg(windows)]
pub use win::{open_at_login_once, seen, show, window};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::{connect, voice, win};
    use context_capsule_core::AppLog;
    use std::cell::{Cell, RefCell};
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        CreateFontW, DeleteObject, GetSysColor, GetSysColorBrush, SetBkMode, SetTextColor, COLOR_BTNFACE, COLOR_GRAYTEXT,
        DEFAULT_CHARSET, FONT_QUALITY, HDC, HFONT, TRANSPARENT,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateDialogIndirectParamW, DestroyWindow, GetDlgCtrlID, GetDlgItem, KillTimer, SendMessageW, SetForegroundWindow, SetTimer,
        ShowWindow, DLGTEMPLATE, SW_HIDE, SW_RESTORE, SW_SHOW, WM_CLOSE, WM_COMMAND, WM_CTLCOLORSTATIC, WM_DESTROY, WM_INITDIALOG,
        WM_SETFONT, WM_TIMER,
    };

    const REFRESH: usize = 1;

    thread_local! {
        static WINDOW: Cell<usize> = const { Cell::new(0) };
        static FONTS: RefCell<Vec<HFONT>> = const { RefCell::new(vec![]) };
        static LOG: RefCell<Option<AppLog>> = const { RefCell::new(None) };
    }

    /// The open setup window, for the message loop (keyboard navigation).
    pub fn window() -> Option<HWND> {
        let h = WINDOW.get();
        (h != 0).then_some(HWND(h as *mut core::ffi::c_void))
    }

    /// Open at login by default, so people never forget to start the app — once only: if they turn it off
    /// (Task Manager → Startup apps, or Settings), it stays off.
    pub fn open_at_login_once(log: &AppLog) {
        if win::reg_dword(APP_KEY, LOGIN_SET_VALUE) == Some(1) {
            return;
        }
        let _ = win::set_reg_dword(APP_KEY, LOGIN_SET_VALUE, 1);
        let on = win::set_reg_string(RUN_KEY, RUN_VALUE, &run_command(&exe()));
        log.write(if on { "open at login: on (first launch)" } else { "open at login: cannot change" });
    }

    /// Shown once already (first launch shows it by itself).
    pub fn seen() -> bool {
        win::reg_dword(APP_KEY, SEEN_VALUE) == Some(1)
    }

    /// Open the window, or bring it to the front.
    pub fn show(log: &AppLog) {
        LOG.with(|l| *l.borrow_mut() = Some(log.clone()));
        if let Some(h) = window() {
            // SAFETY: our own window.
            unsafe {
                let _ = ShowWindow(h, SW_RESTORE);
                let _ = SetForegroundWindow(h);
            }
            return;
        }
        let words = template();
        let mut aligned = vec![0u32; words.len().div_ceil(2)];
        for (i, w) in words.iter().enumerate() {
            aligned[i / 2] |= (*w as u32) << (16 * (i % 2));
        }
        // SAFETY: the template is DWORD aligned and outlives the call (Windows copies it).
        let created = unsafe {
            let Ok(instance) = GetModuleHandleW(None) else { return };
            CreateDialogIndirectParamW(Some(instance.into()), aligned.as_ptr() as *const DLGTEMPLATE, None, Some(proc), LPARAM(0))
        };
        let Ok(h) = created else {
            log.write("setup window: cannot open");
            return;
        };
        WINDOW.set(h.0 as usize);
        let _ = win::set_reg_dword(APP_KEY, SEEN_VALUE, 1);
        log.write("setup window: open");
        // SAFETY: our own window.
        unsafe {
            let _ = ShowWindow(h, SW_SHOW);
            let _ = SetForegroundWindow(h);
        }
    }

    fn log(message: &str) {
        LOG.with(|l| {
            if let Some(l) = &*l.borrow() {
                l.write(message)
            }
        });
    }

    fn exe() -> String {
        std::env::current_exe().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
    }

    fn done() -> Done {
        let login = win::reg_string(RUN_KEY, RUN_VALUE).is_some_and(|v| run_value_matches(&v, &exe()))
            && startup_approved(win::reg_binary(STARTUP_APPROVED_KEY, RUN_VALUE).as_deref());
        Done { voice: !voice::mic_blocked(), claude: connect::is_connected(), login }
    }

    fn act(step: Step) {
        match step {
            Step::Voice => {
                log("setup: opening microphone settings");
                win::open(voice::MIC_SETTINGS_URI);
            }
            Step::Claude => {
                // Not borrowed during the dialogs Connect may show (their loop can reach `show`).
                let log = LOG.with(|l| l.borrow().clone()).unwrap_or_else(AppLog::stderr_only);
                connect::connect(&log);
            }
            Step::Login => {
                let on = win::set_reg_string(RUN_KEY, RUN_VALUE, &run_command(&exe()));
                win::delete_reg_value(STARTUP_APPROVED_KEY, RUN_VALUE); // undo "Disabled" in Task Manager
                log(if on { "open at login: on" } else { "open at login: cannot change" });
                if !on {
                    win::alert(None, "Open at login", "Windows didn't allow this change. Try again, or start Context Capsule yourself.");
                }
            }
        }
    }

    fn item(dlg: HWND, id: u16) -> Option<HWND> {
        // SAFETY: a child of our own dialog.
        unsafe { GetDlgItem(Some(dlg), id as i32) }.ok()
    }

    /// Buttons for what's left, check marks for what's done.
    fn refresh(dlg: HWND) {
        let done = done();
        for step in Step::ALL {
            let on = done.get(step);
            let (button, check) = (item(dlg, ID_BUTTON + step.index()), item(dlg, ID_CHECK + step.index()));
            // SAFETY: children of our own dialog.
            unsafe {
                if let Some(b) = button {
                    let _ = ShowWindow(b, if on { SW_HIDE } else { SW_SHOW });
                }
                if let Some(c) = check {
                    let _ = ShowWindow(c, if on { SW_SHOW } else { SW_HIDE });
                }
            }
        }
    }

    fn font(dlg: HWND, points: i32, weight: i32, face: &str) -> HFONT {
        // SAFETY: plain GDI calls; the font is kept in FONTS and deleted with the window.
        unsafe {
            let dpi = GetDpiForWindow(dlg).max(96) as i32;
            let f = CreateFontW(-(points * dpi / 72), 0, 0, 0, weight, 0, 0, 0, DEFAULT_CHARSET, Default::default(), Default::default(),
                FONT_QUALITY(5), 0, &HSTRING::from(face));
            FONTS.with(|fs| fs.borrow_mut().push(f));
            f
        }
    }

    fn set_font(dlg: HWND, id: u16, f: HFONT) {
        // SAFETY: a child of our own dialog.
        unsafe {
            let _ = SendMessageW(item(dlg, id).unwrap_or_default(), WM_SETFONT, Some(WPARAM(f.0 as usize)), Some(LPARAM(1)));
        }
    }

    /// Secondary text (section names, keys, icons, check marks) in the system's grey.
    fn is_grey(id: u16) -> bool {
        matches!(id, ID_SETUP_LABEL | ID_USE_LABEL)
            || (ID_ICON..ID_ICON + 3).contains(&id)
            || (ID_CHECK..ID_CHECK + 3).contains(&id)
            || (ID_KEYS..ID_KEYS + 3).contains(&id)
    }

    unsafe extern "system" fn proc(dlg: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> isize {
        match msg {
            WM_INITDIALOG => {
                set_font(dlg, ID_TITLE, font(dlg, 14, 600, "Segoe UI"));
                let icons = font(dlg, 11, 400, ICON_FONT);
                for step in Step::ALL {
                    set_font(dlg, ID_ICON + step.index(), icons);
                    set_font(dlg, ID_CHECK + step.index(), icons);
                }
                refresh(dlg);
                // SAFETY: a timer on our own dialog.
                unsafe { SetTimer(Some(dlg), REFRESH, 1000, None) };
                1
            }
            WM_TIMER if wp.0 == REFRESH => {
                refresh(dlg);
                1
            }
            WM_COMMAND => {
                let id = (wp.0 & 0xFFFF) as u16;
                if let Some(step) = Step::from_button(id) {
                    act(step);
                    refresh(dlg);
                } else if id == ID_DONE || id == ID_CLOSE {
                    // SAFETY: our own dialog.
                    unsafe {
                        let _ = DestroyWindow(dlg);
                    }
                }
                1
            }
            WM_CLOSE => {
                // SAFETY: our own dialog.
                unsafe {
                    let _ = DestroyWindow(dlg);
                }
                1
            }
            WM_CTLCOLORSTATIC => {
                let child = HWND(lp.0 as *mut core::ffi::c_void);
                // SAFETY: the control's id and its DC, given by Windows for this message.
                unsafe {
                    let id = GetDlgCtrlID(child) as u16;
                    if !is_grey(id) {
                        return 0;
                    }
                    let dc = HDC(wp.0 as *mut core::ffi::c_void);
                    SetTextColor(dc, windows::Win32::Foundation::COLORREF(GetSysColor(COLOR_GRAYTEXT)));
                    SetBkMode(dc, TRANSPARENT);
                    GetSysColorBrush(COLOR_BTNFACE).0 as isize
                }
            }
            WM_DESTROY => {
                // SAFETY: our own timer and fonts.
                unsafe {
                    let _ = KillTimer(Some(dlg), REFRESH);
                }
                FONTS.with(|fs| {
                    for f in fs.borrow_mut().drain(..) {
                        // SAFETY: fonts we made; the window using them is going away.
                        unsafe {
                            let _ = DeleteObject(f.into());
                        }
                    }
                });
                WINDOW.set(0);
                log("setup window: closed");
                0
            }
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_and_words() {
        let labels: Vec<_> = Step::ALL.iter().map(|s| (s.label(), s.button())).collect();
        assert_eq!(labels, [("Voice notes", "Allow"), ("Connect Claude", "Connect"), ("Open at login", "Turn On")]);
        assert_eq!(Step::from_button(ID_BUTTON + 1), Some(Step::Claude));
        assert_eq!(Step::from_button(ID_BUTTON + 3), None);
        assert_eq!(Step::from_button(ID_DONE), None);
        assert_eq!(uses()[0], ("Start contextualizing", "Ctrl+Alt+Shift+C"));
        assert_eq!(uses()[1], ("Talk", "Ctrl+Alt+Shift+V"));
        assert_eq!(uses()[2], ("Make my capsules", "in Claude"));
        let d = Done { voice: true, claude: false, login: true };
        assert!(d.get(Step::Voice) && !d.get(Step::Claude) && d.get(Step::Login));
    }

    #[test]
    fn layout_fits_and_ids_are_unique() {
        let c = controls();
        let mut ids: Vec<u16> = c.iter().map(|x| x.2).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), c.len(), "unique ids");
        for (_, [x, y, w, h], id, _, _) in &c {
            assert!(*x >= 0 && *y >= 0 && x + w <= W && y + h <= height(), "control {id} inside the window");
        }
        // Buttons and check marks of one row share its place; rows don't overlap.
        for s in Step::ALL {
            let b = c.iter().find(|x| x.2 == ID_BUTTON + s.index()).unwrap().1;
            let k = c.iter().find(|x| x.2 == ID_CHECK + s.index()).unwrap().1;
            assert!(k[1] >= b[1] && k[1] + k[3] <= b[1] + b[3] + 2, "{s:?}: the check sits where the button was");
            assert!(b[1] + b[3] <= ROWS_TOP + ROW * (s.index() as i16 + 1), "{s:?} stays in its row");
        }
        let t = template();
        assert_eq!(t[4] as usize, c.len(), "control count in the header");
        let s = String::from_utf16_lossy(&t);
        for word in ["Context Capsule", "Set up", "Voice notes", "Connect Claude", "Open at login", "Use", "Start contextualizing", "Talk", "Done"] {
            assert!(s.contains(word), "{word}");
        }
    }

    #[test]
    fn run_at_login_values() {
        let exe = r"C:\Users\s\Desktop\kit\context-capsule.exe";
        assert_eq!(run_command(exe), format!("\"{exe}\""));
        assert!(run_value_matches(&run_command(exe), exe));
        assert!(run_value_matches(r#""C:\USERS\S\Desktop\kit\context-capsule.exe" --quiet"#, exe), "case and arguments");
        assert!(run_value_matches(exe, exe), "no quotes");
        assert!(!run_value_matches(r#""C:\old\context-capsule.exe""#, exe), "another copy");
        assert!(startup_approved(None), "never touched: on");
        assert!(startup_approved(Some(&[2, 0, 0, 0])));
        assert!(!startup_approved(Some(&[3, 0, 0, 0])), "turned off in Task Manager");
        assert!(startup_approved(Some(&[6])) && !startup_approved(Some(&[7])));
    }
}
