// context-capsule — windows: any AI (SPEC "Any AI") — the AI apps on this PC, which one is Your AI, and what
// Make today's capsules, Copy my day, Save from clipboard and Connect do. A port of the Mac's AIState
// (capture/Sources/AIBridge.swift).
//! One state, owned by the UI thread: the window and the tray menu read it. Worker threads run the
//! context-capsule-mcp program (core `anyai`) and send `AiEvent`s back through `UiCmd::Ai`. The pure part
//! (the state, what a request does) is tested on any OS.

use context_capsule_core::anyai::{self, AiClient, YourAi};

/// The AI apps, Your AI, and what's running now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiState {
    /// From `clients`, installed first. Without the program: just Claude, as the app detects it.
    pub clients: Vec<AiClient>,
    /// The list came back at least once.
    pub loaded: bool,
    /// Your AI: an id from `clients`, a website id, or "any".
    pub choice: String,
    /// The app being connected or disconnected now.
    pub busy: Option<String>,
    /// A short line under the list after connecting ("Connected to Cursor. Restart Cursor to use it.").
    pub note: Option<String>,
    /// Shown after Copy my day, until a save works or the choice changes.
    pub day_line: Option<String>,
    /// Copy my day or Save from clipboard is running (both buttons are off meanwhile).
    pub working: bool,
    /// Settings → Connect your AI: the "More" fold (apps not installed) is open.
    pub more: bool,
    /// A `clients` call is running (never two at once).
    pub refreshing: bool,
    /// Goes up on every change, so the window knows to lay out again.
    pub version: u64,
}

impl Default for AiState {
    fn default() -> AiState {
        AiState {
            clients: vec![],
            loaded: false,
            choice: anyai::ANY_ID.into(),
            busy: None,
            note: None,
            day_line: None,
            working: false,
            more: false,
            refreshing: false,
            version: 0,
        }
    }
}

/// What a worker thread tells the UI thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiEvent {
    /// `clients` came back (Claude's own detection already applied).
    Clients(Vec<AiClient>),
    Busy(Option<String>),
    Note(Option<String>),
    Working(bool),
    DayLine(Option<String>),
    /// Capsules were saved: read the lists again.
    Saved,
    /// The first app connected becomes Your AI.
    Choose(String),
    /// "Connect X first" → Connect.
    ConnectNow(String),
    /// Ask the program for the list again.
    Refresh,
}

/// What a request for Your AI (make today's capsules, ask, continue from a capsule) does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenWay {
    /// Claude with the request typed in (Your AI is Claude, a website, any other AI, or an app not listed yet).
    Claude,
    /// Your AI is an app that isn't connected: ask to connect it first.
    ConnectFirst(AiClient),
    /// A connected app: the request goes on the clipboard.
    Copy(AiClient),
}

impl AiState {
    pub fn kind(&self) -> YourAi {
        YourAi::of(&self.choice)
    }

    pub fn client(&self, id: &str) -> Option<&AiClient> {
        self.clients.iter().find(|c| c.id == id)
    }

    pub fn changed(&mut self) {
        self.version += 1;
    }

    /// A refresh came back: sorted, and Your AI worked out again from the saved choice (config.json "ai").
    pub fn set_clients(&mut self, list: Vec<AiClient>, saved: Option<&str>) {
        self.refreshing = false;
        self.clients = anyai::sorted(list);
        self.loaded = true;
        self.choice = anyai::resolve(saved, &self.clients);
        self.changed();
    }

    /// What a request for Your AI does (the Mac's `AIState.open`).
    pub fn open_way(&self) -> OpenWay {
        let YourAi::App(id) = self.kind() else { return OpenWay::Claude };
        match self.client(&id) {
            None => OpenWay::Claude,
            Some(c) if c.connected => OpenWay::Copy(c.clone()),
            Some(c) => OpenWay::ConnectFirst(c.clone()),
        }
    }
}

/// The list when the program can't be run: Claude only, as the app knows it.
pub fn fallback(claude_installed: bool) -> Vec<AiClient> {
    vec![AiClient::claude(claude_installed, false)]
}

/// Claude's extension is seen at once through the app's own detection, even before the program looks again.
pub fn with_claude_detection(mut list: Vec<AiClient>, claude_connected: bool) -> Vec<AiClient> {
    if claude_connected {
        if let Some(c) = list.iter_mut().find(|c| c.id == anyai::CLAUDE_ID) {
            c.connected = true;
        }
    }
    list
}

#[cfg(windows)]
pub use win::{handle, init, make, open_received, refresh, save_from_clipboard, with};

