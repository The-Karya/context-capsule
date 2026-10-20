// context-capsule — windows: "Connect Claude" — opens the Context Capsule extension (context-capsule.mcpb,
// shipped next to the exe) with the Claude app, which asks to install it, and tells whether it's connected.
//! Connected = the extension is in Claude's "Claude Extensions" folder, or claude_desktop_config.json
//! has a "context-capsule" server. Claude Code doesn't count here: this is about the Claude app.

use std::path::{Path, PathBuf};

/// The extension file, next to context-capsule.exe (built by mcp/extension/build.sh).
pub const EXTENSION_FILE: &str = "context-capsule.mcpb";
pub const SERVER_NAME: &str = "context-capsule";
pub const DOWNLOAD_URL: &str = "https://claude.ai/download";

pub fn extension_next_to(exe: &Path) -> PathBuf {
    exe.parent().unwrap_or(Path::new(".")).join(EXTENSION_FILE)
}

/// Any "mcpServers" object (at any depth) with a "context-capsule" entry.
pub fn contains_server(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.get("mcpServers").and_then(|s| s.as_object()).is_some_and(|s| s.contains_key(SERVER_NAME))
                || map.values().any(contains_server)
        }
        serde_json::Value::Array(list) => list.iter().any(contains_server),
        _ => false,
    }
}

/// Claude's data folders: `%APPDATA%\Claude`, and the Microsoft Store app's private copy under
/// `%LOCALAPPDATA%\Packages\<Claude…>\LocalCache\Roaming\Claude`.
pub fn claude_data_dirs(appdata: Option<&Path>, local_appdata: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = appdata.map(|a| a.join("Claude")).into_iter().collect();
    if let Some(Ok(entries)) = local_appdata.map(|l| std::fs::read_dir(l.join("Packages"))) {
        for e in entries.flatten() {
            if e.file_name().to_string_lossy().to_lowercase().contains("claude") {
                dirs.push(e.path().join("LocalCache").join("Roaming").join("Claude"));
            }
        }
    }
    dirs
}

/// Is Context Capsule connected in any of Claude's data folders?
pub fn is_connected_in(dirs: &[PathBuf]) -> bool {
    dirs.iter().any(|dir| {
        let listed = std::fs::read_dir(dir.join("Claude Extensions"))
            .map(|entries| entries.flatten().any(|e| e.file_name().to_string_lossy().to_lowercase().contains(SERVER_NAME)))
            .unwrap_or(false);
        listed
            || std::fs::read(dir.join("claude_desktop_config.json"))
                .ok()
                .and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok())
                .is_some_and(|v| contains_server(&v))
    })
}

/// Is this program (a path, or an app's friendly name) the Claude app?
pub fn is_claude_app(name: &str) -> bool {
    let file = name.rsplit(['\\', '/']).next().unwrap_or(name).to_lowercase();
    file.contains("claude")
}

