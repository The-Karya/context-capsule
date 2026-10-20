// context-capsule — linux: updates — the core's signed feed and checked download over https, a check a minute
// after launch and daily (never while contextualizing or during a voice note). As an AppImage: the new
// AppImage is downloaded next to it, and "Restart now" puts it in place of $APPIMAGE and starts it. Built from
// source: only "A new version is out".
//! For a UI: `check_now()` ("Check for updates"), `auto_update()` / `set_auto_update(on)` ("Update
//! automatically"), `set_busy(…)` from status changes.

use crate::notify::Notifier;
use context_capsule_core::update::{self, Outcome, Ready, Target, Updater, LATER, RESTART_NOW};
use context_capsule_core::{AppLog, Msg, Paths};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

struct Shared {
    updater: Arc<Updater>,
    notifier: Notifier,
    quit: Mutex<Sender<Msg>>,
    log: AppLog,
}

static SHARED: OnceLock<Shared> = OnceLock::new();
/// Contextualizing or a voice note: no automatic check now.
static BUSY: AtomicBool = AtomicBool::new(false);
/// One "ready" question at a time.
static ASKING: AtomicBool = AtomicBool::new(false);
/// After "Restart now": the AppImage to start once this process has stopped.
static RELAUNCH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Where the new AppImage waits: `.<name>.new` in the same folder (so it can be renamed over the old one).
pub fn staging_for(image: &Path) -> Option<PathBuf> {
    let name = image.file_name()?.to_string_lossy().into_owned();
    Some(image.with_file_name(format!(".{name}.new")))
}

/// Can we replace this AppImage? (Its folder must be writable.)
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".cc-write-test-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Make the updater and start the automatic checks. `quit` is the engine's channel ("Restart now" quits cleanly).
pub fn start(paths: &Paths, notifier: Notifier, quit: Sender<Msg>) {
    let env = |k: &str| std::env::var(k).ok();
    let staging = crate::appimage::appimage(&env).filter(|i| i.parent().is_some_and(writable)).and_then(|i| staging_for(&i));
    if let Some(s) = &staging {
        let _ = std::fs::remove_file(s); // a staged update from an earlier run that was never used
    }
    let log = AppLog::new(paths.log());
    let updater = Arc::new(Updater::new(
        env!("CARGO_PKG_VERSION"),
        Target::Linux,
        paths.config(),
        staging,
        Box::new(crate::net::Https),
        log.clone(),
    ));
    updater.spawn(|| BUSY.load(Ordering::Relaxed), |outcome| tell(outcome, false));
    let _ = SHARED.set(Shared { updater, notifier, quit: Mutex::new(quit), log });
}

/// From status changes: contextualizing or listening to a voice note.
pub fn set_busy(busy: bool) {
    BUSY.store(busy, Ordering::Relaxed);
}

/// "Update automatically" (config.json "auto_update", on by default).
#[allow(dead_code)] // for a Settings window
pub fn auto_update() -> bool {
    update::Settings::load(&Paths::standard().config()).auto
}

/// Turn "Update automatically" on or off. false: config.json could not be written (it is left as it was).
#[allow(dead_code)] // for a Settings window
pub fn set_auto_update(on: bool) -> bool {
    update::set_auto_update(&Paths::standard().config(), on).is_ok()
}

/// "Check for updates": check now on a thread of its own and always say what came of it.
pub fn check_now() {
    let Some(s) = SHARED.get() else { return };
    let u = s.updater.clone();
    std::thread::spawn(move || tell(u.check(), true));
}

fn tell(outcome: Outcome, asked: bool) {
    let Some(s) = SHARED.get() else { return };
    match outcome {
        Outcome::Ready(r) => ask_restart(s, r),
        Outcome::Available { .. } => s.notifier.toast(&outcome.message()),
        _ if asked => s.notifier.toast(&outcome.message()),
        _ => {}
    }
}

fn ask_restart(s: &Shared, r: Ready) {
    if ASKING.swap(true, Ordering::SeqCst) {
        return;
    }
    let yes = s.notifier.ask("Context Capsule", &update::ready_message(&r.version), RESTART_NOW, LATER);
    ASKING.store(false, Ordering::SeqCst);
    if !yes {
        return;
    }
    let env = |k: &str| std::env::var(k).ok();
    let Some(image) = crate::appimage::appimage(&env) else { return };
    match replace(&r.file, &image) {
        Ok(()) => {
            s.log.write(&format!("update: restarting into {}", r.version));
            *RELAUNCH.lock().unwrap_or_else(|e| e.into_inner()) = Some(image);
            let _ = s.quit.lock().unwrap_or_else(|e| e.into_inner()).send(Msg::Quit);
        }
        Err(e) => {
            s.log.write(&format!("update: cannot replace the AppImage: {e}"));
            s.notifier.alert("Context Capsule", "Couldn't update. Try again later.");
        }
    }
}

/// Make the new file runnable and rename it over the AppImage (the running one keeps its old copy open).
pub fn replace(new: &Path, image: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755))?;
    std::fs::rename(new, image)
}

/// At the very end of main: after "Restart now", start the new AppImage.
pub fn relaunch_if_pending() {
    let image = RELAUNCH.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(image) = image {
        if let Err(e) = std::process::Command::new(&image).spawn() {
            eprintln!("Context Capsule: cannot start the new version: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_and_replaced() {
        assert_eq!(
            staging_for(Path::new("/home/s/Apps/Context-Capsule-0.1.0-x86_64.AppImage")),
            Some(PathBuf::from("/home/s/Apps/.Context-Capsule-0.1.0-x86_64.AppImage.new"))
        );
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("cc-linux-update-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let image = dir.join("cc.AppImage");
        std::fs::write(&image, b"old").unwrap();
        let new = staging_for(&image).unwrap();
        std::fs::write(&new, b"new").unwrap();
        assert!(writable(&dir));
        replace(&new, &image).unwrap();
        assert_eq!(std::fs::read(&image).unwrap(), b"new");
        assert_eq!(std::fs::metadata(&image).unwrap().permissions().mode() & 0o777, 0o755);
        assert!(!new.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