#[cfg(windows)]
mod win {
    use super::*;
    use crate::app::{Ui, UiCmd};
    use crate::{connect as claude_app, input, program, reminder, win};
    use context_capsule_core::anyai::{Failure, Launch};
    use context_capsule_core::claude::{self, Opened};
    use context_capsule_core::reminder::DAY_PROMPT;
    use context_capsule_core::settings::ConfigFile;
    use context_capsule_core::window::CANCEL;
    use context_capsule_core::AppLog;
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::OnceLock;

    thread_local! {
        static STATE: RefCell<AiState> = RefCell::new(AiState::default());
        static CONFIG: RefCell<PathBuf> = const { RefCell::new(PathBuf::new()) };
    }

    /// Run `f` on the state (UI thread). Never call out of `f`: keep it to reading and setting fields.
    pub fn with<R>(f: impl FnOnce(&mut AiState) -> R) -> R {
        STATE.with(|s| f(&mut s.borrow_mut()))
    }

    pub fn snapshot() -> AiState {
        with(|s| s.clone())
    }

    fn config() -> PathBuf {
        CONFIG.with(|c| c.borrow().clone())
    }

    /// At start (UI thread): where config.json is, and Your AI as saved (until the list comes back).
    pub fn init(config: PathBuf) {
        let saved = anyai::saved_choice(&config);
        CONFIG.with(|c| *c.borrow_mut() = config);
        with(|s| s.choice = anyai::resolve(saved.as_deref(), &[]));
    }

    /// The program, found once per run (it may be written out next to the exe first: a second or two).
    fn launch(log: &AppLog) -> Result<Launch, Failure> {
        static LAUNCH: OnceLock<Option<Launch>> = OnceLock::new();
        LAUNCH.get_or_init(|| program::launch(&|l: &str| log.write(l))).clone().ok_or(Failure::Missing)
    }

    fn post(ui: &Ui, ev: AiEvent) {
        ui.post(UiCmd::Ai(ev));
    }

    fn alert(ui: &Ui, title: &str, body: &str) {
        ui.post(UiCmd::Alert(title.to_string(), body.to_string()));
    }

    fn spawn(f: impl FnOnce() + Send + 'static) {
        std::thread::spawn(f);
    }

    /// A worker's news, on the UI thread.
    pub fn handle(ev: AiEvent, ui: &Ui, log: &AppLog) {
        match ev {
            AiEvent::Clients(list) => {
                let saved = anyai::saved_choice(&config());
                with(|s| s.set_clients(list, saved.as_deref()));
            }
            AiEvent::Busy(b) => with(|s| {
                s.busy = b;
                s.changed();
            }),
            AiEvent::Note(n) => with(|s| {
                s.note = n;
                s.changed();
            }),
            AiEvent::Working(on) => with(|s| {
                s.working = on;
                s.changed();
            }),
            AiEvent::DayLine(l) => with(|s| {
                s.day_line = l;
                s.changed();
            }),
            AiEvent::Saved => crate::web_window::emit("data-changed", serde_json::json!({ "what": ["capsules"] })),
            AiEvent::Choose(id) => set_choice(&id, log),
            AiEvent::ConnectNow(id) => connect(ui, log, &id),
            AiEvent::Refresh => refresh(ui, log),
        }
    }

    /// Ask the program which AI apps there are (off the UI thread; never two at once).
    pub fn refresh(ui: &Ui, log: &AppLog) {
        if with(|s| std::mem::replace(&mut s.refreshing, true)) {
            return;
        }
        let (ui, log) = (ui.clone(), log.clone());
        spawn(move || {
            static SAID: AtomicBool = AtomicBool::new(false);
            let list = match launch(&log).and_then(|l| anyai::clients(&l)) {
                Ok(list) => list,
                Err(f) => {
                    if !SAID.swap(true, Ordering::Relaxed) {
                        log.write(&format!("any AI: clients: {}", f.message()));
                    }
                    fallback(claude_app::find().installed())
                }
            };
            post(&ui, AiEvent::Clients(with_claude_detection(list, claude_app::is_connected())));
        });
    }

    /// Settings → Your AI.
    pub fn set_choice(id: &str, log: &AppLog) {
        if let Err(e) = ConfigFile::set(&config(), anyai::KEY_AI, Some(id.into())) {
            log.write(&format!("cannot save ai: {e}"));
        }
        log.write(&format!("your AI: {id}"));
        with(|s| {
            s.choice = id.to_string();
            s.day_line = None;
            s.changed();
        });
    }

