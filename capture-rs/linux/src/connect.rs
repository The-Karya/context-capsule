// context-capsule — linux: Claude's row in "Connect your AI" — is Context Capsule connected to Claude (Claude
// Code, or a Claude app for Linux), is Claude here at all, and how to connect it: open context-capsule.mcpb
// (shipped next to the program) with whatever takes it, else the one line to run in Claude Code (with the
// context-capsule-mcp program when there is one, else node and the MCP server's script).
//! Connected = `~/.claude.json` or `~/.config/Claude/claude_desktop_config.json` has an "mcpServers" object with
//! a "context-capsule" entry (at any depth: Claude Code keeps per-folder servers under "projects"), or
//! `~/.config/Claude/Claude Extensions` has a context-capsule entry.
#![cfg_attr(not(feature = "window"), allow(dead_code))]

use std::path::{Path, PathBuf};

/// The extension file, next to the program (built by mcp/extension/build.sh).
pub const EXTENSION_FILE: &str = "context-capsule.mcpb";
pub const SERVER_NAME: &str = "context-capsule";
/// Shown when the MCP server can't be found next to the program.
pub const ENTRY_PLACEHOLDER: &str = "/path/to/context-capsule/mcp/dist/index.js";

pub const CONNECT_TITLE: &str = "Connect Claude";
pub const CONNECT_BODY: &str = "Claude has no app for Linux yet. In Claude Code, run this once in a terminal:";
pub const CONNECT_AFTER: &str = "Then start a new Claude Code chat. Needs Node 18 or newer.";
/// After the command that runs the program (no Node needed).
pub const CONNECT_AFTER_PROGRAM: &str = "Then start a new Claude Code chat.";
/// Disconnect, for Claude Code (Linux's Claude).
pub const REMOVE_IN_CLAUDE_CODE: &str = "Using Claude Code? Run this in a terminal: claude mcp remove --scope user context-capsule";
pub const COPY: &str = "Copy";
pub const CLOSE: &str = "Close";
pub const COPIED: &str = "Copied.";

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

/// Where Claude keeps its settings: (JSON files that may list servers, the extensions folder).
pub fn claude_places(env: &dyn Fn(&str) -> Option<String>) -> (Vec<PathBuf>, Option<PathBuf>) {
    let home = env("HOME").filter(|h| h.starts_with('/')).map(PathBuf::from);
    let config = env("XDG_CONFIG_HOME").filter(|x| x.starts_with('/')).map(PathBuf::from).or_else(|| home.as_ref().map(|h| h.join(".config")));
    let mut files = vec![];
    if let Some(h) = &home {
        files.push(h.join(".claude.json"));
    }
    let claude = config.map(|c| c.join("Claude"));
    if let Some(c) = &claude {
        files.push(c.join("claude_desktop_config.json"));
    }
    (files, claude.map(|c| c.join("Claude Extensions")))
}

/// Is Context Capsule connected, from these files and folder?
pub fn is_connected_in(files: &[PathBuf], extensions: Option<&Path>) -> bool {
    let listed = extensions
        .and_then(|d| std::fs::read_dir(d).ok())
        .is_some_and(|entries| entries.flatten().any(|e| e.file_name().to_string_lossy().to_lowercase().contains(SERVER_NAME)));
    listed
        || files.iter().any(|f| {
            std::fs::read(f).ok().and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok()).is_some_and(|v| contains_server(&v))
        })
}

pub fn is_connected(env: &dyn Fn(&str) -> Option<String>) -> bool {
    let (files, extensions) = claude_places(env);
    is_connected_in(&files, extensions.as_deref())
}

