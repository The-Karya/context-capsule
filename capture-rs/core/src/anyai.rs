// context-capsule — core: any AI (SPEC "Any AI"). Runs the context-capsule-mcp program (clients, connect,
// disconnect, day, save) with a time limit and reads its JSON; which AI is "Your AI" (config.json "ai"); and the
// plain words the Windows and Linux recorders' own dialogs and messages say about it. A port of AIBridge.swift and
// Core.swift's AIClient, AIAnswer, YourAI and AIProgram.
//! Nothing here draws or touches a clipboard: the platforms do that. Every call blocks, so call it off the UI thread.

use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The program's name (Windows adds `.exe`).
pub const PROGRAM: &str = "context-capsule-mcp";

// MARK: the program's answers

/// One AI app from `context-capsule-mcp clients`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiClient {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub connected: bool,
    /// "extension" (Claude: the .mcpb) or "config" (written into the app's MCP settings).
    pub how: String,
}

impl AiClient {
    /// Claude, as the app knew it before the program (when the program can't be run).
    pub fn claude(installed: bool, connected: bool) -> AiClient {
        AiClient { id: CLAUDE_ID.into(), name: "Claude".into(), installed, connected, how: "extension".into() }
    }

    /// Claude's own flow (the .mcpb extension), not `connect`.
    pub fn is_extension(&self) -> bool {
        self.how == "extension" || self.id == CLAUDE_ID
    }
}

/// `connect` / `disconnect`: `{"ok","message"}` (and `"how":"extension"` for Claude).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectAnswer {
    pub ok: bool,
    pub message: String,
    pub how: Option<String>,
}

/// `day`: the paste-ready message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayAnswer {
    pub text: String,
    pub chars: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub title: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub title: String,
    pub why: String,
}

/// `save`: what was saved and what wasn't (and why).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveAnswer {
    pub saved: Vec<Saved>,
    pub refused: Vec<Refused>,
}

/// Why a call gave nothing usable, said plainly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The program isn't there (or can't be started).
    Missing,
    Timeout,
    /// It said why (its error, or the last line it wrote on stderr).
    Failed(String),
    /// Its answer can't be read.
    BadAnswer,
}

impl Failure {
    pub fn message(&self) -> String {
        match self {
            Failure::Missing => "Part of Context Capsule is missing. Please download Context Capsule again.".into(),
            Failure::Timeout => "That took too long. Please try again.".into(),
            Failure::Failed(why) if !why.trim().is_empty() => why.trim().to_string(),
            Failure::Failed(_) | Failure::BadAnswer => "Something went wrong. Please try again.".into(),
        }
    }
}

/// The JSON value in the output. Tolerates lines before it: takes from the first line starting with `{` or `[`.
pub fn json(out: &[u8]) -> Option<Value> {
    if let Ok(v) = serde_json::from_slice::<Value>(out) {
        return Some(v);
    }
    let text = String::from_utf8_lossy(out);
    let lines: Vec<&str> = text.split('\n').collect();
    let i = lines.iter().position(|l| l.starts_with('{') || l.starts_with('['))?;
    serde_json::from_str(&lines[i..].join("\n")).ok()
}

fn text(o: &Value, key: &str) -> Option<String> {
    o.get(key).and_then(Value::as_str).map(str::to_string)
}

fn title(o: &Value) -> String {
    text(o, "title").filter(|t| !t.is_empty()).unwrap_or_else(|| "Untitled".into())
}

/// `{"error": "…"}`: the program said why it couldn't.
fn error_in(v: &Value) -> Option<Failure> {
    text(v, "error").map(Failure::Failed)
}