    /// Make today's capsules, the way Your AI works: Claude with the request typed in, another app with it on the
    /// clipboard, or Copy my day. UI thread.
    pub fn make(ui: &Ui, log: &AppLog) {
        match snapshot().kind() {
            YourAi::Claude => {
                let (ui, log) = (ui.clone(), log.clone());
                spawn(move || reminder::make(&ui, &log));
            }
            YourAi::App(_) => open(ui, log, DAY_PROMPT, "to read your day", "click Make today's capsules again", "make capsules"),
            YourAi::Website(_) | YourAi::Other => copy_my_day(ui, log),
        }
    }

    /// A capsule someone sent (Open with, or "Open a capsule…"): your AI carries on from it. Not a capsule: say so.
    pub fn open_received(ui: &Ui, log: &AppLog, path: &Path) {
        match claude::open_file(path) {
            Opened::Capsule { prompt } => open(ui, log, &prompt, "to read the capsule", "open the capsule again", "open a received capsule"),
            Opened::NotACapsule => {
                log.write("open a received capsule: not a capsule");
                alert(ui, claude::NOT_A_CAPSULE, claude::NOT_A_CAPSULE_BODY);
            }
            Opened::Unreadable => {
                log.write("open a received capsule: cannot read it");
                alert(ui, claude::NOT_A_CAPSULE, claude::CANT_READ);
            }
        }
    }

    /// A request for Your AI. Claude, websites and any other AI: Claude, as before (these need Context Capsule's
    /// tools). A connected app: the request on the clipboard. An app not connected yet: "Connect X first".
    fn open(ui: &Ui, log: &AppLog, prompt: &str, why: &str, again: &str, what: &str) {
        let state = snapshot();
        match state.open_way() {
            OpenWay::Claude => {
                let (ui, log, prompt, what) = (ui.clone(), log.clone(), prompt.to_string(), what.to_string());
                spawn(move || reminder::ask_claude(&ui, &log, &prompt, &what));
            }
            OpenWay::ConnectFirst(c) => {
                let (title, text) = anyai::connect_first(&c.name, why, again);
                log.write(&format!("{what}: {} is not connected", c.id));
                let ui = ui.clone();
                spawn(move || {
                    if input::ask_choice(&title, &text, anyai::CONNECT, CANCEL) {
                        post(&ui, AiEvent::ConnectNow(c.id));
                    }
                });
            }
            OpenWay::Copy(c) => {
                let copied = win::copy_text(prompt);
                log.write(&format!("{what}: {} with a request on the clipboard ({})", c.id, if copied { "ok" } else { "failed" }));
                if copied {
                    ui.post(UiCmd::Toast(anyai::copied_for(&c.name, false)));
                } else {
                    alert(ui, &c.name, &format!("In {}, start a new chat and type:\n\n{prompt}", c.name));
                }
            }
        }
    }

    /// Runs `day`, copies the message, opens the chosen website, and says what to do next.
    pub fn copy_my_day(ui: &Ui, log: &AppLog) {
        let Some(kind) = with(|s| {
            if s.working {
                return None;
            }
            s.working = true;
            s.changed();
            Some(s.kind())
        }) else {
            return;
        };
        log.write("copy my day");
        let (ui, log) = (ui.clone(), log.clone());
        spawn(move || match launch(&log).and_then(|l| anyai::day(&l, None)) {
            Err(f) => {
                post(&ui, AiEvent::Working(false));
                alert(&ui, anyai::COPY_FAILED, &f.message());
            }
            Ok(day) if day.text.trim().is_empty() => {
                post(&ui, AiEvent::Working(false));
                ui.post(UiCmd::Toast(anyai::NOTHING_TODAY.into()));
            }
            Ok(day) => {
                if !win::copy_text(&day.text) {
                    post(&ui, AiEvent::Working(false));
                    alert(&ui, anyai::COPY_FAILED, "Windows didn't let Context Capsule use the clipboard. Please try again.");
                    return;
                }
                log.write(&format!("copy my day: {} characters copied", day.chars));
                if let YourAi::Website(w) = kind {
                    win::open(w.url);
                }
                post(&ui, AiEvent::DayLine(Some(anyai::PASTE_LINE.into())));
                post(&ui, AiEvent::Working(false));
                ui.post(UiCmd::Toast(anyai::PASTE_LINE.into()));
            }
        });
    }