/// Is Claude on this computer: a Claude app's folder (`~/.config/Claude`), Claude Code's settings
/// (`~/.claude.json`, `~/.claude`), or `claude` on the PATH?
pub fn claude_present(env: &dyn Fn(&str) -> Option<String>) -> bool {
    let (files, extensions) = claude_places(env);
    let mut places: Vec<PathBuf> = files;
    if let Some(app) = extensions.as_deref().and_then(Path::parent) {
        places.push(app.to_path_buf());
    }
    if let Some(h) = env("HOME").filter(|h| h.starts_with('/')) {
        places.push(PathBuf::from(h).join(".claude"));
    }
    let on_path = env("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("claude").is_file()));
    on_path || places.iter().any(|p| p.exists())
}

/// The MCP server (`mcp/dist/index.js`): next to the program, in `../share/context-capsule`, or in a folder
/// above it (a build from the source tree: `capture-rs/target/release/` → the repo's `mcp/`).
pub fn mcp_entry(exe: &Path, exists: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let mut candidates = vec![dir.join("mcp/dist/index.js"), dir.join("../share/context-capsule/mcp/dist/index.js")];
    candidates.extend(dir.ancestors().skip(1).take(5).map(|a| a.join("mcp/dist/index.js")));
    candidates.into_iter().find(|c| exists(c))
}

/// Is this app (its desktop id and program) a Claude app? Only then is the extension handed to it (an archive
/// manager would happily open the .mcpb, which is a zip).
pub fn is_claude_app(id_and_program: &str) -> bool {
    id_and_program.to_lowercase().contains("claude")
}

/// `claude mcp add --scope user context-capsule -- node '/opt/cc/mcp/dist/index.js'`
pub fn claude_code_command(entry: Option<&Path>) -> String {
    let path = entry.map_or_else(|| ENTRY_PLACEHOLDER.to_string(), |p| p.to_string_lossy().into_owned());
    format!("claude mcp add --scope user {SERVER_NAME} -- node {}", shell_quote(&path))
}

/// The command for Claude Code: the context-capsule-mcp program when it was found
/// (`claude mcp add --scope user context-capsule -- '/home/s/.local/share/context-capsule/bin/context-capsule-mcp' serve`),
/// else node with the server's script (`entry`). Also: whether it needs Node.
pub fn claude_code_command_for(launch: Option<&context_capsule_core::anyai::Launch>, entry: Option<&Path>) -> (String, bool) {
    use context_capsule_core::anyai::Launch;
    match launch {
        Some(Launch::Binary(p)) => (format!("claude mcp add --scope user {SERVER_NAME} -- {} serve", shell_quote(&p.to_string_lossy())), false),
        Some(Launch::Node { script, .. }) => (claude_code_command(Some(script)), true),
        None => (claude_code_command(entry), true),
    }
}

/// Quote for sh only when needed.
pub fn shell_quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+:@%".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn finds_the_server_anywhere() {
        assert!(contains_server(&json!({"mcpServers": {"context-capsule": {"command": "node"}}})));
        assert!(contains_server(&json!({"projects": {"/home/s/x": {"mcpServers": {"context-capsule": {}}}}})));
        assert!(!contains_server(&json!({"mcpServers": {"other": {}}})));
        assert!(!contains_server(&json!({"context-capsule": {}})), "only inside mcpServers");
        assert!(!contains_server(&json!([1, "a", null])));
    }

    #[test]
    fn places_and_files() {
        let env = |k: &str| match k {
            "HOME" => Some("/home/s".to_string()),
            _ => None,
        };
        let (files, ext) = claude_places(&env);
        assert_eq!(files, [PathBuf::from("/home/s/.claude.json"), PathBuf::from("/home/s/.config/Claude/claude_desktop_config.json")]);
        assert_eq!(ext, Some(PathBuf::from("/home/s/.config/Claude/Claude Extensions")));
        let xdg = |k: &str| match k {
            "HOME" => Some("/home/s".to_string()),
            "XDG_CONFIG_HOME" => Some("/cfg".to_string()),
            _ => None,
        };
        assert_eq!(claude_places(&xdg).0[1], PathBuf::from("/cfg/Claude/claude_desktop_config.json"));

        let tmp = std::env::temp_dir().join(format!("cc-connect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("ext")).unwrap();
        let f = tmp.join(".claude.json");
        assert!(!is_connected_in(std::slice::from_ref(&f), Some(&tmp.join("ext"))), "nothing yet");
        std::fs::write(&f, r#"{"projects": {"/x": {"mcpServers": {"context-capsule": {"command": "node"}}}}}"#).unwrap();
        assert!(is_connected_in(std::slice::from_ref(&f), None));
        std::fs::write(&f, "not json").unwrap();
        assert!(!is_connected_in(std::slice::from_ref(&f), None));
        std::fs::create_dir_all(tmp.join("ext/local.mcpb.context-capsule")).unwrap();
        assert!(is_connected_in(&[f], Some(&tmp.join("ext"))));

        let home = tmp.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let h = home.to_string_lossy().into_owned();
        let at_home = |k: &str| (k == "HOME").then(|| h.clone());
        assert!(!claude_present(&at_home), "no Claude yet");
        std::fs::create_dir_all(home.join(".config/Claude")).unwrap();
        assert!(claude_present(&at_home), "the Claude app's folder");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn where_the_server_is() {
        let exe = Path::new("/repo/capture-rs/target/release/context-capsule");
        let have = |want: &'static str| move |p: &Path| p == Path::new(want);
        assert_eq!(mcp_entry(exe, &have("/repo/mcp/dist/index.js")), Some(PathBuf::from("/repo/mcp/dist/index.js")));
        assert_eq!(
            mcp_entry(exe, &have("/repo/capture-rs/target/release/mcp/dist/index.js")),
            Some(PathBuf::from("/repo/capture-rs/target/release/mcp/dist/index.js")),
            "next to the program first"
        );
        assert_eq!(mcp_entry(exe, &|_| false), None);
        assert_eq!(extension_next_to(Path::new("/opt/cc/context-capsule")), Path::new("/opt/cc/context-capsule.mcpb"));
    }

    #[test]
    fn the_command() {
        assert_eq!(
            claude_code_command(Some(Path::new("/opt/cc/mcp/dist/index.js"))),
            "claude mcp add --scope user context-capsule -- node /opt/cc/mcp/dist/index.js"
        );
        assert_eq!(
            claude_code_command(Some(Path::new("/home/s/My Apps/mcp/dist/index.js"))),
            "claude mcp add --scope user context-capsule -- node '/home/s/My Apps/mcp/dist/index.js'"
        );
        assert!(claude_code_command(None).ends_with(ENTRY_PLACEHOLDER));
        use context_capsule_core::anyai::Launch;
        let bin = Launch::Binary(PathBuf::from("/home/s/.local/share/context-capsule/bin/context-capsule-mcp"));
        assert_eq!(
            claude_code_command_for(Some(&bin), None),
            ("claude mcp add --scope user context-capsule -- /home/s/.local/share/context-capsule/bin/context-capsule-mcp serve".into(), false)
        );
        let spaced = Launch::Binary(PathBuf::from("/home/s/My Apps/context-capsule-mcp"));
        assert!(claude_code_command_for(Some(&spaced), None).0.ends_with("-- '/home/s/My Apps/context-capsule-mcp' serve"));
        let node = Launch::Node { node: "/usr/bin/node".into(), script: "/r/mcp/dist/index.js".into() };
        assert_eq!(claude_code_command_for(Some(&node), None), ("claude mcp add --scope user context-capsule -- node /r/mcp/dist/index.js".into(), true));
        assert_eq!(claude_code_command_for(None, None).0, claude_code_command(None));
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert!(is_claude_app("com.anthropic.Claude.desktop /usr/bin/claude-desktop"));
        assert!(!is_claude_app("org.gnome.FileRoller.desktop file-roller"));
    }
}
