// context-capsule — windows: small Win32 helpers — registry values, message boxes, opening files and links,
// COM setup, the clipboard (text in and out) and the soft start/stop sounds.

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_DWORD, REG_SZ,
    RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
};

/// COM for this thread: single-threaded for the UI thread, multithreaded for the engine (UIA, OCR, D3D).
pub fn init_com(ui_thread: bool) {
    let mode = if ui_thread { COINIT_APARTMENTTHREADED } else { COINIT_MULTITHREADED };
    // SAFETY: once per thread, before any COM use on it.
    let _ = unsafe { CoInitializeEx(None, mode) };
}

/// A string value under HKEY_CURRENT_USER.
pub fn reg_string(subkey: &str, value: &str) -> Option<String> {
    reg_string_in(HKEY_CURRENT_USER, subkey, value)
}

/// A string value under HKEY_CLASSES_ROOT (registered file types and link schemes).
pub fn reg_string_classes(subkey: &str, value: &str) -> Option<String> {
    reg_string_in(windows::Win32::System::Registry::HKEY_CLASSES_ROOT, subkey, value)
}

/// A string value under HKEY_LOCAL_MACHINE (read only).
pub fn reg_string_machine(subkey: &str, value: &str) -> Option<String> {
    reg_string_in(HKEY_LOCAL_MACHINE, subkey, value)
}

/// A string value under HKEY_LOCAL_MACHINE (`machine`) or HKEY_CURRENT_USER.
pub fn reg_string_at(machine: bool, subkey: &str, value: &str) -> Option<String> {
    reg_string_in(if machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER }, subkey, value)
}

/// The names of a key's subkeys, under HKEY_LOCAL_MACHINE (`machine`) or HKEY_CURRENT_USER. Missing: none.
pub fn reg_subkeys(machine: bool, subkey: &str) -> Vec<String> {
    use windows::core::PWSTR;
    use windows::Win32::System::Registry::{RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, KEY_READ};
    let hive = if machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER };
    let mut key = HKEY::default();
    let mut out = vec![];
    // SAFETY: opens a key for reading, enumerates into our own buffer (its length passed each time), closes it.
    unsafe {
        if RegOpenKeyExW(hive, &HSTRING::from(subkey), None, KEY_READ, &mut key).is_err() {
            return out;
        }
        for i in 0..10_000u32 {
            let mut buf = [0u16; 256];
            let mut len = buf.len() as u32;
            if RegEnumKeyExW(key, i, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None).is_err() {
                break;
            }
            out.push(String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]));
        }
        let _ = RegCloseKey(key);
    }
    out
}

fn reg_string_in(hive: HKEY, subkey: &str, value: &str) -> Option<String> {
    let (key, name) = (HSTRING::from(subkey), HSTRING::from(value));
    let mut buf = [0u16; 512];
    let mut size = std::mem::size_of_val(&buf) as u32;
    // SAFETY: `buf` holds `size` bytes; RegGetValueW NUL-terminates.
    let ok = unsafe { RegGetValueW(hive, &key, &name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) };
    if ok.is_err() {
        return None;
    }
    let chars = (size as usize / 2).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..chars]).trim_end_matches('\0').to_string())
}

/// A DWORD value under HKEY_CURRENT_USER.
pub fn reg_dword(subkey: &str, value: &str) -> Option<u32> {
    let (key, name) = (HSTRING::from(subkey), HSTRING::from(value));
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: `data` is one u32.
    let ok = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, &key, &name, RRF_RT_REG_DWORD, None, Some(&mut data as *mut u32 as *mut _), Some(&mut size))
    };
    ok.is_ok().then_some(data)
}

