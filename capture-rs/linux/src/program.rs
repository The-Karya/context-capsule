// context-capsule — linux: the any-AI program (SPEC "Any AI"). The AppImage carries it in usr/bin; but an
// AppImage is mounted at a new place every time it starts, and AI apps connected to Context Capsule need a path
// that stays. So, from an AppImage, it is copied (when missing or different) to
// `$XDG_DATA_HOME/context-capsule/bin/context-capsule-mcp` (else `~/.local/share/…`) and run from there.
// Built from source: the program next to `context-capsule`, else (developing) the repo's mcp/ build.
//! Pure file work, tested here. Copying takes a moment: it runs on a thread at startup, and every call that
//! needs the program waits for it.

use context_capsule_core::anyai::{self, Launch, PROGRAM};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where the copy from an AppImage lives (stays the same across updates).
pub fn stable_path(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    Some(crate::appimage::data_home(env)?.join("context-capsule").join("bin").join(PROGRAM))
}

/// The program inside the running AppImage (`$APPDIR/usr/bin`), when there is one.
pub fn bundled(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    crate::appimage::appimage(env)?;
    let dir = PathBuf::from(env("APPDIR")?.trim());
    let p = dir.join("usr/bin").join(PROGRAM);
    (dir.is_absolute() && p.is_file()).then_some(p)
}

/// Same bytes? (Sizes first, then the contents, stopping at the first difference.)
pub fn same_file(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else { return false };
    if ma.len() != mb.len() {
        return false;
    }
    let (Ok(mut fa), Ok(mut fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else { return false };
    let (mut ba, mut bb) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let n = match fa.read(&mut ba) {
            Ok(n) => n,
            Err(_) => return false,
        };
        if n == 0 {
            return true;
        }
        if fb.read_exact(&mut bb[..n]).is_err() || ba[..n] != bb[..n] {
            return false;
        }
    }
}

/// Copy `from` to `to` unless it's already the same: through a temporary file renamed over it (a running copy
/// keeps running), made runnable.
pub fn copy_if_changed(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if same_file(from, to) {
        return Ok(());
    }
    let dir = to.parent().ok_or_else(|| std::io::Error::other("no folder"))?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{PROGRAM}.new"));
    let result = std::fs::copy(from, &tmp)
        .and_then(|_| std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)))
        .and_then(|()| std::fs::rename(&tmp, to));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Done once per run: the program to run (or None).
static READY: Mutex<Option<Option<Launch>>> = Mutex::new(None);

/// How to run the program, found (and from an AppImage, copied to its stable place) once per run; other callers
/// wait for it. `log`: one line saying what was found.
pub fn launch(log: &dyn Fn(&str)) -> Option<Launch> {
    let mut ready = READY.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(done) = ready.as_ref() {
        return done.clone();
    }
    let env = |k: &str| std::env::var(k).ok();
    let exe = std::env::current_exe().unwrap_or_default();
    let mut places = Vec::new();
    if let (Some(from), Some(to)) = (bundled(&env), stable_path(&env)) {
        match copy_if_changed(&from, &to) {
            Ok(()) => places.push(to),
            Err(e) => {
                log(&format!("any AI: cannot copy the program to {}: {e}", to.display()));
                places.push(from); // works now; connected apps would lose it when the AppImage closes
            }
        }
    }
    if let Some(dir) = exe.parent() {
        places.push(dir.join(PROGRAM));
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let found = anyai::find(&places, &anyai::repo_candidates(&exe, &cwd), anyai::node_on_path().as_deref(), &|p: &Path| p.is_file());
    log(&format!("any AI: program {}", found.as_ref().map_or_else(|| "not found".to_string(), |l| l.to_string())));
    *ready = Some(found.clone());
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| pairs.iter().find(|(key, _)| *key == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn places() {
        assert_eq!(stable_path(&env(&[("HOME", "/home/s")])), Some(PathBuf::from("/home/s/.local/share/context-capsule/bin/context-capsule-mcp")));
        assert_eq!(stable_path(&env(&[("HOME", "/home/s"), ("XDG_DATA_HOME", "/d")])), Some(PathBuf::from("/d/context-capsule/bin/context-capsule-mcp")));
        assert_eq!(bundled(&env(&[("APPDIR", "/tmp/.mount_x")])), None, "not an AppImage");
    }

    #[test]
    fn copies_only_when_changed() {
        let dir = std::env::temp_dir().join(format!("cc-program-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("from");
        let to = dir.join("bin/context-capsule-mcp");
        let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 253) as u8).collect();
        std::fs::write(&from, &big).unwrap();
        assert!(!same_file(&from, &to));
        copy_if_changed(&from, &to).unwrap();
        assert!(same_file(&from, &to));
        assert_eq!(std::fs::metadata(&to).unwrap().permissions().mode() & 0o777, 0o755);
        let before = std::fs::metadata(&to).unwrap().modified().unwrap();
        copy_if_changed(&from, &to).unwrap();
        assert_eq!(std::fs::metadata(&to).unwrap().modified().unwrap(), before, "same: not copied again");
        let mut changed = big.clone();
        changed[2_500_000] ^= 1; // same size, one byte different
        std::fs::write(&from, &changed).unwrap();
        assert!(!same_file(&from, &to));
        copy_if_changed(&from, &to).unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), changed);
        assert!(!dir.join("bin/.context-capsule-mcp.new").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