#[cfg(windows)]
pub use win::{connect, is_connected};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::win;
    use context_capsule_core::AppLog;
    use windows::core::{w, PWSTR};
    use windows::Win32::UI::Shell::{AssocQueryStringW, ASSOCF_NONE, ASSOCSTR, ASSOCSTR_EXECUTABLE, ASSOCSTR_FRIENDLYAPPNAME};

    fn env_dir(name: &str) -> Option<PathBuf> {
        std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
    }

    pub fn is_connected() -> bool {
        is_connected_in(&claude_data_dirs(env_dir("APPDATA").as_deref(), env_dir("LOCALAPPDATA").as_deref()))
    }

    fn assoc(what: ASSOCSTR) -> Option<String> {
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` holds `len` chars; the answer is NUL-terminated within it.
        unsafe { AssocQueryStringW(ASSOCF_NONE, what, w!(".mcpb"), None, Some(PWSTR(buf.as_mut_ptr())), &mut len) }.ok().ok()?;
        let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }

    /// How to hand the extension to Claude: its file type (Claude registers .mcpb), or its exe.
    enum Opener {
        FileType,
        Exe(PathBuf),
    }

    fn claude() -> Option<Opener> {
        if [ASSOCSTR_EXECUTABLE, ASSOCSTR_FRIENDLYAPPNAME].into_iter().filter_map(assoc).any(|a| is_claude_app(&a)) {
            return Some(Opener::FileType);
        }
        let exe = env_dir("LOCALAPPDATA")?.join("AnthropicClaude").join("claude.exe");
        exe.is_file().then_some(Opener::Exe(exe))
    }

    /// Open the extension in the Claude app (it asks the user to install). No Claude app: offer the download.
    pub fn connect(log: &AppLog) {
        let Some(opener) = claude() else {
            log.write("connect: the Claude app is not installed");
            let body = "Context Capsule works with the Claude desktop app. Download it, sign in, then click Connect again.\n\n\
                        Open the download page now?";
            if win::ask_yes_no("Install the Claude app first", body) {
                win::open(DOWNLOAD_URL);
            }
            return;
        };
        let file = std::env::current_exe().map(|e| extension_next_to(&e)).unwrap_or_else(|_| PathBuf::from(EXTENSION_FILE));
        if !file.is_file() {
            log.write("connect: the extension file is missing next to the exe");
            win::alert(
                None,
                "Something is missing",
                &format!("{EXTENSION_FILE} should be in the same folder as context-capsule.exe. Please download Context Capsule again."),
            );
            return;
        }
        log.write("connect: opening the extension in Claude");
        let path = file.to_string_lossy();
        let opened = match opener {
            Opener::FileType => win::open(&path),
            Opener::Exe(exe) => win::open_with(&exe.to_string_lossy(), Some(&format!("\"{path}\""))),
        };
        if !opened {
            log.write("connect: Windows could not open the extension");
            win::alert(None, "Couldn't open Claude", "Open the Claude app, then double-click context-capsule.mcpb in the Context Capsule folder.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_anywhere_in_the_config() {
        let v = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap();
        assert!(contains_server(&v(r#"{"mcpServers":{"context-capsule":{"command":"node"}}}"#)));
        assert!(contains_server(&v(r#"{"profiles":[{"mcpServers":{"context-capsule":{}}}]}"#)), "nested");
        assert!(!contains_server(&v(r#"{"mcpServers":{"other":{}}}"#)));
        assert!(!contains_server(&v(r#"{"context-capsule":{}}"#)), "only inside mcpServers");
        assert!(!contains_server(&v("[]")));
    }

    #[test]
    fn connected_from_extensions_or_config() {
        let root = std::env::temp_dir().join(format!("cc-connect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (appdata, local) = (root.join("Roaming"), root.join("Local"));
        std::fs::create_dir_all(appdata.join("Claude").join("Claude Extensions").join("other-ext")).unwrap();
        let store = local.join("Packages").join("Claude_pzs8sxrjxfjjc");
        std::fs::create_dir_all(store.join("LocalCache").join("Roaming").join("Claude")).unwrap();
        std::fs::create_dir_all(local.join("Packages").join("Microsoft.Paint_8wekyb3d8bbwe")).unwrap();

        let dirs = claude_data_dirs(Some(&appdata), Some(&local));
        assert_eq!(dirs.len(), 2, "%APPDATA%\\Claude and the Store app's copy: {dirs:?}");
        assert!(!is_connected_in(&dirs));

        std::fs::create_dir_all(appdata.join("Claude").join("Claude Extensions").join("local.mcpb.context-capsule")).unwrap();
        assert!(is_connected_in(&dirs), "the extension is installed");

        std::fs::remove_dir_all(appdata.join("Claude")).unwrap();
        let config = store.join("LocalCache").join("Roaming").join("Claude").join("claude_desktop_config.json");
        std::fs::write(&config, r#"{"mcpServers":{"context-capsule":{"command":"node"}}}"#).unwrap();
        assert!(is_connected_in(&claude_data_dirs(Some(&appdata), Some(&local))), "a config entry in the Store app's folder");
        std::fs::write(&config, "not json").unwrap();
        assert!(!is_connected_in(&claude_data_dirs(Some(&appdata), Some(&local))));
        assert!(claude_data_dirs(None, None).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn claude_app_names() {
        assert!(is_claude_app(r"C:\Users\s\AppData\Local\AnthropicClaude\app-0.14.10\claude.exe"));
        assert!(is_claude_app("Claude"));
        assert!(!is_claude_app(r"C:\Windows\System32\notepad.exe"));
        assert_eq!(extension_next_to(Path::new("/kit/context-capsule.exe")), Path::new("/kit/context-capsule.mcpb"));
    }
}