/// A binary value under HKEY_CURRENT_USER (at most 64 bytes).
pub fn reg_binary(subkey: &str, value: &str) -> Option<Vec<u8>> {
    let (key, name) = (HSTRING::from(subkey), HSTRING::from(value));
    let mut buf = [0u8; 64];
    let mut size = buf.len() as u32;
    // SAFETY: `buf` holds `size` bytes.
    let ok = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, &key, &name, RRF_RT_REG_BINARY, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size))
    };
    ok.is_ok().then(|| buf[..(size as usize).min(buf.len())].to_vec())
}

/// Write a string value under HKEY_CURRENT_USER (the key is made if needed).
pub fn set_reg_string(subkey: &str, value: &str, data: &str) -> bool {
    let wide: Vec<u16> = data.encode_utf16().chain(Some(0)).collect();
    // SAFETY: `wide` is NUL-terminated and its byte length is passed.
    let ok = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            &HSTRING::from(value),
            REG_SZ.0,
            Some(wide.as_ptr() as *const _),
            (wide.len() * 2) as u32,
        )
    };
    ok.is_ok()
}

/// Write a DWORD value under HKEY_CURRENT_USER (the key is made if needed).
pub fn set_reg_dword(subkey: &str, value: &str, data: u32) -> bool {
    // SAFETY: one u32.
    let ok = unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(subkey), &HSTRING::from(value), REG_DWORD.0, Some(&data as *const u32 as *const _), 4)
    };
    ok.is_ok()
}

/// Remove a value under HKEY_CURRENT_USER (missing is fine).
pub fn delete_reg_value(subkey: &str, value: &str) {
    // SAFETY: plain registry call with our own strings.
    let _ = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(subkey), &HSTRING::from(value)) };
}

/// Open a file, folder, web page or ms-settings: page with its default app. false: Windows could not.
pub fn open(target: &str) -> bool {
    open_with(target, None)
}

/// Run `program` (or open `target` when `program` is None) with `args`. false: Windows could not.
pub fn open_with(target: &str, args: Option<&str>) -> bool {
    let args = args.map(HSTRING::from);
    let params = args.as_ref().map_or(PCWSTR::null(), |a| PCWSTR(a.as_ptr()));
    // SAFETY: ShellExecuteW with our own NUL-terminated strings; values above 32 mean success.
    let result = unsafe { ShellExecuteW(None, w!("open"), &HSTRING::from(target), params, None, SW_SHOWNORMAL) };
    result.0 as isize > 32
}

/// A message the user must dismiss.
pub fn alert(owner: Option<HWND>, title: &str, body: &str) {
    // SAFETY: modal dialog with our own strings.
    unsafe {
        MessageBoxW(owner, &HSTRING::from(body), &HSTRING::from(title), MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND | MB_TOPMOST);
    }
}

/// A yes / no question (any thread; it runs its own modal loop).
pub fn ask_yes_no(title: &str, body: &str) -> bool {
    // SAFETY: modal dialog with our own strings.
    let answer = unsafe {
        MessageBoxW(None, &HSTRING::from(body), &HSTRING::from(title), MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND | MB_TOPMOST)
    };
    answer == IDYES
}

/// Put `text` on the clipboard (any thread). false: Windows would not.
pub fn copy_text(text: &str) -> bool {
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    const CF_UNICODETEXT: u32 = 13;
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    // SAFETY: the clipboard is opened and closed here; the memory is ours until SetClipboardData takes it
    // (then Windows owns it), and freed by us if that fails.
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let ok = (|| -> Option<()> {
            EmptyClipboard().ok()?;
            let mem: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).ok()?;
            let dst = GlobalLock(mem) as *mut u16;
            if dst.is_null() {
                let _ = GlobalFree(Some(mem));
                return None;
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
            let _ = GlobalUnlock(mem);
            if SetClipboardData(CF_UNICODETEXT, Some(HANDLE(mem.0))).is_err() {
                let _ = GlobalFree(Some(mem));
                return None;
            }
            Some(())
        })()
        .is_some();
        let _ = CloseClipboard();
        ok
    }
}

