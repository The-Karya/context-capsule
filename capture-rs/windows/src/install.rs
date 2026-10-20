// context-capsule — windows: easy install without admin or an installer — the downloaded exe offers "Install
// Context Capsule?" and copies itself to %LOCALAPPDATA%\Programs\Context Capsule, with a Start menu shortcut and
// an entry in Settings → Apps (HKCU, so no admin). `--uninstall` takes it all away again (the user's recordings
// only if they say so). The Claude extension is inside the exe and written next to it.
//! "Just run it" runs the copy where it is; it asks again next time it is opened, but never at login
//! (`--login`). Also the file swap updates use: rename the running exe to `.old` (Windows allows that), move the new one in.

use std::path::{Path, PathBuf};

/// The Claude extension, inside the exe (empty when the build had no mcp/dist-ext/context-capsule.mcpb).
pub static EXTENSION: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/context-capsule.mcpb"));

pub const DIR_NAME: &str = "Context Capsule";
pub const EXE_NAME: &str = "context-capsule.exe";
pub const PUBLISHER: &str = "Karya";
pub const DISPLAY_NAME: &str = "Context Capsule";
pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\ContextCapsule";
/// In the open-at-login command: started at login, so don't ask to install (only when someone opens it).
pub const LOGIN_ARG: &str = "--login";

pub const INSTALL_QUESTION: &str = "Install Context Capsule?";
pub const INSTALL_BUTTON: &str = "Install";
pub const JUST_RUN_BUTTON: &str = "Just run it";
pub const INSTALL_FAILED: &str = "Couldn't install. Context Capsule runs from here for now.";
pub const DELETE_DATA_QUESTION: &str = "Also delete your recordings and capsules?";
pub const REMOVED: &str = "Context Capsule is removed.\n\nTo remove it from Claude too: Claude → Settings → Extensions.";

/// How this run was started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Launch {
    Normal,
    /// Open at login (never asks to install).
    Login,
    /// From Settings → Apps → Uninstall.
    Uninstall,
    /// Started by the old version after an update; wait for that process (this pid) to end first.
    Updated(u32),
}

impl Launch {
    pub fn parse(args: &[String]) -> Launch {
        let mut it = args.iter().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--uninstall" => return Launch::Uninstall,
                LOGIN_ARG => return Launch::Login,
                "--updated" => return Launch::Updated(it.next().and_then(|p| p.parse().ok()).unwrap_or(0)),
                _ => {}
            }
        }
        Launch::Normal
    }
}

/// `%LOCALAPPDATA%\Programs\Context Capsule`
pub fn install_dir(local_appdata: &Path) -> PathBuf {
    local_appdata.join("Programs").join(DIR_NAME)
}

/// `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Context Capsule.lnk`
pub fn shortcut_path(appdata: &Path) -> PathBuf {
    appdata.join("Microsoft").join("Windows").join("Start Menu").join("Programs").join(format!("{DISPLAY_NAME}.lnk"))
}

/// Is `exe` the installed copy (in `dir`, any letter case)?
pub fn is_installed(exe: &Path, dir: &Path) -> bool {
    let same = |a: &Path, b: &Path| {
        let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
        norm(a) == norm(b)
    };
    exe.parent().is_some_and(|p| same(p, dir))
}

/// `<exe>.old`: where the running exe goes during an update.
pub fn old_path(exe: &Path) -> PathBuf {
    with_suffix(exe, ".old")
}

/// `<exe>.new`: the checked new version, waiting for "Restart now".
pub fn new_path(exe: &Path) -> PathBuf {
    with_suffix(exe, ".new")
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut name = p.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(suffix);
    p.with_file_name(name)
}

/// Old copies left by updates: `<exe name>….old` next to the exe.
pub fn is_leftover(exe: &Path, candidate: &Path) -> bool {
    let (Some(exe_name), Some(name)) = (exe.file_name(), candidate.file_name()) else { return false };
    let (exe_name, name) = (exe_name.to_string_lossy().to_lowercase(), name.to_string_lossy().to_lowercase());
    name.len() > exe_name.len() && name.starts_with(&exe_name) && name.ends_with(".old")
}