pub fn parse_clients(out: &[u8]) -> Result<Vec<AiClient>, Failure> {
    let v = json(out).ok_or(Failure::BadAnswer)?;
    // `[...]`, or `{"clients": [...]}` just in case.
    let list = v.as_array().or_else(|| v.get("clients").and_then(Value::as_array)).ok_or(Failure::BadAnswer)?;
    Ok(list
        .iter()
        .filter_map(|o| {
            let id = text(o, "id").filter(|i| !i.is_empty())?;
            Some(AiClient {
                name: text(o, "name").filter(|n| !n.is_empty()).unwrap_or_else(|| id.clone()),
                installed: o.get("installed").and_then(Value::as_bool).unwrap_or(false),
                connected: o.get("connected").and_then(Value::as_bool).unwrap_or(false),
                how: text(o, "how").unwrap_or_else(|| "config".into()),
                id,
            })
        })
        .collect())
}

pub fn parse_connect(out: &[u8]) -> Result<ConnectAnswer, Failure> {
    let v = json(out).ok_or(Failure::BadAnswer)?;
    let Some(ok) = v.get("ok").and_then(Value::as_bool) else { return Err(error_in(&v).unwrap_or(Failure::BadAnswer)) };
    Ok(ConnectAnswer { ok, message: text(&v, "message").unwrap_or_default(), how: text(&v, "how") })
}

pub fn parse_day(out: &[u8]) -> Result<DayAnswer, Failure> {
    let v = json(out).ok_or(Failure::BadAnswer)?;
    let Some(t) = text(&v, "text") else { return Err(error_in(&v).unwrap_or(Failure::BadAnswer)) };
    let chars = v.get("chars").and_then(Value::as_u64).map_or_else(|| t.chars().count(), |n| n as usize);
    Ok(DayAnswer { text: t, chars })
}

pub fn parse_save(out: &[u8]) -> Result<SaveAnswer, Failure> {
    let v = json(out).ok_or(Failure::BadAnswer)?;
    if v.get("saved").is_none() && v.get("refused").is_none() {
        return Err(error_in(&v).unwrap_or(Failure::BadAnswer));
    }
    let list = |key: &str| v.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(SaveAnswer {
        saved: list("saved").iter().map(|o| Saved { title: title(o), path: text(o, "path").unwrap_or_default() }).collect(),
        refused: list("refused").iter().map(|o| Refused { title: title(o), why: text(o, "why").unwrap_or_default() }).collect(),
    })
}

/// Settings → Connect your AI: installed apps first (Claude first among them), then by name.
pub fn sorted(mut list: Vec<AiClient>) -> Vec<AiClient> {
    list.sort_by(|a, b| {
        b.installed
            .cmp(&a.installed)
            .then_with(|| (b.id == CLAUDE_ID).cmp(&(a.id == CLAUDE_ID)))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    list
}

// MARK: running the program

/// How to start the program: the binary, or (developing) node with mcp/dist/index.js.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Launch {
    Binary(PathBuf),
    Node { node: PathBuf, script: PathBuf },
}

impl std::fmt::Display for Launch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Launch::Binary(p) => write!(f, "{}", p.display()),
            Launch::Node { node, script } => write!(f, "{} {}", node.display(), script.display()),
        }
    }
}

/// The repo's binary for this OS and CPU in mcp/dist-bin (when developing).
pub fn repo_binary_name() -> String {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    format!("{PROGRAM}-{os}-{arch}{}", if cfg!(windows) { ".exe" } else { "" })
}

/// Where the program is: the first of `places` that exists (inside / next to the app), else a repo root's
/// mcp/dist-bin binary, else its mcp/dist/index.js run with `node` (when one is given).
pub fn find(places: &[PathBuf], repos: &[PathBuf], node: Option<&Path>, exists: &dyn Fn(&Path) -> bool) -> Option<Launch> {
    if let Some(p) = places.iter().find(|p| exists(p)) {
        return Some(Launch::Binary(p.clone()));
    }
    let bin = repo_binary_name();
    if let Some(p) = repos.iter().map(|r| r.join("mcp/dist-bin").join(&bin)).find(|p| exists(p)) {
        return Some(Launch::Binary(p));
    }
    let node = node?;
    repos.iter().map(|r| r.join("mcp/dist/index.js")).find(|p| exists(p)).map(|script| Launch::Node { node: node.to_path_buf(), script })
}