/// The text on the clipboard (any thread). None: no text there, or Windows would not share it (another app holds
/// the clipboard for a moment: tried a few times).
pub fn clipboard_text() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    const CF_UNICODETEXT: u32 = 13;
    // SAFETY: the clipboard is opened and closed here; the data is Windows' and only read while locked, up to its
    // size (and its first NUL).
    unsafe {
        IsClipboardFormatAvailable(CF_UNICODETEXT).ok()?;
        let mut open = false;
        for _ in 0..10 {
            if OpenClipboard(None).is_ok() {
                open = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        if !open {
            return None;
        }
        let text = (|| -> Option<String> {
            let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
            let mem = HGLOBAL(handle.0);
            let ptr = GlobalLock(mem) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let units = std::slice::from_raw_parts(ptr, GlobalSize(mem) / 2);
            let end = units.iter().position(|c| *c == 0).unwrap_or(units.len());
            let text = String::from_utf16_lossy(&units[..end]);
            let _ = GlobalUnlock(mem);
            Some(text)
        })();
        let _ = CloseClipboard();
        text
    }
}

/// Play a soft cue without waiting (made in code, kept for the life of the app).
pub fn play_cue(cue: context_capsule_core::Cue) {
    use std::sync::OnceLock;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    static WAVS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    let wavs = WAVS.get_or_init(|| context_capsule_core::Cue::ALL.iter().map(|c| c.wav()).collect());
    let Some(at) = context_capsule_core::Cue::ALL.iter().position(|c| *c == cue) else { return };
    // SAFETY: with SND_MEMORY the "name" is the WAV bytes, which live in a static for as long as it plays.
    unsafe {
        let _ = PlaySoundW(PCWSTR(wavs[at].as_ptr() as *const u16), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

/// Put a file on the clipboard, as File Explorer's Copy does (paste it into a chat, an email or a folder).
pub fn copy_file(path: &std::path::Path) -> bool {
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT};
    use windows::Win32::UI::Shell::DROPFILES;
    const CF_HDROP: u32 = 15;
    let name: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain([0, 0]).collect();
    let head = std::mem::size_of::<DROPFILES>();
    // SAFETY: as in `copy_text`: the memory is ours until SetClipboardData takes it; DROPFILES is followed by
    // the double-NUL-terminated wide path, inside the allocation.
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let ok = (|| -> Option<()> {
            EmptyClipboard().ok()?;
            let mem: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, head + name.len() * 2).ok()?;
            let dst = GlobalLock(mem) as *mut u8;
            if dst.is_null() {
                let _ = GlobalFree(Some(mem));
                return None;
            }
            let drop = DROPFILES { pFiles: head as u32, fWide: true.into(), ..Default::default() };
            std::ptr::copy_nonoverlapping(&drop as *const DROPFILES as *const u8, dst, head);
            std::ptr::copy_nonoverlapping(name.as_ptr() as *const u8, dst.add(head), name.len() * 2);
            let _ = GlobalUnlock(mem);
            if SetClipboardData(CF_HDROP, Some(HANDLE(mem.0))).is_err() {
                let _ = GlobalFree(Some(mem));
                return None;
            }
            Some(())
        })()
        .is_some();
        let _ = CloseClipboard();
        ok
    }
}

/// File Explorer, with the file selected.
pub fn show_in_folder(path: &std::path::Path) -> bool {
    open_with("explorer.exe", Some(&format!("/select,\"{}\"", path.display())))
}

/// The standard Save dialog. None: cancelled.
pub fn save_dialog(owner: HWND, file_name: &str) -> Option<std::path::PathBuf> {
    file_dialog(owner, file_name, true, "Capsule (*.md)\0*.md\0All files\0*.*\0\0", "md", None)
}

/// The standard Open dialog for a capsule someone sent (.md). None: cancelled.
pub fn open_capsule_dialog(owner: HWND) -> Option<std::path::PathBuf> {
    file_dialog(owner, "", false, "Capsule (*.md)\0*.md\0All files\0*.*\0\0", "md", None)
}

/// The standard Open dialog for an app (.exe). None: cancelled.
pub fn open_app_dialog(owner: HWND) -> Option<std::path::PathBuf> {
    let programs = std::env::var("ProgramFiles").unwrap_or_else(|_| r"C:\Program Files".into());
    file_dialog(owner, "", false, "Apps (*.exe)\0*.exe\0\0", "exe", Some(&programs))
}

fn file_dialog(owner: HWND, file_name: &str, save: bool, filter: &str, ext: &str, dir: Option<&str>) -> Option<std::path::PathBuf> {
    use windows::core::PWSTR;
    use windows::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    let mut buf = [0u16; 1024];
    for (d, s) in buf.iter_mut().zip(file_name.encode_utf16().take(1000)) {
        *d = s;
    }
    let filter: Vec<u16> = filter.encode_utf16().collect();
    let ext: Vec<u16> = ext.encode_utf16().chain(Some(0)).collect();
    let dir: Option<Vec<u16>> = dir.map(|d| d.encode_utf16().chain(Some(0)).collect());
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        lpstrDefExt: PCWSTR(ext.as_ptr()),
        lpstrInitialDir: dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
        Flags: OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST | if save { OFN_OVERWRITEPROMPT } else { OFN_FILEMUSTEXIST },
        ..Default::default()
    };
    // SAFETY: every buffer the struct points to lives until the call returns.
    let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } }.as_bool();
    if !ok {
        return None;
    }
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    Some(std::path::PathBuf::from(String::from_utf16_lossy(&buf[..end])))
}

