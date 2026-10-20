// context-capsule — windows: updates — the core's signed feed and checked download over WinINet, a check a
// minute after launch and daily (never while contextualizing or during a voice note), and "Context Capsule
// 0.2.0 is ready. Restart to update." Restart now: the new exe moves in, starts, and this one quits.
//! For the UI: `check_now()` (the "Check for updates" button / menu item), `auto_update()` and
//! `set_auto_update(on)` (the "Update automatically" checkbox), `set_busy(…)` from status changes.

/// Where the new version waits: next to the running exe, as `<exe>.new`.
pub fn staging_for(exe: &std::path::Path) -> std::path::PathBuf {
    crate::install::new_path(exe)
}

#[cfg(windows)]
pub use win::{auto_update, check_now, set_auto_update, set_busy, start};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::{input, install, net, win};
    use context_capsule_core::update::{self, Outcome, Ready, Target, Updater, LATER, RESTART_NOW};
    use context_capsule_core::{AppLog, Paths};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, OnceLock};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    static UPDATER: OnceLock<Arc<Updater>> = OnceLock::new();
    /// Contextualizing or a voice note: no automatic check now.
    static BUSY: AtomicBool = AtomicBool::new(false);
    /// The tray window, to quit the app after "Restart now".
    static TRAY: AtomicUsize = AtomicUsize::new(0);
    /// One "ready" question at a time.
    static ASKING: AtomicBool = AtomicBool::new(false);

    const TITLE: &str = "Context Capsule";

    /// Can we write next to the exe? (Not in Program Files without admin.)
    fn writable(dir: &std::path::Path) -> bool {
        let probe = dir.join(format!(".cc-write-test-{}", std::process::id()));
        let ok = std::fs::write(&probe, b"").is_ok();
        let _ = std::fs::remove_file(&probe);
        ok
    }

    /// Make the updater and start the automatic checks. Call once, on the UI thread, with the tray window.
    pub fn start(tray: HWND, log: AppLog) {
        TRAY.store(tray.0 as usize, Ordering::Relaxed);
        let exe = std::env::current_exe().ok();
        let staging = exe.as_deref().filter(|e| e.parent().is_some_and(writable)).map(staging_for);
        let updater = Arc::new(Updater::new(
            env!("CARGO_PKG_VERSION"),
            Target::Windows,
            Paths::standard().config(),
            staging,
            Box::new(net::WinInet),
            log,
        ));
        updater.spawn(|| BUSY.load(Ordering::Relaxed), |outcome| tell(outcome, false));
        let _ = UPDATER.set(updater);
    }

    /// From status changes: contextualizing or listening to a voice note.
    pub fn set_busy(busy: bool) {
        BUSY.store(busy, Ordering::Relaxed);
    }

    /// "Update automatically" (config.json "auto_update", on by default).
    pub fn auto_update() -> bool {
        update::Settings::load(&Paths::standard().config()).auto
    }

    /// Turn "Update automatically" on or off. false: config.json could not be written (it is left as it was).
    pub fn set_auto_update(on: bool) -> bool {
        update::set_auto_update(&Paths::standard().config(), on).is_ok()
    }

    /// "Check for updates": check now on a thread of its own and always say what came of it.
    pub fn check_now() {
        let Some(u) = UPDATER.get().cloned() else { return };
        std::thread::spawn(move || tell(u.check(), true));
    }

    /// Tell the person: ready (Restart now / Later), or — only when they asked — up to date or failed.
    fn tell(outcome: Outcome, asked: bool) {
        match outcome {
            Outcome::Ready(r) => ask_restart(r),
            Outcome::Available { .. } => win::alert(None, TITLE, &outcome.message()),
            _ if asked => win::alert(None, TITLE, &outcome.message()),
            _ => {}
        }
    }

    fn ask_restart(r: Ready) {
        if ASKING.swap(true, Ordering::SeqCst) {
            return;
        }
        let yes = input::ask_choice(TITLE, &update::ready_message(&r.version), RESTART_NOW, LATER);
        ASKING.store(false, Ordering::SeqCst);
        if yes {
            restart(&r);
        }
    }

    /// Move the new exe in (the running one becomes `.old`), start it, and quit this one. The new one waits for
    /// this process to end, then deletes `.old`.
    fn restart(r: &Ready) {
        let log = AppLog::new(Paths::standard().log());
        let Ok(exe) = std::env::current_exe() else { return };
        if let Err(e) = install::replace_exe(&r.file, &exe) {
            log.write(&format!("update: cannot move the new version in: {e}"));
            win::alert(None, TITLE, "Couldn't update. Try again later.");
            return;
        }
        log.write(&format!("update: restarting into {}", r.version));
        let started = std::process::Command::new(&exe)
            .args(["--updated", &std::process::id().to_string()])
            .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
            .spawn();
        if let Err(e) = started {
            log.write(&format!("update: cannot start the new version: {e}"));
            win::alert(None, TITLE, "The update is ready. Quit and start Context Capsule again to finish.");
            return;
        }
        let tray = TRAY.load(Ordering::Relaxed);
        if tray != 0 {
            // SAFETY: posts WM_CLOSE to our own tray window; it quits the app cleanly.
            let _ = unsafe { PostMessageW(Some(HWND(tray as *mut core::ffi::c_void)), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_next_to_the_exe() {
        let exe = std::path::Path::new("/p/context-capsule.exe");
        assert_eq!(staging_for(exe), std::path::Path::new("/p/context-capsule.exe.new"));
    }
}
