// context-capsule — core: the shared window's bridge (ui/BRIDGE.md) for the Windows and Linux apps — the page's
// files served from inside the program, the script that gives the page `window.capsuleHost`, and one dispatch of
// every method: raw files under the data folder (a strict allow-list), config.json, projects.json, deleting from the
// Timeline, running context-capsule-mcp, sharing a capsule. What only the app can do (the recorder, pickers, the
// clipboard, opening things, set up, updates) is the [`Host`] each app implements.
//! The page asks `{id, method, args}`; [`handle`] answers `{id, ok, value}` or `{id, ok:false, error}`. Call it off
//! the UI thread: reads, writes and program runs happen inside. Events go back as [`event_json`].

use crate::anyai::{self, Failure, Launch};
use crate::json::write_atomically;
use crate::paths::Paths;
use crate::platform::Status;
use crate::settings::ConfigFile;
use crate::shortcuts::{Hotkeys, Shortcut};
use crate::timeline;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/ui_files.rs"));

// MARK: the page's files

/// A file of the page by its URL path ("/", "/index.html", "/js/app.js"): its bytes and content type.
pub fn asset(url_path: &str) -> Option<(&'static [u8], &'static str)> {
    let path = url_path.split(['?', '#']).next().unwrap_or("");
    let path = path.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    if path.split('/').any(|s| s == ".." || s == ".") {
        return None;
    }
    FILES.iter().find(|(p, _)| *p == path).map(|(p, bytes)| (*bytes, mime(p)))
}

/// Content type by extension.
pub fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "woff2" => "font/woff2",
        "json" => "application/json; charset=utf-8",
        _ => "application/octet-stream",
    }
}

// MARK: the script the host puts in the page before it loads

/// How messages travel between the page and the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    /// WebView2: `chrome.webview.postMessage(obj)`; answers and events arrive as `message` events (JSON objects).
    WebView2,
    /// wry on WebView2: `window.ipc.postMessage(json text)`; the app runs `capsuleHost._receive(<json>)`.
    Ipc,
    /// WebKit (GTK, and WKWebView): `webkit.messageHandlers.capsule.postMessage(json text)`; the app runs
    /// `capsuleHost._receive(<json>)`.
    WebKit,
}

/// The script that defines `window.capsuleHost` (`call`, `on`); run it before the page's own scripts. The latest
/// `state` and `appearance` are replayed to listeners that come later (the page listens after its first reads).
pub fn shim_js(transport: Transport) -> String {
    let (send, listen) = match transport {
        Transport::WebView2 => (
            "(m) => window.chrome.webview.postMessage(m)",
            "window.chrome.webview.addEventListener('message', (e) => host._receive(e.data));",
        ),
        Transport::Ipc => ("(m) => window.ipc.postMessage(JSON.stringify(m))", ""),
        Transport::WebKit => ("(m) => window.webkit.messageHandlers.capsule.postMessage(JSON.stringify(m))", ""),
    };
    SHIM.replace("%SEND%", send).replace("%LISTEN%", listen)
}

const SHIM: &str = r#"(() => {
  if (window.capsuleHost) return;
  const send = %SEND%;
  const pending = new Map();
  const listeners = new Map();
  const last = new Map();
  let next = 1;
  const host = {
    call(method, args) {
      return new Promise((resolve, reject) => {
        const id = next++;
        pending.set(id, { resolve, reject });
        try { send({ id, method, args: args ?? {} }); } catch (e) { pending.delete(id); reject(e); }
      });
    },
    on(event, fn) {
      if (!listeners.has(event)) listeners.set(event, new Set());
      listeners.get(event).add(fn);
      if (last.has(event)) queueMicrotask(() => listeners.get(event)?.has(fn) && fn(last.get(event)));
      return () => listeners.get(event)?.delete(fn);
    },
    _emit(event, payload) {
      if (event === "state" || event === "appearance") last.set(event, payload);
      for (const fn of [...(listeners.get(event) ?? [])]) {
        try { fn(payload); } catch (e) { console.error("capsuleHost " + event, e); }
      }
    },
    _reply(id, answer) {
      const p = pending.get(id);
      if (!p) return;
      pending.delete(id);
      if (answer && answer.ok) p.resolve(answer.value ?? null);
      else p.reject(new Error((answer && answer.error) || "Something went wrong. Please try again."));
    },
    _receive(m) {
      if (typeof m === "string") { try { m = JSON.parse(m); } catch { return; } }
      if (!m || typeof m !== "object") return;
      if ("id" in m) host._reply(m.id, m);
      else if (typeof m.event === "string") host._emit(m.event, m.payload);
    },
  };
  Object.defineProperty(window, "capsuleHost", { value: Object.freeze(host), writable: false, configurable: false });
  %LISTEN%
})();
"#;

// MARK: messages

/// `{"event": …, "payload": …}`: what the app sends the page.
pub fn event_json(event: &str, payload: Value) -> String {
    json!({ "event": event, "payload": payload }).to_string()
}

/// One request from the page: `{id, method, args}`.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub id: Value,
    pub method: String,
    pub args: Value,
}

impl Request {
    /// From the message text (a JSON object; WebKit sends it as a string, WebView2 as an object turned to text).
    pub fn parse(text: &str) -> Option<Request> {
        let mut v: Value = serde_json::from_str(text).ok()?;
        if let Value::String(inner) = &v {
            v = serde_json::from_str(inner).ok()?;
        }
        let Value::Object(mut o) = v else { return None };
        let method = o.get("method")?.as_str()?.to_string();
        let id = o.remove("id").unwrap_or(Value::Null);
        let args = match o.remove("args") {
            Some(Value::Null) | None => Value::Object(Map::new()),
            Some(a) => a,
        };
        Some(Request { id, method, args })
    }
}

/// The answer to a request: `{id, ok:true, value}` or `{id, ok:false, error}`.
pub fn answer_json(id: &Value, result: Result<Value, String>) -> String {
    match result {
        Ok(value) => json!({ "id": id, "ok": true, "value": value }).to_string(),
        Err(error) => json!({ "id": id, "ok": false, "error": error }).to_string(),
    }
}