/// The account's display name ("Saachi Badal"), else the user name.
pub fn display_name() -> String {
    use windows::core::PWSTR;
    use windows::Win32::Security::Authentication::Identity::{GetUserNameExW, NameDisplay};
    let mut buf = [0u16; 256];
    let mut len = buf.len() as u32;
    // SAFETY: writes at most `len` chars into `buf`.
    let ok = unsafe { GetUserNameExW(NameDisplay, Some(PWSTR(buf.as_mut_ptr())), &mut len) };
    if ok && len > 0 {
        let name = String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())]).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    std::env::var("USERNAME").unwrap_or_default()
}

fn locale_info(kind: u32) -> Option<String> {
    use windows::Win32::Globalization::GetLocaleInfoEx;
    let mut buf = [0u16; 128];
    // SAFETY: writes at most buf.len() chars; a null locale is the user's.
    let n = unsafe { GetLocaleInfoEx(PCWSTR::null(), kind, Some(&mut buf)) };
    (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
}

/// Weeks start on Sunday in the user's region format.
pub fn sunday_first() -> bool {
    locale_info(windows::Win32::Globalization::LOCALE_IFIRSTDAYOFWEEK).is_some_and(|d| d.trim() == "6")
}

/// Today as the user's long date, without the year ("Tuesday, 20 October").
pub fn long_date_today() -> String {
    long_date(chrono::Local::now().date_naive(), false)
}

/// A day as the user's long date (Settings → Time & language → Region), with or without the year: what Home and
/// the Timeline show.
pub fn long_date(day: chrono::NaiveDate, year: bool) -> String {
    use chrono::Datelike;
    use windows::Win32::Globalization::{GetDateFormatEx, ENUM_DATE_FORMATS_FLAGS, LOCALE_SLONGDATE};
    let full = locale_info(LOCALE_SLONGDATE);
    let pattern = match (full, year) {
        (Some(p), true) => p,
        (Some(p), false) => crate::layout::without_year(&p),
        (None, _) => return context_capsule_core::formats::day_month(day, year),
    };
    let picture: Vec<u16> = pattern.encode_utf16().chain(Some(0)).collect();
    let st = windows::Win32::Foundation::SYSTEMTIME { wYear: day.year() as u16, wMonth: day.month() as u16, wDay: day.day() as u16, ..Default::default() };
    let mut buf = [0u16; 128];
    // SAFETY: writes at most buf.len() chars; null locale = the user's.
    let n = unsafe { GetDateFormatEx(PCWSTR::null(), ENUM_DATE_FORMATS_FLAGS(0), Some(&st), PCWSTR(picture.as_ptr()), Some(&mut buf), PCWSTR::null()) };
    if n > 1 {
        String::from_utf16_lossy(&buf[..n as usize - 1])
    } else {
        context_capsule_core::formats::day_month(day, year)
    }
}

/// A time of day as the user's clock shows it ("4:49 PM" / "16:49"); `hour_only`: the hour mark ("4 PM" / "16").
fn clock_time(hour: u32, minute: u32, hour_only: bool) -> Option<String> {
    use windows::Win32::Globalization::{GetTimeFormatEx, TIME_FORMAT_FLAGS, TIME_NOMINUTESORSECONDS, TIME_NOSECONDS};
    let st = windows::Win32::Foundation::SYSTEMTIME { wYear: 2000, wMonth: 1, wDay: 1, wHour: hour as u16, wMinute: minute as u16, ..Default::default() };
    let flags: TIME_FORMAT_FLAGS = if hour_only { TIME_NOMINUTESORSECONDS } else { TIME_NOSECONDS };
    let mut buf = [0u16; 64];
    // SAFETY: writes at most buf.len() chars; null locale = the user's.
    let n = unsafe { GetTimeFormatEx(PCWSTR::null(), flags, Some(&st), PCWSTR::null(), Some(&mut buf)) };
    (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
}

/// From now on the window writes times and dates as Windows does for this user (12/24-hour clock, date order). Read
/// each time, so a change in Settings shows at the next repaint.
pub fn use_system_formats() {
    use context_capsule_core::formats::{self, Formats};
    formats::set(Formats {
        time: |h, m| clock_time(h, m, false).unwrap_or_else(|| formats::time_24(h, m)),
        hour: |h| clock_time(h, 0, true).unwrap_or_else(|| formats::hour_24(h)),
        date: long_date,
    });
}

/// The app icon (the Karya panther, inside the exe: res.rs) at `size` px; the caller destroys it. None in a build
/// without it (tests on other hosts never get here).
pub fn app_icon(size: i32) -> Option<windows::Win32::UI::WindowsAndMessaging::HICON> {
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{LoadImageW, HICON, IMAGE_ICON, LR_DEFAULTCOLOR};
    // SAFETY: loads our own resource by id (MAKEINTRESOURCE) at the asked size.
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let h = LoadImageW(Some(instance.into()), PCWSTR(crate::res::ICON_ID as usize as *const u16), IMAGE_ICON, size, size, LR_DEFAULTCOLOR).ok()?;
        Some(HICON(h.0))
    }
}

/// Give a dialog (install, update, reminder questions) the app icon in its title bar and taskbar button. The
/// system's shared copies at the standard sizes: never destroyed.
pub fn set_dialog_icon(hwnd: HWND) {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, LoadImageW, SendMessageW, ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_SHARED, SM_CXICON, SM_CXSMICON, WM_SETICON,
    };
    // SAFETY: shared icons from our own resources, sent to our own dialog.
    unsafe {
        let Ok(instance) = GetModuleHandleW(None) else { return };
        for (kind, metric) in [(ICON_SMALL, SM_CXSMICON), (ICON_BIG, SM_CXICON)] {
            let size = GetSystemMetrics(metric);
            if let Ok(h) = LoadImageW(Some(instance.into()), PCWSTR(crate::res::ICON_ID as usize as *const u16), IMAGE_ICON, size, size, LR_SHARED) {
                SendMessageW(hwnd, WM_SETICON, Some(WPARAM(kind as usize)), Some(LPARAM(h.0 as isize)));
            }
        }
    }
}
