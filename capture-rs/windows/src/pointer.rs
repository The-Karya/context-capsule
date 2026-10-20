// context-capsule — windows: the mouse while contextualizing — where it is (GetCursorPos) and when it last
// moved or scrolled (a low-level mouse hook on its own thread, only while recording; no permission needed).
//! The hook only notes the time; it never sees or keeps where you click or what you type.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// When the mouse last moved or scrolled, in ms since 1970 (0: not since recording started).
static LAST_MS: AtomicU64 = AtomicU64::new(0);

fn note_activity() {
    let ms = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    LAST_MS.store(ms.max(1), Ordering::Relaxed);
}

/// When the mouse last moved or scrolled while the hook was on.
pub fn last_activity() -> Option<SystemTime> {
    match LAST_MS.load(Ordering::Relaxed) {
        0 => None,
        ms => Some(UNIX_EPOCH + Duration::from_millis(ms)),
    }
}

#[cfg(windows)]
pub use win::{position, watch};

#[cfg(windows)]
mod win {
    use super::*;
    use std::sync::Mutex;
    use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetCursorPos, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
        HC_ACTION, MSG, PM_NOREMOVE, WH_MOUSE_LL, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_QUIT,
    };

    /// None: off. Some(None): on, but the hook could not be set. Some(Some(id)): the hook thread's id.
    static HOOK_THREAD: Mutex<Option<Option<u32>>> = Mutex::new(None);

    /// The mouse position in physical pixels (we are per-monitor DPI aware), like window frames.
    pub fn position() -> Option<(f64, f64)> {
        let mut p = POINT::default();
        // SAFETY: writes one POINT.
        unsafe { GetCursorPos(&mut p) }.ok()?;
        Some((p.x as f64, p.y as f64))
    }

    /// Watch mouse moves and scrolls (true, while recording) or stop (false). Safe to call repeatedly.
    pub fn watch(on: bool) {
        let mut state = HOOK_THREAD.lock().unwrap_or_else(|e| e.into_inner());
        if on == state.is_some() {
            return;
        }
        LAST_MS.store(0, Ordering::Relaxed);
        if let Some(thread) = state.take() {
            if let Some(id) = thread {
                // SAFETY: asks our hook thread to end its loop (it unhooks itself).
                unsafe {
                    let _ = PostThreadMessageW(id, WM_QUIT, WPARAM(0), LPARAM(0));
                }
            }
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("mouse-hook".into()).spawn(move || run(tx));
        *state = Some(spawned.ok().and_then(|_| rx.recv_timeout(Duration::from_secs(2)).ok().flatten()));
    }

    /// The hook's own thread: it must pump messages, and nothing else runs here, so it never stalls the mouse.
    fn run(started: std::sync::mpsc::Sender<Option<u32>>) {
        // SAFETY: a low-level mouse hook owned by this thread, removed before the thread ends.
        unsafe {
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE); // make the message queue
            let module = GetModuleHandleW(None).ok();
            let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(hook), module.map(Into::into), 0) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("Context Capsule: no mouse hook ({}); the mouse is noticed between samples only", e.message());
                    let _ = started.send(None);
                    return;
                }
            };
            let _ = started.send(Some(GetCurrentThreadId()));
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
            let _ = UnhookWindowsHookEx(hook);
        }
    }

    unsafe extern "system" fn hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 && matches!(wp.0 as u32, WM_MOUSEMOVE | WM_MOUSEWHEEL | WM_MOUSEHWHEEL) {
            note_activity();
        }
        // SAFETY: passes the event on, as every hook must.
        unsafe { CallNextHookEx(None, code, wp, lp) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_time() {
        LAST_MS.store(0, Ordering::Relaxed);
        assert_eq!(last_activity(), None);
        note_activity();
        let t = last_activity().unwrap();
        assert!(SystemTime::now().duration_since(t).unwrap() < Duration::from_secs(5));
    }
}