/// A whole message in, the answer out (None: not a request, nothing to answer). Blocks: call off the UI thread.
pub fn handle(paths: &Paths, host: &dyn Host, text: &str) -> Option<String> {
    let req = Request::parse(text)?;
    let result = dispatch(paths, host, &req.method, &req.args);
    Some(answer_json(&req.id, result))
}

// MARK: what the app does

/// `host.hello`'s answer, from the app (the version comes from here).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hello {
    /// "windows" / "linux".
    pub platform: &'static str,
    /// BCP 47 ("en-GB").
    pub locale: String,
    /// The system's 12/24-hour choice; None: the locale's.
    pub hour12: Option<bool>,
    /// 1 = Sunday … 7 = Saturday; None: from the locale.
    pub first_weekday: Option<u8>,
    pub user_name: String,
    pub dark: bool,
    pub first_run: bool,
    /// The page to show first ("settings"…); None: Home.
    pub page: Option<String>,
    /// The window shows the system's glass behind the page (Windows 11 Mica with transparency effects on):
    /// `material: "glass"`; else "none" (Windows 10, Linux).
    pub glass: bool,
}

impl Hello {
    pub fn json(&self) -> Value {
        let voice: Vec<Value> = crate::settings::language_options(None)
            .into_iter()
            .filter(|(id, _)| !id.is_empty())
            .map(|(id, name)| json!({ "id": id, "name": name }))
            .collect();
        json!({
            "platform": self.platform,
            "version": env!("CARGO_PKG_VERSION"),
            "locale": self.locale,
            "hour12": self.hour12,
            "firstWeekday": self.first_weekday,
            "userName": self.user_name,
            "dark": self.dark,
            "firstRun": self.first_run,
            "page": self.page,
            "material": material(self.glass),
            "voiceLanguages": voice,
            "voiceLanguageDefault": crate::settings::SAME_LANGUAGE,
        })
    }
}

/// `material` for `host.hello` and its updates: "glass" or "none".
pub fn material(glass: bool) -> &'static str {
    if glass {
        "glass"
    } else {
        "none"
    }
}

/// How "Copy file / Save a copy / Show in folder" ask for a capsule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShareHow {
    CopyFile,
    CopyText,
    SaveCopy,
    Reveal,
}

/// Everything only the app can do. Called off the UI thread (from [`handle`]); an app that must do something on
/// its UI thread (a dialog, the clipboard) hands it there and waits. Errors are plain words for people.
pub trait Host: Send + Sync {
    fn hello(&self) -> Hello;
    /// A config.json key was written (`appearance`: the window's look; `auto_update`; `share_screens`…).
    fn config_changed(&self, _key: &str, _value: &Value) {}
    /// projects.json was written: the recorder detects with it.
    fn projects_changed(&self) {}
    /// "toggle" | "voice" | "add-screen" | "new-project" | "project" (with `id`, None = back to Auto).
    fn recorder(&self, action: &str, id: Option<&str>) -> Result<(), String>;
    /// Register both global shortcuts; Err("taken") keeps the old ones. config.json is saved here after Ok.
    fn set_hotkeys(&self, keys: Hotkeys) -> Result<(), String>;
    /// While Settings waits for new keys, the global shortcuts are off.
    fn listen_keys(&self, on: bool);
    /// The setup steps this platform has, in order, and whether each is done ("voice", "ai", "login").
    fn setup_status(&self) -> Vec<(&'static str, bool)>;
    fn setup_do(&self, id: &str) -> Result<(), String>;
    /// Where context-capsule-mcp is (None: missing).
    fn mcp(&self) -> Option<Launch>;
    /// Put `prompt` on the clipboard and open that AI app on a new chat; false: the app wasn't found.
    fn ai_open(&self, id: &str, prompt: &str) -> Result<bool, String>;
    fn clipboard_read(&self) -> Result<String, String>;
    fn clipboard_write(&self, text: &str) -> Result<(), String>;
    /// Copy the file itself (to paste in a chat or a folder).
    fn copy_file(&self, path: &Path) -> Result<(), String>;
    /// An http(s) link in the default browser (checked already).
    fn open_url(&self, url: &str) -> Result<(), String>;
    /// A capsule in the default app for .md (or a text editor).
    fn open_file(&self, path: &Path) -> Result<(), String>;
    /// Show the file in the file manager.
    fn reveal(&self, path: &Path) -> Result<(), String>;
    /// The save panel for a copy named `name`; None: cancelled.
    fn save_panel(&self, name: &str) -> Option<PathBuf>;
    /// The open panel for a received capsule (.md); None: cancelled.
    fn pick_capsule(&self) -> Option<PathBuf>;
    /// An app never to record: (id, name); None: cancelled.
    fn pick_app(&self) -> Option<(String, String)>;
    /// Names for the Never record list (ids it doesn't know are left out).
    fn app_names(&self, ids: &[String]) -> Map<String, Value>;
    /// The app's icon as a PNG of 64 × 64 pixels (`app_icons::SIZE`), by its id as events store it (`bundle`:
    /// Windows the exe name, Linux the desktop id) else its name; None: not found. Each app is asked once (the
    /// bridge caches answers).
    fn app_icon(&self, _app: &str, _bundle: &str) -> Option<Vec<u8>> {
        None
    }
    /// "check" | "restart"; progress comes in `state.update`.
    fn updates(&self, action: &str) -> Result<(), String>;
    /// Send an event to the page (any thread).
    fn emit(&self, event: &str, payload: Value);
}

/// `state`'s payload: the recorder's status plus what the app knows.
pub fn state_json(status: &Status, several_screens: bool, update_line: &str, update_ready: bool, update_working: bool) -> Value {
    json!({
        "recording": status.recording,
        "picking": status.picking,
        "listening": status.listening,
        "started_at": status.started_at.map(crate::clock::iso),
        "session_id": status.session_id,
        "events": status.events,
        "project": status.project,
        "project_by": status.project_by,
        "screens": status.sources,
        "can_add_screen": status.can_add_screen(),
        "several_screens": several_screens,
        "update": { "line": update_line, "ready": update_ready, "working": update_working },
    })
}

// MARK: paths

/// The keys `config.set` may write (shortcuts go through `shortcuts.set`).
pub const CONFIG_KEYS: [&str; 10] =
    ["appearance", "share_screens", "auto_update", "remind_at", "keep_recordings_days", "sounds", "voice_locale", "exclude_apps", "author", "ai"];

/// Names Windows keeps for devices ("CON.md" is not a file there).
fn is_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit())
}