/// Folders that may be the repo root, nearest first: up from the app's exe (…/capture-rs/target/release/x →
/// …), then `cwd` and its parent. No duplicates.
pub fn repo_candidates(exe: &Path, cwd: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = exe.ancestors().skip(1).take(5).map(Path::to_path_buf).collect();
    out.push(cwd.to_path_buf());
    if let Some(p) = cwd.parent() {
        out.push(p.to_path_buf());
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|p| !p.as_os_str().is_empty() && seen.insert(p.clone()));
    out
}

/// `node` on PATH (for the developer fallback).
pub fn node_on_path() -> Option<PathBuf> {
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(name)).find(|p| p.is_file())
}

/// How long each command may take (the same as the Mac).
pub fn timeout_for(command: &str) -> Duration {
    Duration::from_secs(match command {
        "clients" => 10,
        "day" => 60,
        "save" => 30,
        _ => 20,
    })
}

/// Runs `<program> <args>`; `input` goes to its stdin. Gives its stdout, or a plain failure. No console window
/// on Windows. Killed after `timeout`.
pub fn run(launch: &Launch, args: &[&str], input: Option<&[u8]>, timeout: Duration) -> Result<Vec<u8>, Failure> {
    let mut cmd = match launch {
        Launch::Binary(p) => Command::new(p),
        Launch::Node { node, script } => {
            let mut c = Command::new(node);
            c.arg(script);
            c
        }
    };
    cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|_| Failure::Missing)?;
    // Read both pipes and write stdin at the same time, so a full pipe never blocks the program.
    let pipe = |r: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out = pipe(child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let err = pipe(child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    if let Some(mut stdin) = child.stdin.take() {
        let data = input.unwrap_or_default().to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&data);
        });
    }
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::Timeout);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return Err(Failure::Missing),
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if !status.success() && stdout.iter().all(u8::is_ascii_whitespace) {
        let why = String::from_utf8_lossy(&stderr).lines().rev().find(|l| !l.trim().is_empty()).unwrap_or_default().trim().to_string();
        return Err(Failure::Failed(if why.chars().count() > 200 { String::new() } else { why }));
    }
    Ok(stdout)
}

/// The program's commands, each with its time limit.
pub fn clients(launch: &Launch) -> Result<Vec<AiClient>, Failure> {
    parse_clients(&run(launch, &["clients"], None, timeout_for("clients"))?)
}
pub fn connect(launch: &Launch, id: &str) -> Result<ConnectAnswer, Failure> {
    parse_connect(&run(launch, &["connect", id], None, timeout_for("connect"))?)
}
pub fn disconnect(launch: &Launch, id: &str) -> Result<ConnectAnswer, Failure> {
    parse_connect(&run(launch, &["disconnect", id], None, timeout_for("disconnect"))?)
}
pub fn day(launch: &Launch, date: Option<&str>) -> Result<DayAnswer, Failure> {
    let mut args = vec!["day"];
    if let Some(d) = date {
        args.extend(["--date", d]);
    }
    parse_day(&run(launch, &args, None, timeout_for("day"))?)
}
pub fn save(launch: &Launch, reply: &str) -> Result<SaveAnswer, Failure> {
    parse_save(&run(launch, &["save"], Some(reply.as_bytes()), timeout_for("save"))?)
}

// MARK: Your AI

pub const CLAUDE_ID: &str = "claude";
/// config.json key.
pub const KEY_AI: &str = "ai";
/// "Any other AI".
pub const ANY_ID: &str = "any";

/// An AI on the web: Copy my day, then its site opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Website {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
}

pub const WEBSITES: [Website; 2] = [
    Website { id: "chatgpt-web", name: "ChatGPT (website)", url: "https://chatgpt.com" },
    Website { id: "gemini-web", name: "Gemini (website)", url: "https://gemini.google.com" },
];

