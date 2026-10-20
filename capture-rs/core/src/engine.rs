// context-capsule — core: the recorder — start (via the picker) / stop, a sample every 3 s, state.json,
// typed and voice notes, and the `control` file the MCP server writes to drive us from outside.
//! Several screens or windows can be shared (up to `MAX_SOURCES`); each sample reads the window in use on the
//! one holding it, and, when the mouse moved or scrolled over another shared screen in the last 30 s, the
//! window under it too (`PointerRule`). Each sample saves only the lines not saved before for that window
//! (`NewLines`).
//! Once a minute: the evening reminder and the clean-up of old recordings (`DailyJobs`).
//! Projects: projects.json is reloaded when it changes; each sample, voice note and typed note gets
//! the pinned project or the one its window's clues point to.
//! A port of `capture/Sources/Recorder.swift`. Single-threaded: file writes and OCR happen inline on
//! the engine's thread (the platform gives it its own), so a slow OCR only delays the next message.

use crate::applog::AppLog;
use crate::clock;
use crate::events::{Event, EventLog};
use crate::exclusion::{Config, Exclusion};
use crate::image::crop_masked;
use crate::json::write_atomically;
use crate::membership::{Freshness, Membership, OcrDecision, PickStyle, Skip};
use crate::newlines::NewLines;
use crate::paths::Paths;
use crate::membership::{ListedWindow, PickedContent, WindowList};
use crate::platform::{screens_message, Front, Indicator, Msg, PickOutcome, Platform, Status, ALREADY_SHARED, MAX_SOURCES};
use crate::pointer::PointerRule;
use crate::projects::{Location, Project, ProjectChoice, Projects};
use crate::redact::Redactor;
use crate::reminder::DailyJobs;
use crate::state::{session_json, ControlCommand, ControlKind, StateJson};
use crate::text::{TextSource, UiText};
use crate::voice::{busy_listening_message, PlatformText, Voice, VoiceEngine, VoiceNotes, VoicePhase, VoiceSignal, VoiceTarget};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// One sample every 3 s while contextualizing.
pub const INTERVAL: Duration = Duration::from_secs(3);
/// The control file is checked twice a second.
pub const POLL: Duration = Duration::from_millis(500);

/// Platform facts the engine cannot find out itself.
#[derive(Clone, Debug)]
pub struct Options {
    /// Our own app id: never recorded (our tray menu, toasts).
    pub own_app_id: Option<String>,
    /// Hotkey names and settings paths for user messages.
    pub text: PlatformText,
    /// The user's locale for voice notes (e.g. "en-US"), unless config.json sets "voice_locale".
    pub locale: String,
}

impl Default for Options {
    fn default() -> Self {
        Options { own_app_id: None, text: PlatformText::LINUX, locale: "en-US".into() }
    }
}

/// The alert for a session that ended with no events: say why, so it is not a silent empty file.
pub fn empty_session_alert(why: Option<Skip>) -> (String, String) {
    let reason = match why {
        Some(s) => format!("Every few seconds, {}.", s.message()),
        None => "The session was too short to take a sample.".to_string(),
    };
    let body = reason
        + "\n\nContext Capsule only reads the window in front, and only if it is part of the window or screen \
           you picked. Next time, pick the screen you work on (or the window), and keep that window in front.";
    ("Nothing was saved".to_string(), body)
}

/// Watches the `control` file. Each new `at` is acted on once; a command left over from before we
/// launched is stale: remembered, never run. (Read every poll: it is tiny, and file times can be coarse.)
pub struct ControlWatcher {
    path: PathBuf,
    last_at: Option<i64>,
}

impl ControlWatcher {
    pub fn new(path: PathBuf) -> ControlWatcher {
        let last_at = Self::read(&path).map(|c| c.at);
        ControlWatcher { path, last_at }
    }

    fn read(path: &std::path::Path) -> Option<ControlCommand> {
        ControlCommand::parse(&std::fs::read(path).ok()?)
    }

    /// A new command, if one was written since the last poll.
    pub fn poll(&mut self) -> Option<ControlCommand> {
        let cmd = Self::read(&self.path)?;
        if Some(cmd.at) == self.last_at {
            return None;
        }
        self.last_at = Some(cmd.at);
        Some(cmd)
    }
}

struct Session {
    id: String,
    started_at: SystemTime,
    dir: PathBuf,
    log: EventLog,
    next_tick: Instant,
}

/// One window to read this sample: the window in use, or (focus "pointer") the one under the mouse.
struct Target {
    front: Front,
    window: ListedWindow,
    pick: PickedContent,
}

impl Target {
    fn focus(&self) -> &'static str {
        if self.front.pointer { "pointer" } else { "" }
    }
}