/// One part of a path the page may name: not empty, not hidden, no separators, no drive or stream (`:`), no
/// control characters, no device name, no trailing dot or space (Windows drops them).
fn safe_name(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('.')
        && !s.ends_with('.')
        && !s.ends_with(' ')
        && !s.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control())
        && !is_device_name(s)
}

/// A capsule's file name: safe and ending in .md.
pub fn capsule_name(name: &str) -> bool {
    safe_name(name) && name.len() > 3 && name.to_ascii_lowercase().ends_with(".md")
}

/// Is `rel` one of the files the page may read? `config.json`, `projects.json`, `state.json`,
/// `sessions/<id>/events.jsonl|gaps.jsonl|assign.json|session.json`, `capsules/<name>.md`.
pub fn allowed(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    match parts.as_slice() {
        ["config.json" | "projects.json" | "state.json"] => true,
        ["sessions", id, "events.jsonl" | "gaps.jsonl" | "assign.json" | "session.json"] => safe_name(id),
        ["capsules", name] => capsule_name(name),
        _ => false,
    }
}

/// The file for an allowed path, inside the data folder (a link pointing out of it is refused).
pub fn resolve(paths: &Paths, rel: &str) -> Result<PathBuf, String> {
    if !allowed(rel) {
        return Err(format!("not allowed: {rel}"));
    }
    let file = rel.split('/').fold(paths.root.clone(), |p, s| p.join(s));
    if let (Ok(real), Ok(root)) = (std::fs::canonicalize(&file), std::fs::canonicalize(&paths.root)) {
        if !real.starts_with(&root) {
            return Err(format!("not allowed: {rel}"));
        }
    }
    Ok(file)
}

/// `data.list`: session folders, or capsule files (hidden ones and `.history` left out). Sorted.
pub fn list(paths: &Paths, dir: &str) -> Result<Vec<String>, String> {
    let (folder, dirs) = match dir {
        "sessions" => (paths.sessions(), true),
        "capsules" => (paths.capsules(), false),
        _ => return Err(format!("can't list {dir}")),
    };
    let mut names: Vec<String> = std::fs::read_dir(&folder)
        .map(|it| {
            it.flatten()
                .filter(|e| e.file_type().is_ok_and(|t| if dirs { t.is_dir() } else { t.is_file() }))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| safe_name(n))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    Ok(names)
}

/// `data.read`: each file's text (null when missing). Every path must be allowed, else nothing is read.
pub fn read(paths: &Paths, files: &[String]) -> Result<Map<String, Value>, String> {
    let resolved: Vec<PathBuf> = files.iter().map(|f| resolve(paths, f)).collect::<Result<_, _>>()?;
    Ok(files
        .iter()
        .zip(resolved)
        .map(|(rel, file)| {
            let text = std::fs::read(&file).ok().map(|b| Value::String(String::from_utf8_lossy(&b).into_owned()));
            (rel.clone(), text.unwrap_or(Value::Null))
        })
        .collect())
}

/// `config.set`: one key merged into config.json (`null` removes it), every other key kept.
pub fn set_config(paths: &Paths, key: &str, value: &Value) -> Result<(), String> {
    if !CONFIG_KEYS.contains(&key) {
        return Err(format!("can't set {key}"));
    }
    let config = paths.config();
    if value.is_null() {
        let mut obj = match std::fs::read(&config).ok().and_then(|d| serde_json::from_slice::<Value>(&d).ok()) {
            Some(Value::Object(o)) => o,
            _ => return Ok(()),
        };
        if obj.remove(key).is_none() {
            return Ok(());
        }
        let text = serde_json::to_string_pretty(&Value::Object(obj)).map_err(|e| e.to_string())?;
        return write_atomically(&config, &(text + "\n")).map_err(|e| e.to_string());
    }
    ConfigFile::set(&config, key, Some(value.clone())).map_err(|e| e.to_string())
}

/// `projects.save`: projects.json as the page wrote it (it must be JSON with a projects list).
pub fn save_projects(paths: &Paths, text: &str) -> Result<(), String> {
    let v: Value = serde_json::from_str(text).map_err(|_| "That isn't valid projects.json.".to_string())?;
    if !v.get("projects").is_some_and(Value::is_array) {
        return Err("That isn't valid projects.json.".into());
    }
    write_atomically(&paths.projects(), text).map_err(|e| e.to_string())
}

/// `timeline.delete` with lines: for each `(t, line)`, the first line of events.jsonl that is exactly `line` (CR
/// left out) and whose `t` is `t`; all or none, one atomic rewrite. Errors "recording", "gone".
pub fn delete_lines(paths: &Paths, session: &str, lines: &[(String, String)]) -> Result<(), String> {
    if !safe_name(session) || lines.is_empty() {
        return Err("gone".into());
    }
    if timeline::recording_session(paths).as_deref() == Some(session) {
        return Err("recording".into());
    }
    let file = paths.session_dir(session).join("events.jsonl");
    let text = std::fs::read_to_string(&file).map_err(|_| "gone".to_string())?;
    let parts: Vec<&str> = text.split_inclusive('\n').collect();
    let bare = |raw: &'_ str| -> String {
        let r = raw.strip_suffix('\n').unwrap_or(raw);
        r.strip_suffix('\r').unwrap_or(r).to_string()
    };
    let mut gone = vec![false; parts.len()];
    for (t, line) in lines {
        let has_t = serde_json::from_str::<Value>(line).ok().and_then(|v| v.get("t").and_then(Value::as_str).map(|x| x == t)) == Some(true);
        if !has_t {
            return Err("gone".into());
        }
        let Some(i) = (0..parts.len()).find(|&i| !gone[i] && bare(parts[i]) == *line) else { return Err("gone".into()) };
        gone[i] = true;
    }
    let kept: String = parts.iter().zip(&gone).filter(|(_, g)| !**g).map(|(p, _)| *p).collect();
    write_atomically(&file, &kept).map_err(|e| e.to_string())
}