/// Replace `exe` (it may be running) with `new`: the running file moves aside to `.old`, the new one moves in.
/// If the second step fails the old file goes back.
pub fn replace_exe(new: &Path, exe: &Path) -> std::io::Result<()> {
    let mut old = old_path(exe);
    if old.exists() && std::fs::remove_file(&old).is_err() {
        // An older copy is still running from there: use another name.
        old = with_suffix(exe, &format!(".{}.old", std::process::id()));
    }
    let had = exe.exists();
    if had {
        std::fs::rename(exe, &old)?;
    }
    if let Err(e) = std::fs::rename(new, exe) {
        if had {
            let _ = std::fs::rename(&old, exe);
        }
        return Err(e);
    }
    Ok(())
}

/// Delete old copies left by an update (they may still be closing: a few tries).
pub fn remove_leftovers(exe: &Path) {
    let Some(dir) = exe.parent() else { return };
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if is_leftover(exe, &p) {
            for _ in 0..10 {
                if std::fs::remove_file(&p).is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
    }
}

/// Write the embedded Claude extension into `dir` unless the same file is already there. false: nothing to
/// write (no extension inside this build) or Windows wouldn't.
pub fn write_extension(dir: &Path) -> bool {
    if EXTENSION.is_empty() {
        return false;
    }
    write_if_changed(&dir.join(crate::connect::EXTENSION_FILE), EXTENSION)
}

fn write_if_changed(path: &Path, data: &[u8]) -> bool {
    if std::fs::read(path).is_ok_and(|d| d == data) {
        return true;
    }
    let tmp = with_suffix(path, ".tmp");
    let ok = std::fs::write(&tmp, data).and_then(|()| std::fs::rename(&tmp, path)).is_ok();
    if !ok {
        let _ = std::fs::remove_file(&tmp);
    }
    ok
}

/// A value in the uninstall entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegValue {
    Text(String),
    Number(u32),
}

/// The Settings → Apps entry for the installed exe.
pub fn uninstall_values(exe: &Path, version: &str, size_bytes: u64) -> Vec<(&'static str, RegValue)> {
    let exe_s = exe.to_string_lossy();
    let dir = exe.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    vec![
        ("DisplayName", RegValue::Text(DISPLAY_NAME.into())),
        ("DisplayVersion", RegValue::Text(version.into())),
        ("Publisher", RegValue::Text(PUBLISHER.into())),
        ("DisplayIcon", RegValue::Text(format!("\"{exe_s}\",0"))),
        ("InstallLocation", RegValue::Text(dir)),
        ("UninstallString", RegValue::Text(format!("\"{exe_s}\" --uninstall"))),
        ("NoModify", RegValue::Number(1)),
        ("NoRepair", RegValue::Number(1)),
        ("EstimatedSize", RegValue::Number(size_bytes.div_ceil(1024).min(u32::MAX as u64) as u32)),
    ]
}

/// What "delete your recordings and capsules" removes: the whole data folder when it is ours by name, else only
/// what Context Capsule put in it (CONTEXT_CAPSULE_HOME may point at a folder with other things).
pub fn data_to_delete(root: &Path) -> Vec<PathBuf> {
    let name = root.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if name == ".context-capsule" || name == "context-capsule" {
        return vec![root.to_path_buf()];
    }
    ["sessions", "capsules", "models", "config.json", "state.json", "projects.json", "app.log", ".reminded", "control", ".login-set"]
        .iter()
        .map(|n| root.join(n))
        .collect()
}

#[cfg(windows)]
pub use win::startup;