/// Apps that are connected but make capsules with Copy my day: the ChatGPT app loads Context Capsule's tools but
/// doesn't give them to its chats yet (openai/codex#38162, open since Aug 2026), so a request would go nowhere.
pub const COPY_APPS: [&str; 1] = ["chatgpt"];

/// "Your AI" (config.json "ai"): an id from `clients`, a known website, or "any" for any other AI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum YourAi {
    Claude,
    /// Another app connected through MCP (Cursor, VS Code…).
    App(String),
    /// ChatGPT or Gemini on the web: Copy my day, then their site opens.
    Website(Website),
    /// Any other AI: Copy my day.
    Other,
}

impl YourAi {
    pub fn of(id: &str) -> YourAi {
        let id = id.trim();
        if id == CLAUDE_ID {
            YourAi::Claude
        } else if id == ANY_ID || id.is_empty() {
            YourAi::Other
        } else if let Some(w) = WEBSITES.iter().find(|w| w.id == id) {
            YourAi::Website(*w)
        } else {
            YourAi::App(id.to_string())
        }
    }

    /// Copy my day / Save from clipboard is the way (no MCP, or MCP that its chats can't use yet: `COPY_APPS`).
    pub fn uses_copy(&self) -> bool {
        match self {
            YourAi::Website(_) | YourAi::Other => true,
            YourAi::App(id) => COPY_APPS.contains(&id.as_str()),
            YourAi::Claude => false,
        }
    }

    /// Its name in sentences: "Claude", "Cursor", "ChatGPT", "your AI".
    pub fn name(&self, clients: &[AiClient]) -> String {
        match self {
            YourAi::Claude => "Claude".into(),
            YourAi::App(id) => clients.iter().find(|c| &c.id == id).map_or_else(|| id.clone(), |c| c.name.clone()),
            YourAi::Website(w) => w.name.replace(" (website)", ""),
            YourAi::Other => "your AI".into(),
        }
    }
}