/// `timeline.delete` without lines: the whole recording.
pub fn delete_session(paths: &Paths, session: &str) -> Result<(), String> {
    timeline::delete_session(paths, session).map_err(|r| match r {
        timeline::Refused::Recording => "recording".into(),
        timeline::Refused::NotFound => "gone".into(),
        timeline::Refused::Io(e) => e,
    })
}

/// `mcp.run`: the program's stdout (JSON text), or "missing" / "timeout" / its words.
pub fn run_mcp(launch: Option<Launch>, command: &str, id: Option<&str>, date: Option<&str>, input: Option<&str>) -> Result<String, String> {
    const COMMANDS: [&str; 5] = ["clients", "connect", "disconnect", "day", "save"];
    if !COMMANDS.contains(&command) {
        return Err(format!("unknown command {command}"));
    }
    let mut args = vec![command];
    if matches!(command, "connect" | "disconnect") {
        let id = id.filter(|i| !i.is_empty() && i.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        args.push(id.ok_or("Which AI app?")?);
    }
    if let Some(d) = date.filter(|_| command == "day") {
        if !(d.len() == 10 && chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok()) {
            return Err("That isn't a day.".into());
        }
        args.extend(["--date", d]);
    }
    let launch = launch.ok_or("missing")?;
    match anyai::run(&launch, &args, input.map(str::as_bytes), anyai::timeout_for(command)) {
        Ok(out) => Ok(String::from_utf8_lossy(&out).into_owned()),
        Err(Failure::Missing) => Err("missing".into()),
        Err(Failure::Timeout) => Err("timeout".into()),
        Err(f) => Err(f.message()),
    }
}

/// The shared copy for Copy file with a cleaned text: a file with the capsule's name in a temporary folder (it
/// stays there to be pasted; the next share replaces it).
pub fn temp_copy(name: &str, text: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("Context Capsule").join("shared");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(if capsule_name(name) { name } else { "capsule.capsule.md" });
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    Ok(file)
}

fn share(host: &dyn Host, file: &Path, how: ShareHow, text: Option<&str>) -> Result<Value, String> {
    let name = file.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    match how {
        ShareHow::Reveal => host.reveal(file).map(|_| json!({ "done": true })),
        ShareHow::CopyText => {
            let text = match text {
                Some(t) => t.to_string(),
                None => std::fs::read_to_string(file).map_err(|_| "Couldn't open the capsule.".to_string())?,
            };
            host.clipboard_write(&text).map(|_| json!({ "done": true }))
        }
        ShareHow::CopyFile => {
            let path = match text {
                Some(t) => temp_copy(&name, t)?,
                None => file.to_path_buf(),
            };
            host.copy_file(&path).map(|_| json!({ "done": true }))
        }
        ShareHow::SaveCopy => {
            let Some(dest) = host.save_panel(&name) else { return Ok(json!({ "done": false })) };
            match text {
                Some(t) => std::fs::write(&dest, t),
                None => std::fs::copy(file, &dest).map(|_| ()),
            }
            .map_err(|e| format!("Couldn't save the copy: {e}"))?;
            let folder = dest.parent().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            Ok(json!({ "done": true, "where": folder }))
        }
    }
}

// MARK: dispatch

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn need<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    str_arg(args, key).ok_or_else(|| format!("no {key}"))
}

