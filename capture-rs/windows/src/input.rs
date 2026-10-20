// context-capsule — windows: a tiny modal "type a name" dialog (New project from this window…), built
// from an in-memory dialog template: a line of text, one edit field, Create and Cancel.

/// Control ids. Create and Cancel are the dialog's IDOK / IDCANCEL, so Enter and Esc work.
pub const ID_OK: u16 = 1;
pub const ID_CANCEL: u16 = 2;
pub const ID_EDIT: u16 = 100;
pub const ID_PROMPT: u16 = 101;
/// Longest name accepted.
pub const MAX_CHARS: usize = 100;

// Window and control styles (WinUser.h), as plain numbers.
pub(crate) const WS_POPUP: u32 = 0x8000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
pub(crate) const WS_CAPTION: u32 = 0x00C0_0000;
const WS_BORDER: u32 = 0x0080_0000;
pub(crate) const WS_SYSMENU: u32 = 0x0008_0000;
pub(crate) const WS_TABSTOP: u32 = 0x0001_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
pub(crate) const DS_SETFONT: u32 = 0x40;
pub(crate) const DS_MODALFRAME: u32 = 0x80;
const DS_SETFOREGROUND: u32 = 0x200;
pub(crate) const DS_CENTER: u32 = 0x800;
pub(crate) const SS_NOPREFIX: u32 = 0x80;
const ES_AUTOHSCROLL: u32 = 0x80;
pub(crate) const BS_DEFPUSHBUTTON: u32 = 0x1;
// Predefined control classes (ordinal atoms).
pub(crate) const BUTTON: u16 = 0x0080;
const EDIT: u16 = 0x0081;
pub(crate) const STATIC: u16 = 0x0082;

pub(crate) fn push_u32(t: &mut Vec<u16>, v: u32) {
    t.extend([(v & 0xFFFF) as u16, (v >> 16) as u16]);
}

pub(crate) fn push_str(t: &mut Vec<u16>, s: &str) {
    t.extend(s.encode_utf16().filter(|c| *c != 0));
    t.push(0);
}

/// One control: DWORD aligned; style, ex style, x, y, cx, cy (dialog units), id, class atom, text.
pub(crate) fn push_item(t: &mut Vec<u16>, style: u32, rect: [i16; 4], id: u16, class: u16, text: &str) {
    if t.len() % 2 == 1 {
        t.push(0);
    }
    push_u32(t, style | WS_CHILD | WS_VISIBLE);
    push_u32(t, 0);
    t.extend(rect.iter().map(|v| *v as u16));
    t.push(id);
    t.extend([0xFFFF, class]);
    push_str(t, text);
    t.push(0); // no creation data
}

/// A DLGTEMPLATE with its four controls, as 16-bit words (the caller places it DWORD aligned).
pub fn template(title: &str, prompt: &str) -> Vec<u16> {
    let mut t = Vec::with_capacity(256);
    push_u32(&mut t, WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_MODALFRAME | DS_SETFONT | DS_CENTER | DS_SETFOREGROUND);
    push_u32(&mut t, WS_EX_TOPMOST);
    t.push(4); // controls
    t.extend([0u16, 0, 240, 80]); // x, y, cx, cy
    t.extend([0, 0]); // no menu, default class
    push_str(&mut t, title);
    t.push(9); // point size
    push_str(&mut t, "Segoe UI");
    push_item(&mut t, SS_NOPREFIX, [7, 7, 226, 24], ID_PROMPT, STATIC, prompt);
    push_item(&mut t, WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL, [7, 34, 226, 14], ID_EDIT, EDIT, "");
    push_item(&mut t, WS_TABSTOP | BS_DEFPUSHBUTTON, [129, 58, 50, 14], ID_OK, BUTTON, "Create");
    push_item(&mut t, WS_TABSTOP, [183, 58, 50, 14], ID_CANCEL, BUTTON, "Cancel");
    t
}

/// What the user typed, trimmed; None for an empty name.
pub fn answer(typed: &str) -> Option<String> {
    let t = typed.trim();
    (!t.is_empty()).then(|| t.chars().take(MAX_CHARS).collect())
}

#[cfg(windows)]
pub use win::ask;

#[cfg(windows)]
mod win {
    use super::*;
    use std::cell::RefCell;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows::Win32::UI::WindowsAndMessaging::{
        DialogBoxIndirectParamW, EndDialog, GetDlgItem, GetDlgItemTextW, SendDlgItemMessageW, SetDlgItemTextW, DLGTEMPLATE,
        WM_COMMAND, WM_INITDIALOG,
    };

    const EM_SETSEL: u32 = 0x00B1;
    const EM_LIMITTEXT: u32 = 0x00C5;

    thread_local! {
        static TYPED: RefCell<String> = const { RefCell::new(String::new()) };
    }

