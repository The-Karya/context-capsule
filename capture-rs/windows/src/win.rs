// context-capsule — windows: small Win32 helpers — registry values, message boxes, opening files and links,
// COM setup, the clipboard and the soft start/stop sounds.

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