    /// Runs `save` on the clipboard, then shows what was saved and what wasn't (and why).
    pub fn save_from_clipboard(ui: &Ui, log: &AppLog) {
        if with(|s| s.working) {
            return;
        }
        let text = win::clipboard_text().unwrap_or_default();
        if text.trim().is_empty() {
            alert(ui, anyai::CLIPBOARD_EMPTY, anyai::CLIPBOARD_EMPTY_BODY);
            return;
        }
        with(|s| {
            s.working = true;
            s.changed();
        });
        let (ui, log) = (ui.clone(), log.clone());
        spawn(move || match launch(&log).and_then(|l| anyai::save(&l, &text)) {
            Err(f) => {
                post(&ui, AiEvent::Working(false));
                alert(&ui, anyai::SAVE_FAILED, &f.message());
            }
            Ok(result) => {
                log.write(&format!("save from clipboard: {} saved, {} refused", result.saved.len(), result.refused.len()));
                if !result.saved.is_empty() {
                    post(&ui, AiEvent::DayLine(None));
                    post(&ui, AiEvent::Saved);
                }
                post(&ui, AiEvent::Working(false));
                let (title, body) = anyai::summary(&result);
                alert(&ui, &title, &body);
            }
        });
    }

    /// Connect: Claude opens its extension (as before); another app gets Context Capsule in its MCP settings.
    /// UI thread, nothing borrowed (Claude's flow may show a dialog).
    pub fn connect(ui: &Ui, log: &AppLog, id: &str) {
        let state = snapshot();
        let Some(c) = state.client(id).cloned() else { return };
        if state.busy.is_some() {
            return;
        }
        log.write(&format!("connect: {}", c.id));
        if c.is_extension() {
            claude_app::connect(log);
            with(|s| {
                s.note = Some(anyai::CLAUDE_INSTALL_NOTE.into());
                s.changed();
            });
            return;
        }
        with(|s| {
            s.busy = Some(c.id.clone());
            s.note = None;
            s.changed();
        });
        let saved = anyai::saved_choice(&config());
        let (ui, log) = (ui.clone(), log.clone());
        spawn(move || {
            let answer = launch(&log).and_then(|l| anyai::connect(&l, &c.id));
            post(&ui, AiEvent::Busy(None));
            match answer {
                Ok(a) if a.ok => {
                    let note = if a.message.trim().is_empty() { anyai::connected_note(&c.name) } else { a.message };
                    post(&ui, AiEvent::Note(Some(note)));
                    if anyai::first_connected_becomes_choice(saved.as_deref(), &state.clients) {
                        post(&ui, AiEvent::Choose(c.id.clone()));
                    }
                }
                Ok(a) => alert(&ui, &anyai::connect_failed(&c.name), if a.message.trim().is_empty() { "Please try again." } else { &a.message }),
                Err(f) => alert(&ui, &anyai::connect_failed(&c.name), &f.message()),
            }
            post(&ui, AiEvent::Refresh);
        });
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(id: &str, name: &str, installed: bool, connected: bool) -> AiClient {
        AiClient { id: id.into(), name: name.into(), installed, connected, how: "config".into() }
    }

    fn state(choice: &str) -> AiState {
        let mut s = AiState::default();
        s.set_clients(
            vec![client("cursor", "Cursor", true, true), AiClient::claude(true, false), client("vscode", "VS Code", true, false), client("zed", "Zed", false, false)],
            Some(choice),
        );
        s
    }

    #[test]
    fn list_and_choice() {
        let fresh = AiState::default();
        assert_eq!(fresh.kind(), YourAi::Other, "nothing chosen, nothing listed: any AI");
        assert!(!fresh.loaded);

        let s = state("cursor");
        assert!(s.loaded && s.version > 0);
        assert_eq!(s.clients.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["claude", "cursor", "vscode", "zed"], "Claude first, installed first");
        assert_eq!(state("gone").choice, "cursor", "an app no longer listed: the first connected");
    }

    #[test]
    fn claude_is_seen_by_the_app_too() {
        assert_eq!(with_claude_detection(fallback(true), true), vec![AiClient::claude(true, true)]);
        assert!(!with_claude_detection(vec![client("cursor", "Cursor", true, false)], true)[0].connected);
        assert_eq!(fallback(false), vec![AiClient::claude(false, false)]);
    }

    #[test]
    fn what_a_request_does() {
        assert_eq!(state("claude").open_way(), OpenWay::Claude);
        assert_eq!(state("chatgpt-web").open_way(), OpenWay::Claude, "websites keep Claude's flow for requests");
        assert_eq!(state("any").open_way(), OpenWay::Claude);
        assert!(matches!(state("cursor").open_way(), OpenWay::Copy(c) if c.name == "Cursor"));
        assert!(matches!(state("vscode").open_way(), OpenWay::ConnectFirst(c) if c.id == "vscode"));
        let early = AiState { choice: "cursor".into(), ..Default::default() };
        assert_eq!(early.open_way(), OpenWay::Claude, "no list yet");
    }
}
