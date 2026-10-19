// context-capsule — windows: when the screen last switched — app switches, Alt-Tab and virtual-desktop
// switches. Frames from before then may still show the previous app, so OCR waits for a newer one.

use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_SWITCHEND,
    EVENT_SYSTEM_SWITCHSTART, GA_ROOT, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
};

/// Milliseconds since 1970 of the last switch; 0 = none yet.
static LAST: AtomicI64 = AtomicI64::new(0);

/// Install the hooks (UI thread: events arrive through its message loop).
pub fn install() {
    let ranges = [
        (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
        (EVENT_SYSTEM_SWITCHSTART, EVENT_SYSTEM_SWITCHEND),
        // Switching virtual desktops cloaks one set of windows and uncloaks another.
        (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
    ];
    for (min, max) in ranges {
        // SAFETY: an out-of-context hook with a plain callback; it lives as long as the process.
        let hook = unsafe { SetWinEventHook(min, max, None, Some(on_event), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS) };
        if hook.is_invalid() {
            eprintln!("Context Capsule: cannot watch window switches ({min:#x})");
        }
    }
}

pub fn last() -> Option<SystemTime> {
    match LAST.load(Ordering::Relaxed) {
        0 => None,
        ms => Some(UNIX_EPOCH + Duration::from_millis(ms as u64)),
    }
}

unsafe extern "system" fn on_event(_: HWINEVENTHOOK, event: u32, hwnd: HWND, object: i32, _: i32, _: u32, _: u32) {
    if event == EVENT_OBJECT_CLOAKED || event == EVENT_OBJECT_UNCLOAKED {
        // Only whole top-level windows, not parts of them.
        // SAFETY: a read-only query on the event's window.
        if object != OBJID_WINDOW.0 || hwnd.is_invalid() || unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
            return;
        }
    }
    LAST.store(context_capsule_core::clock::millis(SystemTime::now()), Ordering::Relaxed);
}