    /// Ask for a line of text (blocks in a nested modal loop on the UI thread). None: cancelled or empty.
    pub fn ask(owner: HWND, title: &str, prompt: &str, initial: &str) -> Option<String> {
        let words = template(title, prompt);
        // DWORD aligned, as DialogBoxIndirect needs.
        let mut aligned = vec![0u32; words.len().div_ceil(2)];
        for (i, w) in words.iter().enumerate() {
            aligned[i / 2] |= (*w as u32) << (16 * (i % 2));
        }
        let initial: Vec<u16> = initial.encode_utf16().chain([0]).collect();
        TYPED.with(|t| t.borrow_mut().clear());
        // SAFETY: the template and the initial text outlive the modal call; the dialog proc reads the
        // initial text only during WM_INITDIALOG.
        let result = unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            DialogBoxIndirectParamW(
                Some(instance.into()),
                aligned.as_ptr() as *const DLGTEMPLATE,
                Some(owner),
                Some(proc),
                LPARAM(initial.as_ptr() as isize),
            )
        };
        if result != ID_OK as isize {
            return None;
        }
        answer(&TYPED.with(|t| t.borrow().clone()))
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> isize {
        // SAFETY: standard dialog calls on our own dialog and its controls.
        unsafe {
            match msg {
                WM_INITDIALOG => {
                    let _ = SetDlgItemTextW(hwnd, ID_EDIT as i32, PCWSTR(lp.0 as *const u16));
                    SendDlgItemMessageW(hwnd, ID_EDIT as i32, EM_LIMITTEXT, WPARAM(MAX_CHARS), LPARAM(0));
                    SendDlgItemMessageW(hwnd, ID_EDIT as i32, EM_SETSEL, WPARAM(0), LPARAM(-1));
                    if let Ok(edit) = GetDlgItem(Some(hwnd), ID_EDIT as i32) {
                        let _ = SetFocus(Some(edit));
                    }
                    0 // we set the focus ourselves
                }
                WM_COMMAND => {
                    let id = (wp.0 & 0xFFFF) as u16;
                    if id == ID_OK {
                        let mut buf = [0u16; MAX_CHARS + 1];
                        let n = GetDlgItemTextW(hwnd, ID_EDIT as i32, &mut buf) as usize;
                        TYPED.with(|t| *t.borrow_mut() = String::from_utf16_lossy(&buf[..n.min(MAX_CHARS)]));
                    }
                    if id == ID_OK || id == ID_CANCEL {
                        let _ = EndDialog(hwnd, id as isize);
                        return 1;
                    }
                    0
                }
                _ => 0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_at(t: &[u16], at: usize) -> String {
        let end = t[at..].iter().position(|c| *c == 0).unwrap() + at;
        String::from_utf16(&t[at..end]).unwrap()
    }

    #[test]
    fn template_layout() {
        let t = template("New project", "Name the project.");
        let style = t[0] as u32 | (t[1] as u32) << 16;
        assert_eq!(style & DS_SETFONT, DS_SETFONT, "font follows the title");
        assert_eq!(t[4], 4, "four controls");
        assert_eq!(&t[5..9], &[0, 0, 240, 80]);
        assert_eq!(&t[9..11], &[0, 0], "no menu, default class");
        assert_eq!(text_at(&t, 11), "New project");
        let after_title = 11 + "New project".len() + 1;
        assert_eq!(t[after_title], 9, "9 pt");
        assert_eq!(text_at(&t, after_title + 1), "Segoe UI");
        // The first control starts on a DWORD boundary right after the font name.
        let mut first = after_title + 1 + "Segoe UI".len() + 1;
        if first % 2 == 1 {
            first += 1;
        }
        assert_eq!(t[first + 8], ID_PROMPT);
        assert_eq!(&t[first + 9..first + 11], &[0xFFFF, STATIC]);
        assert_eq!(text_at(&t, first + 11), "Name the project.");
        let s = String::from_utf16_lossy(&t);
        assert!(s.contains("Create") && s.contains("Cancel"));
    }

    #[test]
    fn controls_are_dword_aligned() {
        // Odd-length strings force padding; every control's id sits 8 words after its start.
        for (title, prompt) in [("A", "B"), ("AB", "CD"), ("New project", "Windows with “x” will count")] {
            let t = template(title, prompt);
            let ids: Vec<usize> = t.windows(3).enumerate().filter(|(_, w)| w[1] == 0xFFFF && [BUTTON, EDIT, STATIC].contains(&w[2])).map(|(i, _)| i).collect();
            assert_eq!(ids.len(), 4, "{title}");
            for id_at in ids {
                assert_eq!((id_at - 8) % 2, 0, "{title}: control at an even word");
            }
        }
    }

    #[test]
    fn answers() {
        assert_eq!(answer("  Client report "), Some("Client report".into()));
        assert_eq!(answer("   "), None);
        assert_eq!(answer(&"x".repeat(150)).unwrap().len(), MAX_CHARS);
    }
}