/// One method (BRIDGE.md). Blocks (files, programs, dialogs): call off the UI thread.
pub fn dispatch(paths: &Paths, host: &dyn Host, method: &str, args: &Value) -> Result<Value, String> {
    let empty = || Ok(json!({}));
    let changed = |what: &str| host.emit("data-changed", json!({ "what": [what] }));
    match method {
        "host.hello" => Ok(host.hello().json()),
        "data.list" => list(paths, need(args, "dir")?).map(|v| json!(v)),
        "data.read" => {
            let files: Vec<String> = args
                .get("files")
                .and_then(Value::as_array)
                .ok_or("files must be a list")?
                .iter()
                .map(|f| f.as_str().map(String::from).ok_or("files must be a list"))
                .collect::<Result<_, _>>()?;
            read(paths, &files).map(Value::Object)
        }
        "config.set" => {
            let key = need(args, "key")?;
            let value = args.get("value").cloned().unwrap_or(Value::Null);
            set_config(paths, key, &value)?;
            host.config_changed(key, &value);
            changed("config");
            empty()
        }
        "projects.save" => {
            save_projects(paths, need(args, "text")?)?;
            host.projects_changed();
            changed("projects");
            empty()
        }
        "timeline.delete" => {
            let session = need(args, "session")?;
            match args.get("lines") {
                None | Some(Value::Null) => delete_session(paths, session)?,
                Some(Value::Array(list)) => {
                    let lines: Vec<(String, String)> = list
                        .iter()
                        .map(|l| match (str_arg(l, "t"), str_arg(l, "line")) {
                            (Some(t), Some(line)) => Ok((t.to_string(), line.to_string())),
                            _ => Err("gone".to_string()),
                        })
                        .collect::<Result<_, _>>()?;
                    delete_lines(paths, session, &lines)?
                }
                Some(_) => return Err("lines must be a list".into()),
            }
            changed("sessions");
            empty()
        }
        "recorder" => {
            let action = need(args, "action")?;
            if !["toggle", "voice", "add-screen", "new-project", "project"].contains(&action) {
                return Err(format!("unknown action {action}"));
            }
            host.recorder(action, str_arg(args, "id")).and_then(|_| empty())
        }
        "shortcuts.set" => {
            let parse = |k: &str| str_arg(args, k).and_then(Shortcut::parse).filter(|s| s.problem(false).is_none());
            let (Some(record), Some(voice)) = (parse("record"), parse("voice")) else { return Err("Those keys can't be a shortcut.".into()) };
            if record == voice {
                return Err("Those keys can't be a shortcut.".into());
            }
            let keys = Hotkeys { record, voice };
            host.set_hotkeys(keys)?;
            keys.save(&paths.config()).map_err(|e| e.to_string())?;
            changed("config");
            empty()
        }
        "shortcuts.listen" => {
            host.listen_keys(args.get("on").and_then(Value::as_bool).unwrap_or(false));
            empty()
        }
        "setup.status" => {
            let steps: Vec<Value> = host.setup_status().into_iter().map(|(id, done)| json!({ "id": id, "done": done })).collect();
            Ok(json!({ "steps": steps, "screenRecording": true }))
        }
        "setup.do" => host.setup_do(need(args, "id")?).and_then(|_| empty()),
        "mcp.run" => {
            let command = need(args, "command")?;
            let out = run_mcp(host.mcp(), command, str_arg(args, "id"), str_arg(args, "date"), str_arg(args, "input"))?;
            if matches!(command, "connect" | "disconnect") {
                changed("clients");
            } else if command == "save" {
                changed("capsules");
            }
            Ok(Value::String(out))
        }
        "ai.open" => {
            let id = need(args, "id")?;
            let prompt = str_arg(args, "prompt").unwrap_or("");
            host.ai_open(id, prompt).map(|opened| json!({ "opened": opened }))
        }
        "clipboard.read" => host.clipboard_read().map(|text| json!({ "text": text })),
        "clipboard.write" => host.clipboard_write(str_arg(args, "text").unwrap_or("")).and_then(|_| empty()),
        "open.url" => {
            let url = need(args, "url")?;
            let lower = url.to_ascii_lowercase();
            if !(lower.starts_with("https://") || lower.starts_with("http://")) || url.chars().any(char::is_control) {
                return Err("only http(s) links".into());
            }
            host.open_url(url).and_then(|_| empty())
        }
        "capsule.open" => {
            let file = need(args, "file")?;
            if !file.starts_with("capsules/") {
                return Err(format!("not allowed: {file}"));
            }
            let path = resolve(paths, file)?;
            if !path.is_file() {
                return Err("That capsule isn't there any more.".into());
            }
            host.open_file(&path).and_then(|_| empty())
        }
        "capsule.share" => {
            let file = need(args, "file")?;
            if !file.starts_with("capsules/") {
                return Err(format!("not allowed: {file}"));
            }
            let path = resolve(paths, file)?;
            if !path.is_file() {
                return Err("That capsule isn't there any more.".into());
            }
            let how = match need(args, "how")? {
                "copy-file" => ShareHow::CopyFile,
                "copy-text" => ShareHow::CopyText,
                "save-copy" => ShareHow::SaveCopy,
                "reveal" => ShareHow::Reveal,
                other => return Err(format!("can't share with {other} here")),
            };
            share(host, &path, how, str_arg(args, "text"))
        }
        "capsule.pick" => match host.pick_capsule() {
            None => Ok(Value::Null),
            Some(path) => {
                let text = std::fs::read(&path).map_err(|_| crate::claude::CANT_READ.to_string())?;
                Ok(json!({ "path": path.to_string_lossy(), "text": String::from_utf8_lossy(&text) }))
            }
        },
        "apps.pick" => Ok(host.pick_app().map_or(Value::Null, |(id, name)| json!({ "id": id, "name": name }))),
        "apps.names" => {
            let ids: Vec<String> =
                args.get("ids").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
            Ok(Value::Object(host.app_names(&ids)))
        }
        "apps.icons" => {
            let apps = crate::app_icons::parse(args);
            Ok(Value::Object(crate::app_icons::shared().answer(&apps, &|app, bundle| host.app_icon(app, bundle))))
        }
        "updates" => {
            let action = need(args, "action")?;
            if action != "check" && action != "restart" {
                return Err(format!("unknown action {action}"));
            }
            host.updates(action).and_then(|_| empty())
        }
        _ => Err(format!("unknown method {method}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
        events: Mutex<Vec<(String, Value)>>,
        clipboard: Mutex<String>,
        taken: bool,
        save_to: Option<PathBuf>,
    }

    impl Fake {
        fn log(&self, s: impl Into<String>) {
            self.calls.lock().unwrap().push(s.into());
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
        fn events(&self) -> Vec<(String, Value)> {
            self.events.lock().unwrap().clone()
        }
    }

    impl Host for Fake {
        fn hello(&self) -> Hello {
            Hello { platform: "linux", locale: "en-GB".into(), user_name: "Saachi Badal".into(), ..Default::default() }
        }
        fn config_changed(&self, key: &str, value: &Value) {
            self.log(format!("config {key}={value}"));
        }
        fn projects_changed(&self) {
            self.log("projects");
        }
        fn recorder(&self, action: &str, id: Option<&str>) -> Result<(), String> {
            self.log(format!("recorder {action} {id:?}"));
            Ok(())
        }
        fn set_hotkeys(&self, keys: Hotkeys) -> Result<(), String> {
            if self.taken {
                return Err("taken".into());
            }
            self.log(format!("hotkeys {} {}", keys.record, keys.voice));
            Ok(())
        }
        fn listen_keys(&self, on: bool) {
            self.log(format!("listen {on}"));
        }
        fn setup_status(&self) -> Vec<(&'static str, bool)> {
            vec![("voice", true), ("ai", false), ("login", true)]
        }
        fn setup_do(&self, id: &str) -> Result<(), String> {
            self.log(format!("setup {id}"));
            Ok(())
        }
        fn mcp(&self) -> Option<Launch> {
            None
        }
        fn ai_open(&self, id: &str, _prompt: &str) -> Result<bool, String> {
            Ok(id == "claude")
        }
        fn clipboard_read(&self) -> Result<String, String> {
            Ok(self.clipboard.lock().unwrap().clone())
        }
        fn clipboard_write(&self, text: &str) -> Result<(), String> {
            *self.clipboard.lock().unwrap() = text.to_string();
            Ok(())
        }
        fn copy_file(&self, path: &Path) -> Result<(), String> {
            self.log(format!("copy-file {}", std::fs::read_to_string(path).unwrap_or_default()));
            Ok(())
        }
        fn open_url(&self, url: &str) -> Result<(), String> {
            self.log(format!("url {url}"));
            Ok(())
        }
        fn open_file(&self, path: &Path) -> Result<(), String> {
            self.log(format!("open {}", path.file_name().unwrap().to_string_lossy()));
            Ok(())
        }
        fn reveal(&self, _path: &Path) -> Result<(), String> {
            self.log("reveal");
            Ok(())
        }
        fn save_panel(&self, _name: &str) -> Option<PathBuf> {
            self.save_to.clone()
        }
        fn pick_capsule(&self) -> Option<PathBuf> {
            None
        }
        fn pick_app(&self) -> Option<(String, String)> {
            Some(("firefox".into(), "Firefox".into()))
        }
        fn app_names(&self, ids: &[String]) -> Map<String, Value> {
            ids.iter().map(|i| (i.clone(), Value::String(i.to_uppercase()))).collect()
        }
        fn app_icon(&self, app: &str, bundle: &str) -> Option<Vec<u8>> {
            self.log(format!("icon {app}|{bundle}"));
            (bundle == "bridge-test.exe").then(|| vec![0x89, b'P'])
        }
        fn updates(&self, action: &str) -> Result<(), String> {
            self.log(format!("updates {action}"));
            Ok(())
        }
        fn emit(&self, event: &str, payload: Value) {
            self.events.lock().unwrap().push((event.into(), payload));
        }
    }

    fn folder(name: &str) -> Paths {
        let root = std::env::temp_dir().join(format!("cc-bridge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sessions/20261023-112957")).unwrap();
        std::fs::create_dir_all(root.join("capsules/.history")).unwrap();
        std::fs::write(root.join("config.json"), "{\n  \"ai\" : \"claude\",\n  \"sounds\" : false\n}\n").unwrap();
        std::fs::write(root.join("capsules/website.capsule.md"), "---\ntitle: Website\n---\nHi\n").unwrap();
        std::fs::write(root.join("capsules/.hidden.md"), "x").unwrap();
        Paths::new(root)
    }

    fn call(paths: &Paths, host: &Fake, method: &str, args: Value) -> Result<Value, String> {
        dispatch(paths, host, method, &args)
    }

    #[test]
    fn allow_list() {
        for ok in [
            "config.json",
            "projects.json",
            "state.json",
            "sessions/20261023-112957/events.jsonl",
            "sessions/20261023-112957/gaps.jsonl",
            "sessions/20261023-112957/assign.json",
            "sessions/20261023-112957/session.json",
            "capsules/website.capsule.md",
            "capsules/Pricing page — notes.md",
        ] {
            assert!(allowed(ok), "{ok}");
        }
        for no in [
            "",
            "/etc/passwd",
            "C:/Windows/win.ini",
            "../config.json",
            "./config.json",
            "app.log",
            "control",
            "sessions/../config.json",
            "sessions/20261023-112957/../../x/events.jsonl",
            "sessions//events.jsonl",
            "sessions/20261023-112957/notes.txt",
            "sessions/20261023-112957/events.jsonl/x",
            "sessions\\20261023-112957\\events.jsonl",
            "sessions/a\\b/events.jsonl",
            "sessions/.x/events.jsonl",
            "capsules/x.txt",
            "capsules/.history",
            "capsules/.md",
            "capsules/a/b.md",
            "capsules/x.md:stream",
            "capsules/CON.md",
            "capsules/com1.md",
            "capsules/x.md.",
            "capsules/new\nline.md",
            "config.json/",
            "CONFIG.JSON",
        ] {
            assert!(!allowed(no), "{no:?}");
        }
        assert!(capsule_name("Console.md"), "only the exact device names");
    }

    #[test]
    fn reading_and_listing() {
        let p = folder("read");
        let host = Fake::default();
        assert_eq!(call(&p, &host, "data.list", json!({"dir": "sessions"})).unwrap(), json!(["20261023-112957"]));
        assert_eq!(call(&p, &host, "data.list", json!({"dir": "capsules"})).unwrap(), json!(["website.capsule.md"]), "hidden files and .history left out");
        assert!(call(&p, &host, "data.list", json!({"dir": ".."})).is_err());
        let r = call(&p, &host, "data.read", json!({"files": ["config.json", "projects.json", "capsules/website.capsule.md"]})).unwrap();
        assert_eq!(r["config.json"].as_str().unwrap(), "{\n  \"ai\" : \"claude\",\n  \"sounds\" : false\n}\n");
        assert_eq!(r["projects.json"], Value::Null, "missing: null");
        assert!(r["capsules/website.capsule.md"].as_str().unwrap().contains("Hi"));
        let bad = call(&p, &host, "data.read", json!({"files": ["config.json", "../secret"]}));
        assert_eq!(bad, Err("not allowed: ../secret".into()), "one bad path: nothing is read");
        assert!(call(&p, &host, "data.read", json!({"files": "config.json"})).is_err());
        assert!(call(&p, &host, "data.read", json!({"files": [1]})).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn links_out_of_the_folder_are_refused() {
        let p = folder("link");
        let outside = p.root.with_extension("outside");
        std::fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, p.capsules().join("leak.md")).unwrap();
        assert!(read(&p, &["capsules/leak.md".into()]).is_err());
        let _ = std::fs::remove_file(outside);
    }

    #[test]
    fn config_and_projects() {
        let p = folder("config");
        let host = Fake::default();
        call(&p, &host, "config.set", json!({"key": "appearance", "value": "dark"})).unwrap();
        let c: Value = serde_json::from_str(&std::fs::read_to_string(p.config()).unwrap()).unwrap();
        assert_eq!(c, json!({"ai": "claude", "appearance": "dark", "sounds": false}));
        call(&p, &host, "config.set", json!({"key": "sounds", "value": null})).unwrap();
        let c: Value = serde_json::from_str(&std::fs::read_to_string(p.config()).unwrap()).unwrap();
        assert_eq!(c, json!({"ai": "claude", "appearance": "dark"}), "null removes it");
        assert!(call(&p, &host, "config.set", json!({"key": "hotkeys", "value": {}})).is_err(), "not through config.set");
        assert!(call(&p, &host, "config.set", json!({"key": "pid", "value": 1})).is_err());
        assert_eq!(host.calls(), ["config appearance=\"dark\"", "config sounds=null"]);
        assert_eq!(host.events()[0], ("data-changed".into(), json!({"what": ["config"]})));

        let text = "{\n  \"projects\" : [\n    {\n      \"id\" : \"web\",\n      \"name\" : \"Website\"\n    }\n  ]\n}\n";
        call(&p, &host, "projects.save", json!({ "text": text })).unwrap();
        assert_eq!(std::fs::read_to_string(p.projects()).unwrap(), text);
        assert!(call(&p, &host, "projects.save", json!({"text": "nope"})).is_err());
        assert!(call(&p, &host, "projects.save", json!({"text": "[]"})).is_err());
        assert!(host.calls().contains(&"projects".to_string()));
    }

    #[test]
    fn deleting_lines() {
        let p = folder("delete");
        let host = Fake::default();
        let id = "20261023-112957";
        let a = r#"{"t":"2026-10-23T06:59:57Z","kind":"read","text":"A"}"#;
        let b = r#"{"t":"2026-10-23T07:00:05Z","kind":"read","text":"B"}"#;
        let file = p.session_dir(id).join("events.jsonl");
        std::fs::write(&file, format!("{a}\r\n{b}\r\n{a}\r\n")).unwrap();
        let one = |t: &str, line: &str| json!({ "t": t, "line": line });
        call(&p, &host, "timeline.delete", json!({ "session": id, "lines": [one("2026-10-23T06:59:57Z", a)] })).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), format!("{b}\r\n{a}\r\n"), "the first match only, CRLF kept");
        let wrong_t = call(&p, &host, "timeline.delete", json!({ "session": id, "lines": [one("2026-10-23T09:00:00Z", b)] }));
        assert_eq!(wrong_t, Err("gone".into()));
        let half = call(&p, &host, "timeline.delete", json!({ "session": id, "lines": [one("2026-10-23T07:00:05Z", b), one("2026-10-23T07:00:05Z", b)] }));
        assert_eq!(half, Err("gone".into()), "all or none");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), format!("{b}\r\n{a}\r\n"));
        // Recording that session: refused.
        std::fs::write(p.state(), format!(r#"{{"recording":true,"session_id":"{id}"}}"#)).unwrap();
        assert_eq!(call(&p, &host, "timeline.delete", json!({ "session": id, "lines": [one("2026-10-23T07:00:05Z", b)] })), Err("recording".into()));
        assert_eq!(call(&p, &host, "timeline.delete", json!({ "session": id })), Err("recording".into()));
        std::fs::write(p.state(), r#"{"recording":false}"#).unwrap();
        assert_eq!(call(&p, &host, "timeline.delete", json!({ "session": "../x" })), Err("gone".into()));
        assert_eq!(call(&p, &host, "timeline.delete", json!({ "session": "../../x", "lines": [one("t", "x")] })), Err("gone".into()));
        call(&p, &host, "timeline.delete", json!({ "session": id })).unwrap();
        assert!(!p.session_dir(id).exists());
        assert_eq!(call(&p, &host, "timeline.delete", json!({ "session": id })), Err("gone".into()));
        assert!(host.events().iter().all(|(e, v)| e == "data-changed" && v == &json!({"what": ["sessions"]})));
    }

    #[test]
    fn sharing() {
        let p = folder("share");
        let dest = p.root.join("out.md");
        let host = Fake { save_to: Some(dest.clone()), ..Default::default() };
        let file = "capsules/website.capsule.md";
        call(&p, &host, "capsule.share", json!({ "file": file, "how": "copy-text" })).unwrap();
        assert!(host.clipboard.lock().unwrap().contains("Hi"));
        call(&p, &host, "capsule.share", json!({ "file": file, "how": "copy-text", "text": "clean" })).unwrap();
        assert_eq!(*host.clipboard.lock().unwrap(), "clean");
        call(&p, &host, "capsule.share", json!({ "file": file, "how": "copy-file", "text": "cleaned copy" })).unwrap();
        assert!(host.calls().contains(&"copy-file cleaned copy".to_string()), "a temporary copy with the cleaned text");
        let saved = call(&p, &host, "capsule.share", json!({ "file": file, "how": "save-copy" })).unwrap();
        assert_eq!(saved["done"], true);
        assert!(std::fs::read_to_string(&dest).unwrap().contains("Hi"));
        let cancelled = Fake::default();
        assert_eq!(call(&p, &cancelled, "capsule.share", json!({ "file": file, "how": "save-copy" })).unwrap(), json!({"done": false}));
        assert!(call(&p, &host, "capsule.share", json!({ "file": file, "how": "more" })).is_err());
        assert!(call(&p, &host, "capsule.share", json!({ "file": "config.json", "how": "reveal" })).is_err());
        assert!(call(&p, &host, "capsule.share", json!({ "file": "capsules/gone.md", "how": "reveal" })).is_err());
        call(&p, &host, "capsule.open", json!({ "file": file })).unwrap();
        assert!(call(&p, &host, "capsule.open", json!({ "file": "sessions/20261023-112957/events.jsonl" })).is_err());
        assert!(host.calls().contains(&"open website.capsule.md".to_string()));
    }

    #[test]
    fn host_methods() {
        let p = folder("host");
        let host = Fake::default();
        let hello = call(&p, &host, "host.hello", json!({})).unwrap();
        assert_eq!(hello["platform"], "linux");
        assert_eq!(hello["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(hello["hour12"], Value::Null);
        assert_eq!(hello["material"], "none");
        assert_eq!(hello["voiceLanguageDefault"], "Same as my computer");
        assert!(hello["voiceLanguages"].as_array().unwrap().iter().all(|l| l["id"].as_str().is_some_and(|i| !i.is_empty())));
        call(&p, &host, "recorder", json!({"action": "project", "id": "web"})).unwrap();
        call(&p, &host, "recorder", json!({"action": "project", "id": null})).unwrap();
        assert!(call(&p, &host, "recorder", json!({"action": "explode"})).is_err());
        call(&p, &host, "shortcuts.set", json!({"record": "Ctrl+Alt+R", "voice": "Alt+V"})).unwrap();
        assert_eq!(Hotkeys::load(&p.config()).record.label(), "Ctrl+Alt+R");
        assert!(call(&p, &host, "shortcuts.set", json!({"record": "Alt+V", "voice": "Alt+V"})).is_err());
        assert!(call(&p, &host, "shortcuts.set", json!({"record": "Q", "voice": "Alt+V"})).is_err());
        let taken = Fake { taken: true, ..Default::default() };
        assert_eq!(call(&p, &taken, "shortcuts.set", json!({"record": "Alt+K", "voice": "Alt+V"})), Err("taken".into()));
        assert_eq!(Hotkeys::load(&p.config()).record.label(), "Ctrl+Alt+R", "kept");
        let setup = call(&p, &host, "setup.status", json!({})).unwrap();
        assert_eq!(setup["steps"][1], json!({"id": "ai", "done": false}));
        assert_eq!(call(&p, &host, "mcp.run", json!({"command": "clients"})), Err("missing".into()));
        assert!(call(&p, &host, "mcp.run", json!({"command": "rm"})).is_err());
        assert!(call(&p, &host, "mcp.run", json!({"command": "connect", "id": "--evil"})).is_err());
        assert!(call(&p, &host, "mcp.run", json!({"command": "day", "date": "--help"})).is_err());
        assert_eq!(call(&p, &host, "ai.open", json!({"id": "claude", "prompt": "x"})).unwrap(), json!({"opened": true}));
        call(&p, &host, "clipboard.write", json!({"text": "hello"})).unwrap();
        assert_eq!(call(&p, &host, "clipboard.read", json!({})).unwrap(), json!({"text": "hello"}));
        call(&p, &host, "open.url", json!({"url": "https://chatgpt.com"})).unwrap();
        for bad in ["file:///etc/passwd", "javascript:alert(1)", "ms-settings:privacy", "https://x\n.com"] {
            assert!(call(&p, &host, "open.url", json!({ "url": bad })).is_err(), "{bad}");
        }
        assert_eq!(call(&p, &host, "capsule.pick", json!({})).unwrap(), Value::Null);
        assert_eq!(call(&p, &host, "apps.pick", json!({})).unwrap(), json!({"id": "firefox", "name": "Firefox"}));
        assert_eq!(call(&p, &host, "apps.names", json!({"ids": ["slack"]})).unwrap(), json!({"slack": "SLACK"}));
        let apps = json!({"apps": [{"app": "Bridge Test", "bundle": "bridge-test.exe"}, {"app": "Bridge Nothing", "bundle": ""}]});
        let icons = call(&p, &host, "apps.icons", apps.clone()).unwrap();
        assert_eq!(icons, json!({"bridge-test.exe": "data:image/png;base64,iVA=", "Bridge Nothing": null}));
        assert_eq!(call(&p, &host, "apps.icons", apps).unwrap(), icons);
        assert_eq!(host.calls().iter().filter(|c| c.starts_with("icon ")).count(), 2, "cached: one lookup per app");
        assert_eq!(call(&p, &host, "apps.icons", json!({})).unwrap(), json!({}));
        call(&p, &host, "updates", json!({"action": "check"})).unwrap();
        assert!(call(&p, &host, "updates", json!({"action": "rm"})).is_err());
        assert!(call(&p, &host, "nope", json!({})).is_err());
        assert_eq!(
            host.calls().iter().filter(|c| c.starts_with("recorder") || c.starts_with("hotkeys") || c.starts_with("url") || c.starts_with("updates")).cloned().collect::<Vec<_>>(),
            ["recorder project Some(\"web\")", "recorder project None", "hotkeys Ctrl+Alt+R Alt+V", "url https://chatgpt.com", "updates check"]
        );
    }

    #[test]
    fn messages() {
        let p = folder("msg");
        let host = Fake::default();
        let r = Request::parse(r#"{"id":7,"method":"data.list","args":{"dir":"sessions"}}"#).unwrap();
        assert_eq!((r.id.clone(), r.method.as_str()), (json!(7), "data.list"));
        assert_eq!(Request::parse(r#""{\"id\":1,\"method\":\"host.hello\"}""#).unwrap().args, json!({}), "a JSON string holding the request");
        assert_eq!(Request::parse("[]"), None);
        assert_eq!(Request::parse(r#"{"id":1}"#), None);
        let out: Value = serde_json::from_str(&handle(&p, &host, r#"{"id":3,"method":"data.list","args":{"dir":"capsules"}}"#).unwrap()).unwrap();
        assert_eq!(out, json!({"id": 3, "ok": true, "value": ["website.capsule.md"]}));
        let out: Value = serde_json::from_str(&handle(&p, &host, r#"{"id":4,"method":"data.read","args":{"files":["/etc/passwd"]}}"#).unwrap()).unwrap();
        assert_eq!(out, json!({"id": 4, "ok": false, "error": "not allowed: /etc/passwd"}));
        assert_eq!(event_json("navigate", json!({"page": "settings"})), r#"{"event":"navigate","payload":{"page":"settings"}}"#);
    }

    #[test]
    fn page_files_and_script() {
        let (html, kind) = asset("/").unwrap();
        assert!(kind.starts_with("text/html") && std::str::from_utf8(html).unwrap().contains("js/app.js"));
        assert!(asset("/js/app.js").is_some_and(|(_, k)| k.starts_with("text/javascript")));
        assert!(asset("/js/core/timeline.js?x=1").is_some());
        assert!(asset("/css/app.css").is_some_and(|(_, k)| k.starts_with("text/css")));
        assert!(asset("/dev/shim.js").is_none(), "the dev host isn't shipped");
        assert!(asset("/tests/smoke.test.js").is_none());
        assert!(asset("/../Cargo.toml").is_none());
        assert!(asset("/BRIDGE.md").is_none());
        for t in [Transport::WebView2, Transport::Ipc, Transport::WebKit] {
            let js = shim_js(t);
            assert!(!js.contains('%'), "placeholders filled");
            assert!(js.contains("capsuleHost") && js.contains("_receive"));
        }
        assert!(shim_js(Transport::WebView2).contains("chrome.webview.postMessage"));
        assert!(shim_js(Transport::WebKit).contains("messageHandlers.capsule"));
    }

    #[test]
    fn state_payload() {
        let s = Status { recording: true, sources: 1, session_id: Some("20261023-112957".into()), ..Default::default() };
        let v = state_json(&s, true, "", false, false);
        assert_eq!(v["recording"], true);
        assert_eq!(v["can_add_screen"], true);
        assert_eq!(v["screens"], 1);
        assert_eq!(v["several_screens"], true);
        assert_eq!(v["update"], json!({"line": "", "ready": false, "working": false}));
        assert_eq!(state_json(&Status::default(), false, "", false, false)["started_at"], Value::Null);
    }
}
