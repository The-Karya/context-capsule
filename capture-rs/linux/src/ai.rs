// context-capsule — linux: any AI (SPEC "Any AI") — which AI apps there are and which one is "Your AI", kept in
// one place that the window and the tray both read; the program's commands (clients, connect, disconnect, day,
// save), run on worker threads; and, without the window (tray only), Make today's capsules and Save from
// clipboard with the clipboard tools (wl-copy / wl-paste, xclip, xsel) and plain dialogs.
//! The words and the program's answers live in core (`anyai`). Every function here that runs the program blocks:
//! call it off the GTK thread and off the tray's.
#![cfg_attr(not(feature = "window"), allow(dead_code))]

use crate::link::WindowLink;
use crate::notify::Notifier;
use crate::{connect, program, reminder};
use context_capsule_core::anyai::{self, AiClient, ConnectAnswer, DayAnswer, Failure, Launch, SaveAnswer, YourAi};
use context_capsule_core::reminder::DAY_PROMPT;
use context_capsule_core::settings::ConfigFile;
use context_capsule_core::window::CANCEL;
use context_capsule_core::{Msg, Paths, Status};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Make today's capsules, for an app that isn't connected yet.
pub const WHY_DAY: &str = "to read your day";
pub const AGAIN_DAY: &str = "click Make today's capsules again";
/// Ask your AI (Capsules search).
pub const WHY_SEARCH: &str = "to search your capsules";
pub const AGAIN_SEARCH: &str = "click Ask again";
/// A capsule someone sent.
pub const WHY_CAPSULE: &str = "to read the capsule";
pub const AGAIN_CAPSULE: &str = "open the capsule again";
/// Timeline → Make a capsule.
pub const WHY_SESSION: &str = "to read the recording";
pub const AGAIN_SESSION: &str = "click Make a capsule again";
pub const TRY_AGAIN: &str = "Please try again.";
/// Tray only, and no clipboard tool is installed.
pub const NO_CLIPBOARD_TOOL: &str = "To use the clipboard here, install wl-clipboard (or xclip), or open Context Capsule's window.";

/// Everything about Your AI the window and the tray show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiState {
    /// From `clients`, installed first. Without the program: Claude only (as the app detects it).
    pub clients: Vec<AiClient>,
    /// The list was read at least once.
    pub loaded: bool,
    /// Your AI: an id from `clients`, a website id, or "any".
    pub choice: String,
    /// The app being connected or disconnected now.
    pub busy: Option<String>,
    /// A short line under the list after connecting.
    pub note: Option<String>,
    /// Shown after Copy my day, until a save works or the choice changes.
    pub day_line: Option<String>,
    /// Copy my day or Save from clipboard is running.
    pub working: bool,
    /// Counts the saves that saved something (the window reloads its lists when it changes).
    pub saves: u64,
    refreshing: bool,
}

impl AiState {
    const fn new() -> AiState {
        AiState {
            clients: Vec::new(),
            loaded: false,
            choice: String::new(),
            busy: None,
            note: None,
            day_line: None,
            working: false,
            saves: 0,
            refreshing: false,
        }
    }

    pub fn kind(&self) -> YourAi {
        YourAi::of(&self.choice)
    }

    /// "Claude", "Cursor", "ChatGPT", "your AI".
    pub fn name(&self) -> String {
        self.kind().name(&self.clients)
    }

    pub fn client(&self, id: &str) -> Option<&AiClient> {
        self.clients.iter().find(|c| c.id == id)
    }

    /// At least one AI is connected (Claude counts as the app detects it: merged in by `refresh`).
    pub fn any_connected(&self) -> bool {
        self.clients.iter().any(|c| c.connected)
    }
}

type Listener = Arc<dyn Fn() + Send + Sync>;

static STATE: Mutex<AiState> = Mutex::new(AiState::new());
static CONFIG: Mutex<Option<PathBuf>> = Mutex::new(None);
static LISTENERS: Mutex<Vec<Listener>> = Mutex::new(Vec::new());

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn log(line: &str) {
    eprintln!("Context Capsule: {line}");
}