/// What a read of the window in use tells the project state: the project found, and the window.
type Noticed = (Option<(String, String)>, (String, String, String));

pub struct Engine<P: Platform> {
    paths: Paths,
    platform: P,
    opts: Options,
    voice: VoiceNotes<Box<dyn VoiceEngine>>,
    control: ControlWatcher,
    config: Config,
    session: Option<Session>,
    picking: bool,
    /// Why the last start did not happen ("cancelled", "picker: …", "capture: …"); None after a good start.
    last_error: Option<String>,
    /// A voice note is listening (shown in state.json).
    listening: bool,
    /// `at` of the last typed note from the MCP that was written.
    note_at: Option<i64>,
    /// Events written in this session, and why the latest sample wrote nothing (None: it wrote).
    event_count: usize,
    last_skip: Option<Skip>,
    skip_counts: [usize; 7],
    /// Session start or the last overview: frames from before it are stale (with `Platform::changed_at`).
    changed_at: Option<SystemTime>,
    indicator: Indicator,
    now: Box<dyn Fn() -> SystemTime>,
    /// Projects from projects.json (reloaded when its time changes), and auto-detect vs. pinned.
    projects: Vec<Project>,
    projects_modified: Option<SystemTime>,
    choice: ProjectChoice,
    /// The project of the latest sample (id, "auto" / "you").
    current_project: Option<(String, String)>,
    /// The front window as of the latest sample (title, where, app): what "New project from this
    /// window" uses.
    last_window: Option<(String, String, String)>,
    /// app.log: counts, reasons and error codes only.
    log: AppLog,
    /// How many screens or windows are shared (0 when not recording).
    sources: usize,
    /// Lines already saved per window in this recording.
    new_lines: NewLines,
    /// The evening reminder and the clean-up of old recordings.
    daily: DailyJobs,
    /// When the mouse last moved (between samples) in this recording, and where it was last sample.
    pointer_activity: Option<SystemTime>,
    last_pointer: Option<(f64, f64)>,
}

impl<P: Platform> Engine<P> {
    /// Reads the control file once, so a command left from before launch is never run.
    pub fn new(paths: Paths, platform: P, voice: Box<dyn VoiceEngine>, opts: Options) -> Self {
        let control = ControlWatcher::new(paths.control());
        let log = AppLog::new(paths.log());
        Engine {
            log,
            paths,
            platform,
            opts,
            voice: VoiceNotes::new(voice),
            control,
            config: Config::default(),
            session: None,
            picking: false,
            last_error: None,
            listening: false,
            note_at: None,
            event_count: 0,
            last_skip: None,
            skip_counts: [0; 7],
            changed_at: None,
            indicator: Indicator::Idle,
            now: Box::new(SystemTime::now),
            projects: vec![],
            projects_modified: None,
            choice: ProjectChoice::default(),
            current_project: None,
            last_window: None,
            sources: 0,
            new_lines: NewLines::new(),
            daily: DailyJobs::new(),
            pointer_activity: None,
            last_pointer: None,
        }
    }

    /// The clock for events and freshness (tests move time by hand). Voice notes use it too.
    pub fn set_clock(&mut self, now: Arc<dyn Fn() -> SystemTime>) {
        let n = now.clone();
        self.now = Box::new(move || n());
        self.voice.set_clock(Box::new(move || now()));
    }

    /// Called on the voice engine's thread after each reply; send `Msg::VoiceReady` from it.
    pub fn set_voice_wake(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        self.voice.set_wake(wake);
    }

