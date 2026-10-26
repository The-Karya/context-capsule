// context-capsule — windows: the global shortcuts (Alt+C to start or stop, Alt+V for a voice note, or your own from
// config.json "hotkeys") — registered with RegisterHotKey, changed live from Settings → Shortcuts.
//! RegisterHotKey needs no permission. A combination another app already has can't be registered: at launch we log
//! it and say so once (the tray menu still works); when changing, it's refused and the old one stays.

use context_capsule_core::shortcuts::{Hotkeys, Key, Shortcut, Which};
use context_capsule_core::Msg;
use std::sync::RwLock;

const RECORD_ID: i32 = 1;
const VOICE_ID: i32 = 2;

/// The shortcuts in use now (any thread may read them: the tray, the window, voice messages).
static CURRENT: RwLock<Hotkeys> = RwLock::new(Hotkeys::DEFAULT);

pub fn current() -> Hotkeys {
    *CURRENT.read().unwrap_or_else(|e| e.into_inner())
}

fn set_current(h: Hotkeys) {
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = h;
}

/// What a WM_HOTKEY id asks the engine to do.
pub fn msg_for(id: usize) -> Option<Msg> {
    match id as i32 {
        RECORD_ID => Some(Msg::ToggleRecord),
        VOICE_ID => Some(Msg::ToggleVoice),
        _ => None,
    }
}

fn id_of(which: Which) -> i32 {
    match which {
        Which::Record => RECORD_ID,
        Which::Voice => VOICE_ID,
    }
}

/// The virtual-key code of a key: 'A'–'Z' and '0'–'9' are their ASCII codes, F1–F12 are 0x70–0x7B.
pub fn vk_of(key: Key) -> u32 {
    match key {
        Key::Letter(c) => c as u32,
        Key::Digit(d) => b'0' as u32 + d as u32,
        Key::F(n) => 0x6F + n as u32,
    }
}

/// RegisterHotKey's modifier flags (MOD_ALT 1, MOD_CONTROL 2, MOD_SHIFT 4), with MOD_NOREPEAT (0x4000).
pub fn mods_of(s: &Shortcut) -> u32 {
    let mut m = 0x4000;
    if s.alt {
        m |= 0x1;
    }
    if s.ctrl {
        m |= 0x2;
    }
    if s.shift {
        m |= 0x4;
    }
    m
}

#[cfg(windows)]
pub use win::{apply, register, resume, suspend};

#[cfg(windows)]
mod win {
    use super::*;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};

    fn grab(hwnd: HWND, which: Which, s: &Shortcut) -> bool {
        // SAFETY: registers a hotkey for our own window.
        unsafe { RegisterHotKey(Some(hwnd), id_of(which), HOT_KEY_MODIFIERS(mods_of(s)), vk_of(s.key)) }.is_ok()
    }

    fn release(hwnd: HWND, which: Which) {
        // SAFETY: unregisters our own hotkey (fine if it wasn't registered).
        unsafe {
            let _ = UnregisterHotKey(Some(hwnd), id_of(which));
        }
    }

    /// At launch: the shortcuts from config.json; returns the labels of those another app already has.
    pub fn register(hwnd: HWND, h: Hotkeys) -> Vec<String> {
        set_current(h);
        Which::ALL.into_iter().filter(|w| !grab(hwnd, *w, &h.get(*w))).map(|w| h.get(w).label()).collect()
    }

    /// Use `new` instead of the shortcuts in use. Err(which): another app has that one; nothing changed (the old
    /// ones are registered again).
    pub fn apply(hwnd: HWND, new: Hotkeys) -> Result<(), Which> {
        let old = current();
        for w in Which::ALL {
            release(hwnd, w);
        }
        let failed = Which::ALL.into_iter().find(|w| !grab(hwnd, *w, &new.get(*w)));
        if let Some(w) = failed {
            for w in Which::ALL {
                release(hwnd, w);
                grab(hwnd, w, &old.get(w));
            }
            return Err(w);
        }
        set_current(new);
        Ok(())
    }

    /// While a new shortcut is being recorded: the old ones would fire instead of reaching the window.
    pub fn suspend(hwnd: HWND) {
        for w in Which::ALL {
            release(hwnd, w);
        }
    }

    pub fn resume(hwnd: HWND) {
        let h = current();
        for w in Which::ALL {
            release(hwnd, w);
            grab(hwnd, w, &h.get(w));
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
        assert_eq!(current(), Hotkeys::DEFAULT, "Alt+C and Alt+V until config.json says otherwise");
    }

    #[test]
    fn keys_and_codes() {
        assert_eq!(vk_of(Key::Letter('C')), 0x43);
        assert_eq!(vk_of(Key::Digit(0)), 0x30);
        assert_eq!(vk_of(Key::F(1)), 0x70);
        assert_eq!(vk_of(Key::F(12)), 0x7B);
        let s = |t: &str| Shortcut::parse(t).unwrap();
        assert_eq!(mods_of(&s("Alt+C")), 0x4001);
        assert_eq!(mods_of(&s("Ctrl+Shift+R")), 0x4006);
        assert_eq!(mods_of(&s("F9")), 0x4000);
    }
}
