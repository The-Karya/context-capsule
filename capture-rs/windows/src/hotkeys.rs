// context-capsule — windows: the global hotkeys Ctrl+Alt+Shift+C (record) and Ctrl+Alt+Shift+V (voice note).
//! RegisterHotKey needs no permission. If another app already owns a combination, we log it and the
//! tray menu still works.

use context_capsule_core::Msg;

pub const RECORD_LABEL: &str = "Ctrl+Alt+Shift+C";
pub const VOICE_LABEL: &str = "Ctrl+Alt+Shift+V";

const RECORD_ID: i32 = 1;
const VOICE_ID: i32 = 2;

/// What a WM_HOTKEY id asks the engine to do.
pub fn msg_for(id: usize) -> Option<Msg> {
    match id as i32 {
        RECORD_ID => Some(Msg::ToggleRecord),
        VOICE_ID => Some(Msg::ToggleVoice),
        _ => None,
    }
}

#[cfg(windows)]
pub fn register(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT};
    let mods = MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT;
    for (id, key, label) in [(RECORD_ID, 'C', RECORD_LABEL), (VOICE_ID, 'V', VOICE_LABEL)] {
        // SAFETY: registers a hotkey for our own window.
        if unsafe { RegisterHotKey(Some(hwnd), id, mods, key as u32) }.is_err() {
            eprintln!("Context Capsule: {label} is taken by another app; use the tray menu instead");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_map_to_messages() {
        assert_eq!(msg_for(1), Some(Msg::ToggleRecord));
        assert_eq!(msg_for(2), Some(Msg::ToggleVoice));
        assert_eq!(msg_for(3), None);
    }
}