/// Where config.json is, and Your AI as saved there (before the list is read).
pub fn init(config: PathBuf) {
    let saved = anyai::saved_choice(&config);
    *lock(&CONFIG) = Some(config);
    update(|s| s.choice = anyai::resolve(saved.as_deref(), &[]));
}

fn config() -> PathBuf {
    lock(&CONFIG).clone().unwrap_or_else(|| Paths::standard().config())
}

/// A copy of the state, as of now.
pub fn state() -> AiState {
    lock(&STATE).clone()
}

/// `f` runs (on the thread that changed it) every time the state changes.
pub fn on_change(f: impl Fn() + Send + Sync + 'static) {
    lock(&LISTENERS).push(Arc::new(f));
}

/// Change the state; the listeners hear of it when something changed.
pub fn update(f: impl FnOnce(&mut AiState)) {
    let changed = {
        let mut s = lock(&STATE);
        let before = s.clone();
        f(&mut s);
        *s != before
    };
    if changed {
        notify_all();
    }
}

/// How to run the program (found once per run; from an AppImage, copied to its stable place first).
pub fn launch() -> Option<Launch> {
    program::launch(&log)
}

fn run<T>(f: impl FnOnce(&Launch) -> Result<T, Failure>) -> Result<T, Failure> {
    launch().ok_or(Failure::Missing).and_then(|l| f(&l))
}

/// The program's list with Claude as the app detects it: Claude is always there, installed or connected when
/// either says so (its extension or Claude Code's settings are seen at once, before the program looks again).
pub fn with_claude(found: Option<Vec<AiClient>>, installed: bool, connected: bool) -> Vec<AiClient> {
    let mut list = found.unwrap_or_default();
    match list.iter_mut().find(|c| c.id == anyai::CLAUDE_ID) {
        Some(c) => {
            c.installed |= installed || connected;
            c.connected |= connected;
        }
        None => list.push(AiClient::claude(installed || connected, connected)),
    }
    anyai::sorted(list)
}

/// Ask the program which AI apps there are (blocks). false: a refresh is already running.
pub fn refresh() -> bool {
    {
        let mut s = lock(&STATE);
        if s.refreshing {
            return false;
        }
        s.refreshing = true;
    }
    let env = |k: &str| std::env::var(k).ok();
    let found = match run(anyai::clients) {
        Ok(list) => Some(list),
        Err(f) => {
            log(&format!("any AI: clients: {}", f.message()));
            None
        }
    };
    let list = with_claude(found, connect::claude_present(&env), connect::is_connected(&env));
    let saved = anyai::saved_choice(&config());
    update(|s| {
        s.refreshing = false;
        s.loaded = true;
        s.choice = anyai::resolve(saved.as_deref(), &list);
        s.clients = list;
    });
    true
}