/// config.json "ai" (None when not set).
pub fn saved_choice(config: &Path) -> Option<String> {
    let v: Value = serde_json::from_slice(&std::fs::read(config).ok()?).ok()?;
    v.get(KEY_AI).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// The id in use: the saved one, else Claude if it's connected, else the first connected app, else "any".
/// A saved app id the program doesn't list (any more) falls back the same way; with no list yet it is kept.
pub fn resolve(saved: Option<&str>, clients: &[AiClient]) -> String {
    if let Some(s) = saved.map(str::trim).filter(|s| !s.is_empty()) {
        if s == ANY_ID || WEBSITES.iter().any(|w| w.id == s) || clients.is_empty() || clients.iter().any(|c| c.id == s) {
            return s.to_string();
        }
    }
    if clients.iter().any(|c| c.id == CLAUDE_ID && c.connected) {
        return CLAUDE_ID.into();
    }
    clients.iter().find(|c| c.connected).map_or_else(|| ANY_ID.into(), |c| c.id.clone())
}

/// After connecting `id`: it becomes Your AI if nothing was chosen and nothing else was connected yet.
pub fn first_connected_becomes_choice(saved: Option<&str>, clients_before: &[AiClient]) -> bool {
    saved.is_none() && !clients_before.iter().any(|c| c.connected)
}

// MARK: words (the Mac's, word for word)

pub const CONNECT: &str = "Connect";
/// After Connect for Claude (the extension): Claude asks to install it.
pub const CLAUDE_INSTALL_NOTE: &str = "Click Install in Claude. The ✓ shows when it's done.";
pub const SAVE_FROM_CLIPBOARD: &str = "Save from clipboard";
/// Shown after Copy my day, until the reply is saved.
pub const PASTE_LINE: &str = "Paste it into your AI. When it answers, copy its whole reply, then click Save from clipboard.";
pub const NOTHING_TODAY: &str = "Nothing recorded today yet.";
pub const COPY_FAILED: &str = "Couldn't copy your day";
pub const CLIPBOARD_EMPTY: &str = "The clipboard is empty";
pub const CLIPBOARD_EMPTY_BODY: &str = "Copy your AI's whole reply first, then click Save from clipboard.";
pub const SAVE_FAILED: &str = "Couldn't save";

/// After connecting an app, when it didn't say.
pub fn connected_note(name: &str) -> String {
    format!("Connected to {name}. Restart {name} to use it.")
}
pub fn connect_failed(name: &str) -> String {
    format!("Couldn't connect {name}")
}

/// A request for an app that isn't connected yet: (title, text). `why`: "to read your day"; `again`: "click
/// Make today's capsules again".
pub fn connect_first(name: &str, why: &str, again: &str) -> (String, String) {
    (format!("Connect {name} first"), format!("{name} needs Context Capsule {why}. Click Connect. After that, {again}."))
}

/// A request copied for another app: whether the app could be opened.
pub fn copied_for(name: &str, opened: bool) -> String {
    if opened {
        format!("Copied. Paste it in a new chat in {name}.")
    } else {
        format!("Copied. Open {name} and paste it in a new chat.")
    }
}

/// What Save from clipboard says: a title, then what was saved and what wasn't (and why).
pub fn summary(r: &SaveAnswer) -> (String, String) {
    let n = r.saved.len();
    if n == 0 && r.refused.is_empty() {
        return (
            "No capsules found".into(),
            "The clipboard has no capsule in it. Copy your AI's whole reply, then click Save from clipboard again.".into(),
        );
    }
    let title = if n == 0 { "Nothing was saved".to_string() } else { format!("Saved {n} capsule{}", if n == 1 { "" } else { "s" }) };
    let mut lines: Vec<String> = r.saved.iter().map(|s| format!("✓ {}", s.title)).collect();
    if !r.refused.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push("Not saved:".into());
        lines.extend(r.refused.iter().map(|f| if f.why.is_empty() { format!("• {}", f.title) } else { format!("• {}: {}", f.title, f.why) }));
    }
    (title, lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(id: &str, installed: bool, connected: bool) -> AiClient {
        AiClient { id: id.into(), name: id.to_uppercase(), installed, connected, how: "config".into() }
    }

    #[test]
    fn reads_answers() {
        let out = br#"[{"id":"claude","name":"Claude","installed":true,"connected":false,"how":"extension"},
            {"id":"cursor","name":"Cursor","installed":false,"connected":false,"how":"config","extra":1},{"name":"no id"}]"#;
        let list = parse_clients(out).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].is_extension() && list[0].installed && !list[1].is_extension());
        assert_eq!(parse_clients(br#"{"clients":[{"id":"x"}]}"#).unwrap()[0], AiClient { id: "x".into(), name: "x".into(), installed: false, connected: false, how: "config".into() });
        assert_eq!(parse_clients(b"bun says hi\n[{\"id\":\"a\"}]\n").unwrap().len(), 1, "lines before the JSON");
        assert_eq!(parse_clients(b"nope"), Err(Failure::BadAnswer));
        assert_eq!(parse_clients(br#"{"x":1}"#), Err(Failure::BadAnswer));

        assert_eq!(parse_connect(br#"{"ok":false,"how":"extension","message":"m"}"#).unwrap(), ConnectAnswer { ok: false, message: "m".into(), how: Some("extension".into()) });
        assert_eq!(parse_day(r#"{"text":"héllo","chars":5}"#.as_bytes()).unwrap(), DayAnswer { text: "héllo".into(), chars: 5 });
        assert_eq!(parse_day(r#"{"text":"héllo"}"#.as_bytes()).unwrap().chars, 5);
        assert_eq!(parse_day(br#"{"error":"\"x\" is not a date."}"#), Err(Failure::Failed("\"x\" is not a date.".into())));
        let s = parse_save(br#"{"saved":[{"title":"","path":"/p"}],"refused":[{"title":"B","why":"no Next steps"}]}"#).unwrap();
        assert_eq!(s.saved[0], Saved { title: "Untitled".into(), path: "/p".into() });
        assert_eq!(s.refused[0].why, "no Next steps");
        assert_eq!(parse_save(br#"{"saved":[]}"#).unwrap(), SaveAnswer::default());
        assert_eq!(parse_save(br#"{"error":"boom"}"#), Err(Failure::Failed("boom".into())));
    }

    #[test]
    fn order_and_choice() {
        let list = sorted(vec![client("zed", true, false), client("cursor", false, false), AiClient::claude(true, false), client("codex", true, true)]);
        assert_eq!(list.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["claude", "codex", "zed", "cursor"]);

        assert_eq!(resolve(None, &[]), "any");
        assert_eq!(resolve(Some("cursor"), &[]), "cursor", "no list yet: kept");
        assert_eq!(resolve(Some("gone"), &list), "codex", "not listed: the first connected");
        assert_eq!(resolve(Some(" chatgpt-web "), &list), "chatgpt-web");
        let with_claude = vec![AiClient::claude(true, true), client("codex", true, true)];
        assert_eq!(resolve(None, &with_claude), "claude");
        assert_eq!(resolve(None, &[client("a", true, false)]), "any");

        assert_eq!(YourAi::of("claude"), YourAi::Claude);
        assert_eq!(YourAi::of(""), YourAi::Other);
        assert!(YourAi::of("gemini-web").uses_copy() && YourAi::of("any").uses_copy());
        assert!(!YourAi::of("cursor").uses_copy() && !YourAi::Claude.uses_copy());
        assert!(YourAi::of("chatgpt").uses_copy(), "the ChatGPT app's chats can't use the tools yet");
        assert!(!YourAi::of("codex").uses_copy(), "Codex can");
        assert_eq!(YourAi::of("chatgpt-web").name(&[]), "ChatGPT");
        assert_eq!(YourAi::of("codex").name(&list), "CODEX");

        assert!(first_connected_becomes_choice(None, &[client("a", true, false)]));
        assert!(!first_connected_becomes_choice(Some("any"), &[]));
        assert!(!first_connected_becomes_choice(None, &[client("a", true, true)]));
    }

    #[test]
    fn config_and_words() {
        let dir = std::env::temp_dir().join(format!("cc-anyai-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.json");
        assert_eq!(saved_choice(&config), None);
        std::fs::write(&config, r#"{"ai":"  ","author":"S"}"#).unwrap();
        assert_eq!(saved_choice(&config), None);
        crate::settings::ConfigFile::set(&config, KEY_AI, Some("cursor".into())).unwrap();
        assert_eq!(saved_choice(&config).as_deref(), Some("cursor"));
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(connect_first("Cursor", "to read your day", "click Make today's capsules again").1, "Cursor needs Context Capsule to read your day. Click Connect. After that, click Make today's capsules again.");
        assert_eq!(copied_for("Cursor", false), "Copied. Open Cursor and paste it in a new chat.");

        let none = summary(&SaveAnswer::default());
        assert_eq!(none.0, "No capsules found");
        let r = SaveAnswer {
            saved: vec![Saved { title: "Website".into(), path: "/a".into() }],
            refused: vec![Refused { title: "Shop".into(), why: "no Next steps".into() }, Refused { title: "X".into(), why: String::new() }],
        };
        assert_eq!(summary(&r), ("Saved 1 capsule".into(), "✓ Website\n\nNot saved:\n• Shop: no Next steps\n• X".into()));
        assert_eq!(summary(&SaveAnswer { saved: vec![], refused: r.refused.clone() }).0, "Nothing was saved");
        assert_eq!(Failure::Failed("  ".into()).message(), "Something went wrong. Please try again.");
        assert!(Failure::Missing.message().contains("download Context Capsule again"));
    }

    #[test]
    fn finds_the_program() {
        let exists = |p: &Path| p.ends_with(format!("mcp/dist-bin/{}", repo_binary_name())) || p.ends_with("next/context-capsule-mcp");
        let repos = repo_candidates(Path::new("/r/capture-rs/target/release/context-capsule"), Path::new("/w"));
        assert_eq!(repos[..3], [PathBuf::from("/r/capture-rs/target/release"), PathBuf::from("/r/capture-rs/target"), PathBuf::from("/r/capture-rs")]);
        assert!(repos.contains(&PathBuf::from("/r")) && repos.contains(&PathBuf::from("/w")) && repos.contains(&PathBuf::from("/")));
        assert_eq!(find(&[PathBuf::from("/next/context-capsule-mcp")], &repos, None, &exists), Some(Launch::Binary("/next/context-capsule-mcp".into())));
        assert_eq!(find(&[PathBuf::from("/none")], &repos, None, &exists), Some(Launch::Binary(PathBuf::from("/r/capture-rs/target/release/mcp/dist-bin").join(repo_binary_name()))));
        let js = |p: &Path| p == Path::new("/r/mcp/dist/index.js");
        assert_eq!(find(&[], &repos, None, &js), None, "no node: no fallback");
        assert_eq!(find(&[], &repos, Some(Path::new("/n/node")), &js), Some(Launch::Node { node: "/n/node".into(), script: "/r/mcp/dist/index.js".into() }));
    }

    /// A fake program: a shell script that answers like context-capsule-mcp.
    #[cfg(unix)]
    fn fake(dir: &Path, body: &str) -> Launch {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("context-capsule-mcp");
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        Launch::Binary(p)
    }

    #[cfg(unix)]
    #[test]
    fn runs_the_program() {
        let dir = std::env::temp_dir().join(format!("cc-anyai-run-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = fake(
            &dir,
            r#"case "$1" in
  clients) echo '[{"id":"cursor","name":"Cursor","installed":true,"connected":true,"how":"config"}]' ;;
  connect) echo "{\"ok\":true,\"message\":\"Connected to $2.\"}" ;;
  disconnect) echo 'not json'; exit 1 ;;
  day) [ "$2" = "--date" ] && echo "{\"text\":\"day $3\",\"chars\":14}" || echo '{"text":"today","chars":5}' ;;
  save) n=$(wc -c | tr -d ' '); echo "{\"saved\":[{\"title\":\"got $n\",\"path\":\"/x\"}],\"refused\":[]}" ;;
  slow) sleep 5 ;;
  fail) echo "first" >&2; echo "It broke." >&2; exit 3 ;;
  big) head -c 300000 /dev/zero | tr '\0' 'a'; echo ;;
esac"#,
        );
        assert_eq!(clients(&p).unwrap()[0].name, "Cursor");
        assert_eq!(connect(&p, "cursor").unwrap().message, "Connected to cursor.");
        assert_eq!(disconnect(&p, "cursor"), Err(Failure::BadAnswer));
        assert_eq!(day(&p, None).unwrap().text, "today");
        assert_eq!(day(&p, Some("2026-10-19")).unwrap().text, "day 2026-10-19");
        let reply = "x".repeat(200_000); // bigger than a pipe buffer
        assert_eq!(save(&p, &reply).unwrap().saved[0].title, "got 200000");
        let start = Instant::now();
        assert_eq!(run(&p, &["slow"], None, Duration::from_millis(300)), Err(Failure::Timeout));
        assert!(start.elapsed() < Duration::from_secs(3), "killed, not waited for");
        assert_eq!(run(&p, &["fail"], None, Duration::from_secs(5)), Err(Failure::Failed("It broke.".into())));
        assert_eq!(run(&p, &["big"], None, Duration::from_secs(5)).unwrap().len(), 300_001, "a full pipe never blocks it");
        assert_eq!(run(&Launch::Binary(dir.join("missing")), &["clients"], None, Duration::from_secs(1)), Err(Failure::Missing));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