#[cfg(windows)]
mod win {
    use super::*;
    use crate::setup::{APP_KEY, RUN_KEY, RUN_VALUE, STARTUP_APPROVED_KEY};
    use crate::{input, win};
    use context_capsule_core::{AppLog, Paths};
    use std::time::Duration;
    use windows::core::{w, Interface, HSTRING};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, WPARAM};
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER};
    use windows::Win32::System::Registry::{RegDeleteTreeW, HKEY_CURRENT_USER};
    use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetWindowThreadProcessId, MessageBoxW, PostMessageW, IDYES, MB_DEFBUTTON2, MB_ICONQUESTION, MB_SETFOREGROUND,
        MB_TOPMOST, MB_YESNO, WM_CLOSE,
    };

    fn env_dir(name: &str) -> Option<PathBuf> {
        std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
    }

    fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    /// Before the app starts: uninstall, wait for the old version after an update, offer to install, write the
    /// extension. false: this process is done (installed and started the installed copy, or uninstalled).
    pub fn startup() -> bool {
        let log = AppLog::new(Paths::standard().log());
        let args: Vec<String> = std::env::args().collect();
        let launch = Launch::parse(&args);
        match launch {
            Launch::Uninstall => {
                uninstall(&log);
                return false;
            }
            Launch::Updated(pid) => {
                wait_for_exit(pid, Duration::from_secs(60));
                log.write(&format!("update: now running {}", version()));
            }
            Launch::Normal | Launch::Login => {}
        }
        let Ok(exe) = std::env::current_exe() else { return true };
        remove_leftovers(&exe);
        let _ = std::fs::remove_file(new_path(&exe)); // a staged update that was never used
        let dir = env_dir("LOCALAPPDATA").map(|l| install_dir(&l));
        let installed = dir.as_deref().is_some_and(|d| is_installed(&exe, d));
        if let (Some(dir), false, Launch::Normal) = (&dir, installed, &launch) {
            if offer(&exe, dir, &log) {
                return false;
            }
        }
        if let Some(exe_dir) = exe.parent() {
            write_extension(exe_dir);
        }
        if installed {
            register_uninstall(&exe); // keeps DisplayVersion right after an update
        }
        true
    }

    /// "Install Context Capsule?" Install / Just run it. true: installed and the installed copy started.
    /// Asked each time someone opens a copy that isn't installed (never at login).
    fn offer(exe: &Path, dir: &Path, log: &AppLog) -> bool {
        if !input::ask_choice(DISPLAY_NAME, INSTALL_QUESTION, INSTALL_BUTTON, JUST_RUN_BUTTON) {
            log.write("install: just run it");
            return false;
        }
        match install(exe, dir, log) {
            Ok(target) => {
                log.write("install: done");
                match std::process::Command::new(&target).current_dir(dir).spawn() {
                    Ok(_) => true,
                    Err(e) => {
                        log.write(&format!("install: cannot start the installed copy: {e}"));
                        false
                    }
                }
            }
            Err(e) => {
                log.write(&format!("install: failed: {e}"));
                win::alert(None, DISPLAY_NAME, INSTALL_FAILED);
                false
            }
        }
    }

    fn install(exe: &Path, dir: &Path, log: &AppLog) -> Result<PathBuf, String> {
        close_running(log);
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let target = dir.join(EXE_NAME);
        let staged = new_path(&target);
        std::fs::copy(exe, &staged).map_err(|e| format!("copy: {e}"))?;
        replace_exe(&staged, &target).map_err(|e| {
            let _ = std::fs::remove_file(&staged);
            format!("move: {e}")
        })?;
        write_extension(dir);
        if let Some(appdata) = env_dir("APPDATA") {
            if let Err(e) = create_shortcut(&target, &shortcut_path(&appdata)) {
                log.write(&format!("install: no Start menu shortcut: {e}"));
            }
        }
        register_uninstall(&target);
        // Open at login keeps working, now with the installed copy.
        if win::reg_string(RUN_KEY, RUN_VALUE).is_some() {
            let _ = win::set_reg_string(RUN_KEY, RUN_VALUE, &crate::setup::run_command(&target.to_string_lossy()));
        }
        Ok(target)
    }

    fn register_uninstall(exe: &Path) {
        let size = std::fs::metadata(exe).map(|m| m.len()).unwrap_or(0) + EXTENSION.len() as u64;
        for (name, value) in uninstall_values(exe, version(), size) {
            let _ = match value {
                RegValue::Text(t) => win::set_reg_string(UNINSTALL_KEY, name, &t),
                RegValue::Number(n) => win::set_reg_dword(UNINSTALL_KEY, name, n),
            };
        }
    }

    fn create_shortcut(target: &Path, lnk: &Path) -> windows::core::Result<()> {
        win::init_com(true);
        if let Some(d) = lnk.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        // SAFETY: a ShellLink COM object used and released here, with our own strings.
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(&HSTRING::from(target.as_os_str()))?;
            if let Some(d) = target.parent() {
                link.SetWorkingDirectory(&HSTRING::from(d.as_os_str()))?;
            }
            link.SetDescription(w!("Turn your work into context for Claude"))?;
            link.cast::<IPersistFile>()?.Save(&HSTRING::from(lnk.as_os_str()), true)
        }
    }

    /// Ask a running Context Capsule to quit (it ends a recording cleanly) and wait for it to be gone.
    pub fn close_running(log: &AppLog) {
        // SAFETY: looks up a window class by name.
        let Ok(hwnd) = (unsafe { FindWindowW(w!("ContextCapsuleTray"), None) }) else { return };
        if hwnd.is_invalid() {
            return;
        }
        let mut pid = 0u32;
        // SAFETY: `pid` is a u32 we own.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 || pid == std::process::id() {
            return;
        }
        log.write("install: asking the running copy to quit");
        // SAFETY: posts a message to that window.
        let _ = unsafe { PostMessageW(Some(HWND(hwnd.0)), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        wait_for_exit(pid, Duration::from_secs(15));
    }

    /// Wait until process `pid` has ended (at most `limit`).
    pub fn wait_for_exit(pid: u32, limit: Duration) {
        if pid == 0 {
            return;
        }
        // SAFETY: a handle we open and close here, only to wait on.
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
                let _ = WaitForSingleObject(h, limit.as_millis().min(u32::MAX as u128) as u32);
                let _ = CloseHandle(h);
            }
        }
    }

    fn ask_delete_data() -> bool {
        // SAFETY: modal dialog with our own strings. No is the default button.
        let answer = unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(DELETE_DATA_QUESTION),
                &HSTRING::from(DISPLAY_NAME),
                MB_YESNO | MB_DEFBUTTON2 | MB_ICONQUESTION | MB_SETFOREGROUND | MB_TOPMOST,
            )
        };
        answer == IDYES
    }

    /// Remove the shortcut, the Settings → Apps entry, open at login and the program folder. The user's data
    /// only if they say so (No is the default).
    fn uninstall(log: &AppLog) {
        log.write("uninstall: start");
        close_running(log);
        let delete_data = ask_delete_data();
        if let Some(appdata) = env_dir("APPDATA") {
            let _ = std::fs::remove_file(shortcut_path(&appdata));
        }
        win::delete_reg_value(RUN_KEY, RUN_VALUE);
        win::delete_reg_value(STARTUP_APPROVED_KEY, RUN_VALUE);
        // SAFETY: deletes our own keys under HKCU (missing is fine).
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(UNINSTALL_KEY));
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(APP_KEY));
        }
        if delete_data {
            log.write("uninstall: deleting recordings and capsules");
            for p in data_to_delete(&Paths::standard().root) {
                let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
            }
        } else {
            log.write("uninstall: done (recordings and capsules kept)");
        }
        if let Some(dir) = env_dir("LOCALAPPDATA").map(|l| install_dir(&l)).filter(|d| d.is_dir()) {
            remove_folder_after_exit(&dir);
        }
        win::alert(None, DISPLAY_NAME, REMOVED);
    }

    /// A running exe can't delete its own folder: a hidden cmd does it two seconds after we exit.
    fn remove_folder_after_exit(dir: &Path) {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let d = dir.to_string_lossy().replace('"', "");
        let _ = std::process::Command::new("cmd.exe")
            .raw_arg(format!("/C \"ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{d}\"\""))
            .current_dir(std::env::temp_dir())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_args() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(Launch::parse(&a(&["cc.exe"])), Launch::Normal);
        assert_eq!(Launch::parse(&a(&["cc.exe", "--uninstall"])), Launch::Uninstall);
        assert_eq!(Launch::parse(&a(&["cc.exe", "--login"])), Launch::Login);
        assert_eq!(Launch::parse(&a(&["cc.exe", "--updated", "4242"])), Launch::Updated(4242));
        assert_eq!(Launch::parse(&a(&["cc.exe", "--updated"])), Launch::Updated(0));
    }

    #[test]
    fn places() {
        let local = Path::new(r"C:\Users\s\AppData\Local");
        let dir = install_dir(local);
        assert!(dir.ends_with("Programs/Context Capsule") || dir.to_string_lossy().ends_with(r"Programs\Context Capsule"));
        assert!(is_installed(&dir.join(EXE_NAME), &dir));
        let upper = PathBuf::from(dir.to_string_lossy().to_uppercase()).join(EXE_NAME);
        assert!(is_installed(&upper, &dir), "letter case does not matter");
        assert!(!is_installed(Path::new(r"C:\Users\s\Downloads\Context-Capsule-0.1.0-windows.exe"), &dir));
        let lnk = shortcut_path(Path::new(r"C:\Users\s\AppData\Roaming"));
        assert!(lnk.to_string_lossy().ends_with("Context Capsule.lnk"));
        assert!(lnk.to_string_lossy().contains("Start Menu"));
    }

    #[test]
    fn swap_and_leftovers() {
        let dir = std::env::temp_dir().join(format!("cc-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join(EXE_NAME);
        std::fs::write(&exe, b"old").unwrap();
        std::fs::write(new_path(&exe), b"new").unwrap();
        replace_exe(&new_path(&exe), &exe).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(old_path(&exe)).unwrap(), b"old");
        assert!(!new_path(&exe).exists());
        assert!(is_leftover(&exe, &old_path(&exe)));
        assert!(is_leftover(&exe, &dir.join("context-capsule.exe.123.old")));
        assert!(!is_leftover(&exe, &exe));
        assert!(!is_leftover(&exe, &dir.join("other.exe.old")));
        remove_leftovers(&exe);
        assert!(!old_path(&exe).exists() && exe.exists());
        // A missing new file leaves the exe where it was.
        assert!(replace_exe(&dir.join("missing"), &exe).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        // The extension is written once, and again only when it changed.
        assert!(write_if_changed(&dir.join("x.mcpb"), b"one"));
        assert!(write_if_changed(&dir.join("x.mcpb"), b"two"));
        assert_eq!(std::fs::read(dir.join("x.mcpb")).unwrap(), b"two");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn uninstall_entry() {
        let exe = Path::new(r"C:\Users\s\AppData\Local\Programs\Context Capsule\context-capsule.exe");
        let v = uninstall_values(exe, "0.2.0", 2048 + 1);
        let get = |k: &str| v.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(get("DisplayName"), RegValue::Text("Context Capsule".into()));
        assert_eq!(get("DisplayVersion"), RegValue::Text("0.2.0".into()));
        assert_eq!(get("Publisher"), RegValue::Text("Karya".into()));
        let RegValue::Text(u) = get("UninstallString") else { panic!() };
        assert!(u.starts_with('"') && u.ends_with("\" --uninstall"));
        assert_eq!(get("EstimatedSize"), RegValue::Number(3), "kilobytes, rounded up");
    }

    #[test]
    fn data_deletion_is_careful() {
        let ours = Path::new("/home/s").join(".context-capsule");
        assert_eq!(data_to_delete(&ours), vec![ours.clone()]);
        let custom = data_to_delete(Path::new("/home/s/Documents"));
        assert!(custom.iter().all(|p| p.starts_with("/home/s/Documents") && p != Path::new("/home/s/Documents")));
        assert!(custom.contains(&PathBuf::from("/home/s/Documents/sessions")));
    }
}