    pub fn platform(&self) -> &P {
        &self.platform
    }
    pub fn platform_mut(&mut self) -> &mut P {
        &mut self.platform
    }
    pub fn paths(&self) -> &Paths {
        &self.paths
    }
    /// app.log (platforms may write to it too).
    pub fn log(&self) -> &AppLog {
        &self.log
    }
    pub fn is_recording(&self) -> bool {
        self.session.is_some()
    }
    pub fn is_picking(&self) -> bool {
        self.picking && self.session.is_none()
    }
    pub fn is_listening(&self) -> bool {
        self.listening
    }
    pub fn voice_phase(&self) -> VoicePhase {
        self.voice.phase()
    }
    pub fn session_id(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.id.as_str())
    }
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
    pub fn event_count(&self) -> usize {
        self.event_count
    }
    pub fn last_skip(&self) -> Option<Skip> {
        self.last_skip
    }

    pub fn projects(&self) -> &[Project] {
        &self.projects
    }
    pub fn project_choice(&self) -> &ProjectChoice {
        &self.choice
    }
    /// The current project (id, "auto" / "you").
    pub fn current_project(&self) -> Option<(&str, &str)> {
        self.current_project.as_ref().map(|(id, by)| (id.as_str(), by.as_str()))
    }

    pub fn status(&self) -> Status {
        Status {
            recording: self.is_recording(),
            picking: self.is_picking(),
            listening: self.listening,
            session_id: self.session.as_ref().map(|s| s.id.clone()),
            started_at: self.session.as_ref().map(|s| s.started_at),
            events: self.event_count,
            skip: self.last_skip,
            last_error: self.last_error.clone(),
            projects: self.projects.clone(),
            pinned: self.choice.pinned.clone(),
            project: self.current_project.as_ref().map(|p| p.0.clone()),
            project_by: self.current_project.as_ref().map(|p| p.1.clone()),
            new_project_clue: self.new_project_clue(),
            sources: self.sources,
        }
    }

    /// At launch: the sessions folder and a fresh state.json.
    pub fn launch(&mut self) {
        let _ = std::fs::create_dir_all(self.paths.sessions());
        self.reload_projects(true);
        self.changed();
    }

    // MARK: messages

    /// Handle one message. false = quit.
    pub fn handle(&mut self, msg: Msg) -> bool {
        match msg {
            Msg::ToggleRecord => self.toggle_record(),
            Msg::ToggleVoice => self.toggle_voice(),
            Msg::AddScreen => self.add_screen(),
            Msg::Pick(outcome) => self.on_pick(outcome),
            Msg::VoiceReady => self.pump_voice(),
            Msg::PickProject(id) => self.pin_project(id.as_deref()),
            Msg::NewProject { name, clue } => {
                self.new_project(&name, clue.as_deref());
            }
            Msg::Quit => {
                self.shutdown();
                return false;
            }
        }
        true
    }

    /// The platform's loop: messages as they come, the control file every 0.5 s, a sample every 3 s.
    /// Returns on `Msg::Quit` or when every sender is gone.
    pub fn run(&mut self, rx: Receiver<Msg>) {
        self.launch();
        let mut next_poll = Instant::now() + POLL;
        loop {
            let now = Instant::now();
            if now >= next_poll {
                self.poll_control();
                self.pump_voice();
                if self.reload_projects(false) {
                    self.changed(); // the tray's project list
                }
                let (clock_now, recording) = ((self.now)(), self.is_recording());
                if self.daily.poll(&self.paths, clock_now, recording, &self.log) {
                    self.platform.remind();
                }
                next_poll = now + POLL;
            }
            if let Some(s) = &mut self.session {
                if now >= s.next_tick {
                    s.next_tick = now + INTERVAL;
                    self.tick();
                }
            }
            let next = self.session.as_ref().map_or(next_poll, |s| s.next_tick.min(next_poll));
            match rx.recv_timeout(next.saturating_duration_since(Instant::now())) {
                Ok(msg) => {
                    if !self.handle(msg) {
                        return;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return self.shutdown(),
            }
        }
    }

    // MARK: start / stop

    pub fn toggle_record(&mut self) {
        if self.is_recording() && self.voice.phase() == VoicePhase::Listening {
            // A voice note is open: the record key waits until it is finished, so one key never ends both.
            self.log.write("record key while a voice note is open: waiting");
            let message = busy_listening_message(&self.opts.text);
            return self.platform.toast(&message);
        }
        if self.is_recording() {
            self.log.write("stop: hotkey or menu");
            self.stop()
        } else if self.is_picking() {
            self.cancel_picking()
        } else {
            self.start()
        }
    }

    /// Opens the picker (or brings it to the front). Recording begins only once the user picks.
    pub fn start(&mut self) {
        if self.is_recording() {
            return;
        }
        self.last_error = None;
        self.picking = true;
        self.platform.present_picker();
        self.changed();
    }

    /// While recording: share one more screen or window (the picker again), up to `MAX_SOURCES`.
    pub fn add_screen(&mut self) {
        if !self.is_recording() || self.sources >= MAX_SOURCES {
            return;
        }
        self.log.write("add a screen: picker");
        self.platform.add_source();
    }

    /// The evening reminder and clean-up, now (the run loop does this once a minute). true: reminded.
    pub fn daily_now(&mut self) -> bool {
        let (now, recording) = ((self.now)(), self.is_recording());
        let due = self.daily.run(&self.paths, now, recording, &self.log);
        if due {
            self.platform.remind();
        }
        due
    }

    /// Stops recording (or closes an open picker).
    pub fn stop(&mut self) {
        if self.is_picking() {
            return self.cancel_picking();
        }
        if !self.is_recording() {
            return;
        }
        self.end_session();
        self.changed();
    }

    /// Quitting: end any session, then a last state.json.
    pub fn shutdown(&mut self) {
        if self.is_recording() {
            self.log.write("stop: quit");
        }
        self.stop();
        self.changed();
    }

    fn cancel_picking(&mut self) {
        self.platform.stop_capture();
        self.picking = false;
        self.last_error = Some("cancelled".into());
        self.changed();
    }

    /// The picker's outcome (also what `Msg::Pick` does).
    pub fn on_pick(&mut self, outcome: PickOutcome) {
        let was_picking = self.picking;
        self.picking = false;
        match outcome {
            PickOutcome::Picked if was_picking => self.begin_session(),
            PickOutcome::Picked => {
                if !self.is_recording() {
                    self.platform.stop_capture(); // a pick after the user cancelled: not wanted
                }
            }
            PickOutcome::Cancelled => {
                self.log.write("picker: cancelled");
                self.last_error = Some("cancelled".into())
            }
            PickOutcome::Failed(message) => {
                self.log.write(&format!("start failed: {message}"));
                if self.is_recording() {
                    self.end_session();
                }
                self.last_error = Some(message);
            }
            PickOutcome::Stopped(message) => {
                if self.is_recording() {
                    self.log.write(&match &message {
                        Some(m) => format!("stop: capture error: {m}"),
                        None => "stop: capture closed (sharing was stopped from the system)".into(),
                    });
                }
                self.end_session();
                self.last_error = message.map(|m| format!("capture: {m}"));
            }
            PickOutcome::Added(count) if self.is_recording() => {
                self.sources = count;
                self.log.write(&format!("screen added: {count} shared"));
                self.platform.toast(&screens_message(count, true));
            }
            PickOutcome::Added(_) => self.platform.stop_capture(), // recording ended meanwhile
            PickOutcome::AddCancelled => self.log.write("add a screen: cancelled"),
            PickOutcome::AlreadyShared => {
                self.log.write("add a screen: picked one that's already shared");
                if self.is_recording() {
                    self.platform.toast(ALREADY_SHARED);
                }
            }
            PickOutcome::Removed(count) => {
                if self.is_recording() {
                    self.sources = count;
                    self.log.write(&format!("a shared screen stopped: {count} left"));
                    self.platform.toast(&screens_message(count, false));
                }
            }
        }
        self.changed();
    }

    fn begin_session(&mut self) {
        if self.is_recording() {
            return;
        }
        let now = (self.now)();
        let id = clock::session_id(now);
        let dir = self.paths.session_dir(&id);
        let host = self.platform.host_name();
        let opened = write_atomically(&dir.join("session.json"), &session_json(&id, now, None, &host))
            .and_then(|_| EventLog::open(&dir.join("events.jsonl")));
        let log = match opened {
            Ok(log) => log,
            Err(e) => {
                self.log.write(&format!("cannot create session {id}: {e}"));
                self.platform.stop_capture();
                self.last_error = Some(format!("cannot create session: {e}"));
                return;
            }
        };
        self.config = Config::load(&self.paths.config());
        self.last_error = None;
        self.event_count = 0;
        self.last_skip = None;
        self.skip_counts = [0; 7];
        self.changed_at = Some(now);
        self.reload_projects(true);
        self.current_project = self.choice.pinned.clone().map(|id| (id, ProjectChoice::YOU.to_string()));
        self.new_lines.reset();
        self.pointer_activity = None;
        self.last_pointer = None;
        self.sources = 1;
        let picked = match self.platform.picked().first().map(|p| p.style) {
            Some(PickStyle::Display) => "a screen",
            Some(PickStyle::Window) => "a window",
            None => "unknown",
        };
        self.log.write(&format!("recording started: {id}, picked {picked}"));
        self.session = Some(Session { id, started_at: now, dir, log, next_tick: Instant::now() + INTERVAL });
    }

    fn end_session(&mut self) {
        if self.is_recording() {
            self.voice.recording_will_stop(); // finish what was said first; it goes into this session
            self.apply_voice();
        }
        self.platform.stop_capture();
        self.picking = false;
        self.sources = 0;
        let Some(s) = self.session.take() else { return };
        let ended = (self.now)();
        let host = self.platform.host_name();
        let _ = write_atomically(&s.dir.join("session.json"), &session_json(&s.id, s.started_at, Some(ended), &host));
        let skips: Vec<String> = Skip::ALL
            .iter()
            .filter(|k| self.skip_counts[k.index()] > 0)
            .map(|k| format!("{} {}", k.as_str(), self.skip_counts[k.index()]))
            .collect();
        let skipped = if skips.is_empty() { String::new() } else { format!(", skipped: {}", skips.join(", ")) };
        self.log.write(&format!("recording ended: {}, {} events{skipped}", s.id, s.log.appended()));
        if s.log.appended() == 0 {
            // The most common skip; ties go to the first in `Skip::ALL`.
            let top = Skip::ALL
                .iter()
                .copied()
                .filter(|k| self.skip_counts[k.index()] > 0)
                .fold(None::<Skip>, |best, k| match best {
                    Some(b) if self.skip_counts[b.index()] >= self.skip_counts[k.index()] => Some(b),
                    _ => Some(k),
                });
            self.platform.empty_session(top);
        }
    }

    /// state.json, the tray icon and the menu, after anything changed.
    fn changed(&mut self) {
        let state = StateJson {
            recording: self.is_recording(),
            session_id: self.session.as_ref().map(|s| s.id.clone()),
            started_at: self.session.as_ref().map(|s| s.started_at),
            pid: std::process::id(),
            picking: self.is_picking(),
            last_error: self.last_error.clone(),
            events: self.event_count,
            skip: self.last_skip,
            listening: self.listening,
            note_at: self.note_at,
            project: self.current_project.as_ref().map(|p| p.0.clone()),
            project_by: self.current_project.as_ref().map(|p| p.1.clone()),
        };
        let _ = write_atomically(&self.paths.state(), &state.json());
        let indicator = if self.listening {
            Indicator::Listening
        } else if self.is_recording() {
            Indicator::Recording
        } else if self.is_picking() {
            Indicator::Picking
        } else {
            Indicator::Idle
        };
        if indicator != self.indicator {
            self.indicator = indicator;
            self.platform.set_indicator(indicator);
        }
        let status = self.status();
        self.platform.status_changed(&status);
    }

    // MARK: control file

    /// Check the control file once (the run loop does this every 0.5 s).
    pub fn poll_control(&mut self) {
        let Some(cmd) = self.control.poll() else { return };
        match cmd.kind {
            ControlKind::Start => {
                self.log.write("start: control file (Claude / MCP)");
                self.start()
            }
            ControlKind::Stop => {
                if self.is_recording() {
                    self.log.write("stop: control file (Claude / MCP)");
                }
                self.stop()
            }
            ControlKind::Note => {
                if let Some(text) = cmd.text {
                    self.add_typed_note(&text, cmd.at)
                }
            }
            ControlKind::Project => {
                let id = cmd.project.filter(|p| p != "auto");
                self.pin_project(id.as_deref())
            }
        }
    }

    // MARK: projects

    /// Read projects.json again if its time changed (or `force`). true: the list changed.
    pub fn reload_projects(&mut self, force: bool) -> bool {
        let path = self.paths.projects();
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if !force && modified == self.projects_modified {
            return false;
        }
        self.projects_modified = modified;
        let projects = Projects::load(&path);
        let changed = projects != self.projects;
        self.projects = projects;
        changed
    }

    /// Pin to a project (menu, or the MCP's `set_project`), or None to auto-detect again. An id that
    /// is not in projects.json means auto-detect.
    pub fn pin_project(&mut self, id: Option<&str>) {
        self.reload_projects(false);
        self.choice.pinned = id.filter(|id| self.projects.iter().any(|p| p.id == *id)).map(String::from);
        // No project names in the log.
        self.log.write(if self.choice.pinned.is_some() { "project: picked by you" } else { "project: auto-detect" });
        if let Some(pinned) = self.choice.pinned.clone() {
            self.set_current(Some((pinned, ProjectChoice::YOU.into())));
        } else if self.current_project.as_ref().is_some_and(|c| c.1 == ProjectChoice::YOU) {
            self.set_current(None);
        }
        self.changed(); // the menu's check marks
    }

    /// Make a project: the name the user typed, `clue` (if any) as its `match`. An existing project
    /// with the same id gets the clue added. None: no usable name, or projects.json cannot be written.
    pub fn create_project(&mut self, name: &str, clue: Option<&str>) -> Option<Project> {
        self.reload_projects(true);
        let id = Projects::slug(name);
        if id.is_empty() {
            return None;
        }
        let clue = clue.map(str::trim).filter(|c| !c.is_empty());
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == id) {
            if let Some(clue) = clue {
                if !p.clues.iter().any(|m| m.to_lowercase() == clue.to_lowercase()) {
                    p.clues.push(clue.to_string());
                }
            }
        } else {
            let clues = clue.map(|c| vec![c.to_string()]).unwrap_or_default();
            self.projects.push(Project { id: id.clone(), name: name.to_string(), clues });
        }
        if let Err(e) = Projects::save(&self.projects, &self.paths.projects()) {
            self.log.write(&format!("cannot save projects.json: {e}"));
            self.reload_projects(true);
            return None;
        }
        self.reload_projects(true);
        self.projects.iter().find(|p| p.id == id).cloned()
    }

    /// A clue from the window of the latest sample (see `Projects::clue`).
    pub fn new_project_clue(&self) -> Option<String> {
        let (title, location, _) = self.last_window.as_ref()?;
        Projects::clue(title, location)
    }

    /// "New project from this window": create it, then with a clue let auto-detect find it from now
    /// on; without one (or while another project is pinned) stick to it until changed.
    pub fn new_project(&mut self, name: &str, clue: Option<&str>) -> Option<Project> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let project = self.create_project(name, clue);
        match &project {
            Some(p) if clue.is_none() || self.choice.pinned.is_some() => self.pin_project(Some(&p.id.clone())),
            _ => self.changed(),
        }
        project
    }

    /// The current project, as (id, by). The platform hears about a new id while contextualizing.
    fn set_current(&mut self, next: Option<(String, String)>) {
        if next == self.current_project {
            return;
        }
        let new_id = next.as_ref().map(|n| &n.0) != self.current_project.as_ref().map(|c| &c.0);
        self.current_project = next;
        self.changed();
        if new_id && self.is_recording() {
            let id = self.current_project.as_ref().map(|c| c.0.clone());
            let project = id.and_then(|id| self.projects.iter().find(|p| p.id == id).cloned());
            self.platform.project_changed(project.as_ref());
        }
    }

    /// After a written sample: remember its window, and its project if it got one. Auto mode keeps the
    /// last project across windows with no clue (chat, mail…) for display only; those events stay
    /// unassigned.
    fn sampled_project(&mut self, found: Option<(String, String)>, window: (String, String, String)) {
        if !self.is_recording() {
            return;
        }
        let clue_before = self.new_project_clue();
        self.last_window = Some(window);
        if found.is_some() {
            self.set_current(found);
        }
        if self.new_project_clue() != clue_before {
            let status = self.status();
            self.platform.status_changed(&status); // the dialog's prefilled name
        }
    }

    // MARK: notes

    /// A note typed in Claude (the MCP's `add_note`). Ignored when not recording.
    pub fn add_typed_note(&mut self, text: &str, at: i64) {
        let Some(s) = &mut self.session else { return };
        let now = (self.now)();
        let mut event = Voice::event(text, Event::NOTE, "typed", now, now, "", "", "");
        if let Some((id, by)) = &self.current_project {
            event.project = id.clone();
            event.project_by = by.clone();
        }
        if let Err(e) = s.log.record(&event) {
            self.log.write(&format!("cannot write note: {e}"));
        }
        self.note_at = Some(at);
        self.event_count = s.log.appended();
        self.log.write(&format!("typed note saved: {} chars", crate::redact::char_count(&event.text)));
        self.changed();
    }

    /// Voice hotkey: start talking, press again to stop. Only while contextualizing.
    pub fn toggle_voice(&mut self) {
        // The voice locale from config.json, else the user's.
        let locale = Config::load(&self.paths.config()).voice_locale.unwrap_or_else(|| self.opts.locale.clone());
        let target = self.note_target();
        self.voice.toggle(&locale, target);
        self.apply_voice();
    }

    /// Handle voice engine replies that arrived, and the 2-minute limit.
    pub fn pump_voice(&mut self) {
        self.voice.pump();
        self.voice.check_timeout();
        self.apply_voice();
    }

    /// The voice engine, e.g. for a test to finish a held clip.
    pub fn voice_engine_mut(&mut self) -> &mut Box<dyn VoiceEngine> {
        self.voice.engine_mut()
    }

    /// Where a voice note goes: the current session, about the front window (nothing about a private
    /// app). None when not recording.
    fn note_target(&mut self) -> Option<VoiceTarget> {
        let session = self.session.as_ref()?.id.clone();
        let (own, user) = (self.opts.own_app_id.clone(), self.config.exclude_apps.clone());
        let is_excluded = move |id: Option<&str>, name: &str| {
            (id.is_some() && id == own.as_deref()) || Exclusion::is_excluded(id, Some(name), &user)
        };
        let (project, project_by) = self.current_project.clone().unwrap_or_default();
        let mut target = VoiceTarget { session, project, project_by, ..Default::default() };
        if let Some(front) = self.platform.front(&is_excluded) {
            if !is_excluded(front.app_id.as_deref(), &front.app_name) {
                let title = self.platform.ui_title(&front).or_else(|| front.window.as_ref().and_then(|w| w.name.clone()));
                target.app = front.app_name.clone();
                target.bundle = front.app_id.clone().unwrap_or_default();
                target.title = title.unwrap_or_default();
            }
        }
        Some(target)
    }

    fn apply_voice(&mut self) {
        for signal in self.voice.take_signals() {
            match signal {
                VoiceSignal::Indicator(on) => {
                    if on != self.listening {
                        self.listening = on;
                        self.changed();
                    }
                }
                VoiceSignal::Outcome(outcome) => {
                    self.log.write(&outcome.log_summary());
                    let message = outcome.message(&self.opts.text);
                    self.platform.toast(&message);
                }
                VoiceSignal::Save { session, event } => self.save_note(&session, &event),
            }
        }
    }

    /// Into the current session, or (if it stopped meanwhile) appended to that session's file.
    fn save_note(&mut self, session: &str, event: &Event) {
        let result = match &mut self.session {
            Some(s) if s.id == session => s.log.record(event).map(|_| {
                self.event_count = s.log.appended();
            }),
            _ => EventLog::open(&self.paths.session_dir(session).join("events.jsonl"))
                .and_then(|mut log| log.record(event).map(|_| ())),
        };
        if let Err(e) = result {
            self.log.write(&format!("cannot write note: {e}"));
        }
        self.changed();
    }

    // MARK: sampling

    /// One sample: the window in use (keyboard focus) when it's on something the user shared, and, if the
    /// mouse moved or scrolled over another shared screen in the last 30 s, the window under it too. Text
    /// comes from the accessibility API when allowed and useful, else from OCR of that screen's latest
    /// frame — cropped to the window, never-record windows painted out, never a frame from before the last
    /// app switch, never during a window overview.
    pub fn tick(&mut self) {
        if self.session.is_none() {
            return;
        }
        let now = (self.now)();
        let (own, user) = (self.opts.own_app_id.clone(), self.config.exclude_apps.clone());
        let is_excluded = move |id: Option<&str>, name: &str| {
            (id.is_some() && id == own.as_deref()) || Exclusion::is_excluded(id, Some(name), &user)
        };
        let Some(front) = self.platform.front(&is_excluded) else { return self.skip(Skip::NoWindow) };
        if front.overlay {
            self.changed_at = Some(now); // the frame after it closes is the first good one
            return self.skip(Skip::Overlay);
        }
        let picks = self.platform.picked();
        if picks.is_empty() {
            return self.skip(Skip::NotPicked);
        }

        // The window in use. Several screens can be shared: the one holding it is read.
        let mut targets: Vec<Target> = vec![];
        let mut front_skip = None;
        let mut front_pick = None;
        if is_excluded(front.app_id.as_deref(), &front.app_name) {
            front_skip = Some(Skip::PrivateApp); // nothing read
        } else if let Some(window) = front.window.clone() {
            if let Some(pick) = Membership::pick(&window, &picks) {
                front_pick = Some(pick.clone());
                targets.push(Target { front: front.clone(), window, pick: pick.clone() });
            } else {
                let unknown = picks.iter().any(|p| p.style == PickStyle::Display && p.rect.is_empty());
                front_skip = Some(if unknown { Skip::UnknownDisplay } else { Skip::NotPicked });
            }
        } else {
            front_skip = Some(Skip::NoWindow);
        }

        // Where you're looking: the window under the mouse, on another shared screen.
        if let Some(target) = self.pointer_target(&front, front_pick.as_ref(), &picks, now, &is_excluded) {
            targets.push(target);
        }
        if targets.is_empty() {
            return self.skip(front_skip.unwrap_or(Skip::NotPicked));
        }

        self.reload_projects(false);
        let mut outcome = front_skip;
        let mut noticed = None;
        for target in &targets {
            let read = self.read(target, now);
            if !target.front.pointer {
                match read {
                    Ok(n) => noticed = n,
                    Err(why) => outcome = Some(why),
                }
            }
        }
        let count = self.session.as_ref().map_or(self.event_count, |s| s.log.appended());
        self.sampled(outcome, count);
        if let Some((found, window)) = noticed {
            self.sampled_project(found, window);
        }
    }

    /// The window under the mouse, when `PointerRule` says to read it and it isn't private.
    fn pointer_target(
        &mut self,
        front: &Front,
        front_pick: Option<&PickedContent>,
        picks: &[PickedContent],
        now: SystemTime,
        is_excluded: &dyn Fn(Option<&str>, &str) -> bool,
    ) -> Option<Target> {
        let at = self.platform.pointer();
        if PointerRule::moved(self.last_pointer, at) {
            self.pointer_activity = Some(now);
        }
        self.last_pointer = at;
        let (x, y) = at?;
        let window = PointerRule::window_at(x, y, &front.list)?.clone();
        let pick = Membership::pick(&window, picks)?.clone();
        let activity = self.pointer_activity.max(self.platform.pointer_activity());
        if !PointerRule::should_read(Some(&window), Some(&pick), front.window.as_ref(), front_pick, activity, now)
            || WindowList::is_private(&window)
        {
            return None;
        }
        let mut under = self.platform.pointer_front(&window, front, is_excluded)?;
        if is_excluded(under.app_id.as_deref(), &under.app_name) {
            return None;
        }
        under.pointer = true;
        under.window = Some(window.clone());
        Some(Target { front: under, window, pick })
    }

    /// Read one target and write what's new. Err: why nothing was read. Ok: for the window in use, its
    /// project and window (the pointer's never touch the project state).
    fn read(&mut self, target: &Target, now: SystemTime) -> Result<Option<Noticed>, Skip> {
        let (front, window, pointer) = (&target.front, &target.window, target.front.pointer);
        let ui = self.platform.ui_text(front);
        let source = UiText::choose(ui.is_some(), ui.as_deref().unwrap_or(""));
        let mut text = ui.unwrap_or_default();
        if source == TextSource::Ocr {
            let changed_at = self.changed_at.max(self.platform.changed_at());
            let frame = self.platform.latest_frame(target.pick.source);
            let Some(frame) = frame.filter(|f| Freshness::ok(Some(f.arrived), changed_at)) else {
                return Err(self.break_run_unless(pointer, Skip::WaitingFrame));
            };
            let pick =
                self.platform.picked().into_iter().find(|p| p.source == target.pick.source).unwrap_or(target.pick.clone());
            match Membership::ocr_decision(&pick, window, &front.list) {
                OcrDecision::Skip(why) => return Err(self.break_run_unless(pointer, why)),
                OcrDecision::Read { region, masks } => {
                    text = match crop_masked(&frame.bgra, frame.width, frame.height, frame.stride, region, &masks) {
                        Some(image) => self.platform.ocr(&image), // the copy is dropped right after
                        None => String::new(),
                    };
                }
            }
        }
        let title = self.platform.ui_title(front).or_else(|| window.name.clone()).unwrap_or_default();
        let location = Location::clean(self.platform.location(front).as_deref());
        let stamp = clock::iso(now);
        let mut event = Event {
            t: stamp.clone(),
            end: stamp,
            app: front.app_name.clone(),
            bundle: front.app_id.clone().unwrap_or_default(),
            title: Redactor::clean(&title),
            src: source.as_str().into(),
            text: Redactor::clean(&text),
            location,
            focus: target.focus().into(),
            ..Default::default()
        };
        let found = self.choice.resolve(&self.projects, &event.title, &event.location, &front.app_name);
        if let Some((id, by)) = &found {
            event.project = id.clone();
            event.project_by = by.clone();
        }
        let noticed = (!pointer).then(|| (found, (event.title.clone(), event.location.clone(), front.app_name.clone())));
        // Save only what's new in this window since it was last seen. Nothing new: just extend the time (the
        // window in use), or write nothing (the window under the mouse).
        let key = NewLines::key(&event.app, &event.bundle, &event.title, &event.location);
        event.text = self.new_lines.filter(&event.text, &key);
        if event.text.is_empty() && pointer {
            return Ok(None);
        }
        let Some(s) = &mut self.session else { return Ok(noticed) };
        let extended = event.text.is_empty() && s.log.extend_if_same_window(&event).unwrap_or(false);
        if !extended {
            if let Err(e) = s.log.record(&event) {
                self.log.write(&format!("cannot write event: {e}"));
            }
        }
        Ok(noticed)
    }

    /// A failed read of the window in use breaks the run of samples (the pointer's doesn't).
    fn break_run_unless(&mut self, pointer: bool, why: Skip) -> Skip {
        if !pointer {
            if let Some(s) = &mut self.session {
                s.log.break_run();
            }
        }
        why
    }

    fn skip(&mut self, why: Skip) {
        if let Some(s) = &mut self.session {
            s.log.break_run();
        }
        self.sampled(Some(why), self.event_count);
    }

    /// Note how the sample went; state.json only changes when something did.
    fn sampled(&mut self, why: Option<Skip>, count: usize) {
        if !self.is_recording() {
            return;
        }
        if let Some(w) = why {
            self.skip_counts[w.index()] += 1;
        }
        if why == self.last_skip && count == self.event_count {
            return;
        }
        self.last_skip = why;
        self.event_count = count;
        self.changed();
    }
}