/// The list, read once if it wasn't yet (blocks; waits for a refresh already running, at most `LOAD_WAIT`).
pub fn ensure_loaded() {
    let start = Instant::now();
    while !state().loaded && start.elapsed() < LOAD_WAIT {
        if !refresh() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// How long a request waits for the list (`clients` itself is stopped after 10 s).
const LOAD_WAIT: Duration = Duration::from_secs(15);

/// Settings → Your AI: saved in config.json at once. false: it couldn't be saved.
pub fn set_choice(id: &str) -> bool {
    let saved = match ConfigFile::set(&config(), anyai::KEY_AI, Some(id.into())) {
        Ok(()) => true,
        Err(e) => {
            log(&format!("cannot save ai: {e}"));
            false
        }
    };
    log(&format!("your AI: {id}"));
    update(|s| {
        s.choice = id.to_string();
        s.day_line = None;
    });
    saved
}

/// What to tell the user after connecting or disconnecting: a note under the list, or a message to dismiss.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Told {
    Note(String),
    Alert(String, String),
}

fn or_try_again(message: &str) -> String {
    if message.trim().is_empty() {
        TRY_AGAIN.into()
    } else {
        message.trim().to_string()
    }
}

pub fn connect_told(name: &str, r: Result<ConnectAnswer, Failure>) -> Told {
    match r {
        Ok(a) if a.ok => Told::Note(if a.message.trim().is_empty() { anyai::connected_note(name) } else { a.message }),
        Ok(a) => Told::Alert(anyai::connect_failed(name), or_try_again(&a.message)),
        Err(f) => Told::Alert(anyai::connect_failed(name), f.message()),
    }
}

pub fn disconnect_told(name: &str, r: Result<ConnectAnswer, Failure>) -> Told {
    match r {
        Ok(a) if a.ok => Told::Note(if a.message.trim().is_empty() { anyai::disconnected_note(name) } else { a.message }),
        Ok(a) if a.how.as_deref() == Some("extension") => {
            Told::Alert(anyai::REMOVE_IN_CLAUDE.into(), anyai::REMOVE_IN_CLAUDE_BODY.into())
        }
        Ok(a) => Told::Alert(anyai::disconnect_failed(name), or_try_again(&a.message)),
        Err(f) => Told::Alert(anyai::disconnect_failed(name), f.message()),
    }
}

/// Connect an app through its MCP settings (not Claude: that's the .mcpb flow). Blocks. The first app connected
/// becomes Your AI if none was chosen. Then the list is read again.
pub fn connect(c: &AiClient) -> Told {
    log(&format!("connect: {}", c.id));
    let before = state().clients;
    let saved = anyai::saved_choice(&config());
    update(|s| {
        s.busy = Some(c.id.clone());
        s.note = None;
    });
    let told = connect_told(&c.name, run(|l| anyai::connect(l, &c.id)));
    if matches!(told, Told::Note(_)) && anyai::first_connected_becomes_choice(saved.as_deref(), &before) {
        set_choice(&c.id);
    }
    finish_connect(&told);
    told
}

pub fn disconnect(c: &AiClient) -> Told {
    log(&format!("disconnect: {}", c.id));
    update(|s| {
        s.busy = Some(c.id.clone());
        s.note = None;
    });
    let told = disconnect_told(&c.name, run(|l| anyai::disconnect(l, &c.id)));
    finish_connect(&told);
    told
}

fn finish_connect(told: &Told) {
    update(|s| {
        s.busy = None;
        if let Told::Note(n) = told {
            s.note = Some(n.clone());
        }
    });
    refresh();
}

/// A note under the list (Claude: "Click Install in Claude…").
pub fn set_note(note: &str) {
    update(|s| s.note = Some(note.to_string()));
}

/// Copy my day or Save from clipboard starts. false: one is already running.
pub fn begin_work() -> bool {
    let mut s = lock(&STATE);
    if s.working {
        return false;
    }
    s.working = true;
    drop(s);
    notify_all(); // the buttons go off
    true
}

pub fn end_work() {
    update(|s| s.working = false);
}

fn notify_all() {
    let listeners = lock(&LISTENERS).clone();
    for l in listeners {
        l();
    }
}

/// What Copy my day got.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Day {
    Failed(String),
    /// Nothing recorded today.
    Nothing,
    /// Copy `text`; then open `url` (Your AI is a website).
    Copy { text: String, chars: usize, url: Option<&'static str> },
}

pub fn day_outcome(r: Result<DayAnswer, Failure>, kind: &YourAi) -> Day {
    match r {
        Err(f) => Day::Failed(f.message()),
        Ok(d) if d.text.trim().is_empty() => Day::Nothing,
        Ok(d) => Day::Copy {
            text: d.text,
            chars: d.chars,
            url: match kind {
                YourAi::Website(w) => Some(w.url),
                _ => None,
            },
        },
    }
}

/// Runs `day` (blocks).
pub fn day() -> Day {
    day_outcome(run(|l| anyai::day(l, None)), &state().kind())
}

/// The day is on the clipboard: say what to do next until the reply is saved.
pub fn day_copied(chars: usize) {
    log(&format!("copy my day: {chars} characters copied"));
    update(|s| s.day_line = Some(anyai::PASTE_LINE.into()));
}

/// Runs `save` on the pasted reply (blocks): the dialog's (title, text), or why it failed.
pub fn save(text: &str) -> Result<(String, String), String> {
    let r: SaveAnswer = run(|l| anyai::save(l, text)).map_err(|f| f.message())?;
    log(&format!("save from clipboard: {} saved, {} refused", r.saved.len(), r.refused.len()));
    if !r.saved.is_empty() {
        update(|s| {
            s.day_line = None;
            s.saves += 1;
        });
    }
    Ok(anyai::summary(&r))
}

// MARK: the tray (and the evening reminder)

/// Make today's capsules from the tray or the reminder: the window does it when there is one (it stops
/// recording first, and has the clipboard and dialogs); else here, with the clipboard tools and plain dialogs.
pub fn make_todays_capsules(window: &WindowLink, notifier: &Notifier, tx: Option<&Sender<Msg>>, status: &Status) {
    if window.is_on() {
        window.make_capsules();
        return;
    }
    if status.recording && !status.listening {
        log("stop: make today's capsules");
        if let Some(tx) = tx {
            let _ = tx.send(Msg::ToggleRecord);
        }
    }
    let n = notifier.clone();
    std::thread::spawn(move || {
        ensure_loaded();
        match state().kind() {
            YourAi::Claude => reminder::claude_with(&n, DAY_PROMPT),
            YourAi::App(_) => open_alone(&n, DAY_PROMPT, WHY_DAY, AGAIN_DAY),
            YourAi::Website(_) | YourAi::Other => copy_day_alone(&n),
        }
    });
}

/// Save from clipboard from the tray: the window does it when there is one.
pub fn save_from_clipboard(window: &WindowLink, notifier: &Notifier) {
    if window.is_on() {
        window.save_from_clipboard();
        return;
    }
    let n = notifier.clone();
    std::thread::spawn(move || save_alone(&n));
}

/// A request for Your AI without the window (blocks: dialogs wait). Another app: connect it first, or the
/// request goes on the clipboard. Claude, a website or any other AI: Claude, as before.
pub fn open_alone(n: &Notifier, prompt: &str, why: &str, again: &str) {
    let s = state();
    let client = match s.kind() {
        YourAi::App(id) => s.client(&id).cloned(),
        _ => None,
    };
    let Some(c) = client else { return reminder::claude_with(n, prompt) };
    if !c.connected {
        let (title, body) = anyai::connect_first(&c.name, why, again);
        if n.ask(&title, &body, anyai::CONNECT, CANCEL) {
            match connect(&c) {
                Told::Note(note) => n.toast(&note),
                Told::Alert(t, b) => n.alert(&t, &b),
            }
        }
        return;
    }
    if reminder::copy_text(prompt) {
        n.toast(&anyai::copied_for(&c.name, false));
    } else {
        n.alert(&c.name, &format!("Open {} and type this in a new chat:\n\n{prompt}", c.name));
    }
}

fn copy_day_alone(n: &Notifier) {
    if !begin_work() {
        return;
    }
    let d = day();
    end_work();
    match d {
        Day::Failed(why) => n.alert(anyai::COPY_FAILED, &why),
        Day::Nothing => n.toast(anyai::NOTHING_TODAY),
        Day::Copy { text, chars, url } => {
            if !reminder::copy_text(&text) {
                n.alert(anyai::COPY_FAILED, NO_CLIPBOARD_TOOL);
                return;
            }
            if let Some(url) = url {
                reminder::xdg_open(url);
            }
            day_copied(chars);
            n.toast(anyai::PASTE_LINE);
        }
    }
}

fn save_alone(n: &Notifier) {
    if state().working {
        return;
    }
    let Some(text) = reminder::paste_text() else {
        n.alert(anyai::SAVE_FAILED, NO_CLIPBOARD_TOOL);
        return;
    };
    if text.trim().is_empty() {
        n.alert(anyai::CLIPBOARD_EMPTY, anyai::CLIPBOARD_EMPTY_BODY);
        return;
    }
    if !begin_work() {
        return;
    }
    let r = save(&text);
    end_work();
    match r {
        Ok((title, body)) => n.alert(&title, &body),
        Err(why) => n.alert(anyai::SAVE_FAILED, &why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(id: &str, installed: bool, connected: bool) -> AiClient {
        AiClient { id: id.into(), name: id.to_uppercase(), installed, connected, how: "config".into() }
    }

    #[test]
    fn claude_is_always_listed() {
        let none = with_claude(None, false, false);
        assert_eq!(none, [AiClient::claude(false, false)], "no program: Claude only");
        assert_eq!(with_claude(None, false, true), [AiClient::claude(true, true)], "connected means installed");
        let found = vec![client("cursor", true, true), AiClient::claude(false, false)];
        let list = with_claude(Some(found), true, true);
        assert_eq!(list[0], AiClient::claude(true, true), "the app's own detection wins, Claude first");
        assert_eq!(list[1].id, "cursor");
        let list = with_claude(Some(vec![client("zed", true, false)]), false, false);
        assert_eq!(list.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["zed", "claude"]);
    }

    #[test]
    fn what_to_say() {
        let ok = |m: &str| Ok(ConnectAnswer { ok: true, message: m.into(), how: None });
        let no = |m: &str, how: Option<&str>| Ok(ConnectAnswer { ok: false, message: m.into(), how: how.map(String::from) });
        assert_eq!(connect_told("Cursor", ok("")), Told::Note("Connected to Cursor. Restart Cursor to use it.".into()));
        assert_eq!(connect_told("Cursor", ok("Done.")), Told::Note("Done.".into()));
        assert_eq!(connect_told("Cursor", no(" ", None)), Told::Alert("Couldn't connect Cursor".into(), TRY_AGAIN.into()));
        assert_eq!(
            connect_told("Cursor", Err(Failure::Missing)),
            Told::Alert("Couldn't connect Cursor".into(), Failure::Missing.message())
        );
        assert_eq!(disconnect_told("Cursor", ok("")), Told::Note("Disconnected from Cursor.".into()));
        assert_eq!(
            disconnect_told("Claude", no("", Some("extension"))),
            Told::Alert(anyai::REMOVE_IN_CLAUDE.into(), anyai::REMOVE_IN_CLAUDE_BODY.into())
        );
        assert_eq!(disconnect_told("Zed", no("busy", None)), Told::Alert("Couldn't disconnect Zed".into(), "busy".into()));
    }

    #[test]
    fn copy_my_day() {
        let day = |t: &str| Ok(DayAnswer { text: t.into(), chars: t.chars().count() });
        assert_eq!(day_outcome(day("  \n"), &YourAi::Other), Day::Nothing);
        assert_eq!(day_outcome(Err(Failure::Timeout), &YourAi::Other), Day::Failed(Failure::Timeout.message()));
        assert_eq!(day_outcome(day("hi"), &YourAi::Other), Day::Copy { text: "hi".into(), chars: 2, url: None });
        assert_eq!(
            day_outcome(day("hi"), &YourAi::of("gemini-web")),
            Day::Copy { text: "hi".into(), chars: 2, url: Some("https://gemini.google.com") }
        );
    }

    /// The one test that uses the shared state (tests run at the same time).
    #[test]
    fn shared_state_and_listeners() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = std::env::temp_dir().join(format!("cc-ai-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.json");
        std::fs::write(&config, r#"{"ai":"gemini-web"}"#).unwrap();
        init(config.clone());
        assert_eq!(state().choice, "gemini-web", "the saved choice, before the list is read");
        let heard = Arc::new(AtomicUsize::new(0));
        let h = heard.clone();
        on_change(move || {
            h.fetch_add(1, Ordering::SeqCst);
        });
        update(|s| s.note = Some("n".into()));
        update(|s| s.note = Some("n".into()));
        assert_eq!(heard.load(Ordering::SeqCst), 1, "only real changes are told");
        update(|s| s.day_line = Some(anyai::PASTE_LINE.into()));
        assert!(set_choice("any"));
        assert_eq!(anyai::saved_choice(&config).as_deref(), Some("any"));
        assert_eq!(state().day_line, None, "a new choice clears the day line");
        assert!(begin_work());
        assert!(!begin_work(), "never two at once");
        end_work();
        assert!(!state().working);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_state() {
        let mut s = AiState::new();
        assert_eq!(s.kind(), YourAi::Other, "nothing chosen: any AI");
        assert_eq!(s.name(), "your AI");
        s.clients = vec![client("cursor", true, false)];
        s.choice = "cursor".into();
        assert_eq!(s.name(), "CURSOR");
        assert!(!s.any_connected());
        s.clients[0].connected = true;
        assert!(s.any_connected());
        assert!(s.client("cursor").is_some() && s.client("zed").is_none());
    }
}
