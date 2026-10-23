// context-capsule — windows: Automatic capsules (SPEC "Automatic capsules"; the decision and the run are
// core::auto). Here: the program (context-capsule-mcp.exe next to the app), whether a run's process is still there,
// the window's `auto` and `data-changed` events, and the notification when a run ends: a tray notification
// ("3 capsules ready" / "Couldn't make today's capsules" with the reason). Clicking it opens the Capsules page
// (which has Try again: tray notifications have no buttons). The engine checks once a minute; the tray window
// asks for a check at once when the computer wakes.

#[cfg(windows)]
pub use win::{get, start};

#[cfg(windows)]
mod win {
    use crate::app::{Ui, UiCmd};
    use context_capsule_core::anyai::Launch;
    use context_capsule_core::auto::{self, Auto, AutoHost, By, MakeResult};
    use context_capsule_core::bridge;
    use context_capsule_core::{AppLog, Paths};
    use serde_json::{json, Value};
    use std::sync::{Arc, OnceLock};
    use windows::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    static AUTO: OnceLock<Arc<Auto>> = OnceLock::new();

    /// The app's Automatic capsules (None before `start`).
    pub fn get() -> Option<Arc<Auto>> {
        AUTO.get().cloned()
    }

    /// At launch (the engine's first check is right away).
    pub fn start(paths: Paths, log: AppLog, ui: Ui) -> Arc<Auto> {
        AUTO.get_or_init(|| Auto::new(paths, log.clone(), Box::new(WinAuto { ui, log, launch: OnceLock::new() }))).clone()
    }

    struct WinAuto {
        ui: Ui,
        log: AppLog,
        launch: OnceLock<Option<Launch>>,
    }

    impl AutoHost for WinAuto {
        fn launch(&self) -> Option<Launch> {
            let log = self.log.clone();
            self.launch.get_or_init(|| crate::program::launch(&|l: &str| log.write(l))).clone()
        }

        fn alive(&self, pid: u32) -> bool {
            // SAFETY: opens the process only to read its exit code, and closes the handle.
            unsafe {
                let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
                let mut code = 0u32;
                let ok = GetExitCodeProcess(h, &mut code).is_ok();
                let _ = CloseHandle(h);
                ok && code == STILL_ACTIVE.0 as u32
            }
        }

        fn changed(&self, status: Value) {
            self.ui.post(UiCmd::Web(bridge::event_json("auto", status)));
        }

        fn finished(&self, by: By, result: &MakeResult) {
            self.ui.post(UiCmd::Web(bridge::event_json("data-changed", json!({ "what": ["capsules"] }))));
            if let Some((title, body)) = auto::notification(result) {
                self.ui.post(UiCmd::Notify(title, body));
            } else if by == By::Click {
                self.log.write(&format!("capsules by click: not made ({})", result.status));
            }
        }
    }
}
