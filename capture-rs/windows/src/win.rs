// context-capsule — windows: small Win32 helpers — wide strings, registry values, message boxes, COM setup.

use windows::core::HSTRING;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ};
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
#[cfg_attr(not(feature = "voice"), allow(dead_code))]
pub fn reg_string(subkey: &str, value: &str) -> Option<String> {
    let (key, name) = (HSTRING::from(subkey), HSTRING::from(value));
    let mut buf = [0u16; 512];
    let mut size = std::mem::size_of_val(&buf) as u32;
    // SAFETY: `buf` holds `size` bytes; RegGetValueW NUL-terminates.
    let ok = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, &key, &name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size))
    };
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

/// A message the user must dismiss.
pub fn alert(owner: Option<HWND>, title: &str, body: &str) {
    // SAFETY: modal dialog with our own strings.
    unsafe {
        MessageBoxW(owner, &HSTRING::from(body), &HSTRING::from(title), MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND | MB_TOPMOST);
    }
}

/// A yes / no question (any thread; it runs its own modal loop).
#[cfg_attr(not(feature = "voice"), allow(dead_code))]
pub fn ask_yes_no(title: &str, body: &str) -> bool {
    // SAFETY: modal dialog with our own strings.
    let answer = unsafe {
        MessageBoxW(None, &HSTRING::from(body), &HSTRING::from(title), MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND | MB_TOPMOST)
    };
    answer == IDYES
}
