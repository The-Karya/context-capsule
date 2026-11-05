// context-capsule — core: Automatic capsules (SPEC "Automatic capsules"). The apps only decide when to start a
// run and say how it went; `context-capsule-mcp make --by auto|click` does the run (a local AI command-line tool,
// headless, with only Context Capsule's tools) and writes auto.json. Here: the decision (`due`, the same cases as
// Swift `AutoRun.due`), auto.json and config.json read, the `auto.status` object for the window, the tools found by
// `context-capsule-mcp agents` (cached), and [`Auto`]: the once-a-minute check and the run in the background,
// shared by the Windows and Linux apps (each gives an [`AutoHost`]: the program, notifications, the window).

use crate::anyai::{self, Launch};
use crate::applog::AppLog;
use crate::paths::Paths;
use crate::reminder::{recorded_on, Reminder};
use chrono::{DateTime, Local, Timelike};
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// The time when `remind_at` is missing, off or unreadable.
pub const DEFAULT_AT: &str = "18:00";
/// The config.json key: "auto" | "click" (missing = click).
pub const KEY_MAKE: &str = "make_capsules";
/// A "running" entry in auto.json older than this is a run that was cut off (as in mcp: 10 + 5 minutes).
pub const STALE: Duration = Duration::from_secs(15 * 60);
/// The app stops `make` after this (mcp stops the AI tool after 10 minutes itself).
pub const TIME_LIMIT: Duration = Duration::from_secs(12 * 60);
/// How often the tools are looked for again.
pub const AGENTS_EVERY: Duration = Duration::from_secs(5 * 60);
/// How often it checks whether a run is due.
pub const EVERY: Duration = Duration::from_secs(60);

/// Make capsules: Automatically, or When I click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Click,
}

impl Mode {
    pub fn parse(v: Option<&Value>) -> Mode {
        if v.and_then(Value::as_str) == Some("auto") {
            Mode::Auto
        } else {
            Mode::Click
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Mode::Auto => "auto",
            Mode::Click => "click",
        }
    }
}

/// Who started a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    Auto,
    Click,
}

impl By {
    pub fn id(self) -> &'static str {
        match self {
            By::Auto => "auto",
            By::Click => "click",
        }
    }
}

// MARK: the decision

/// The last run in auto.json, as the decision needs it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LastRun<'a> {
    /// The day it was for ("2026-10-23").
    pub day: &'a str,
    /// When it started, in local minutes after midnight (None: not today, or unreadable).
    pub started_minutes: Option<u32>,
}

/// Everything `due` looks at.
#[derive(Clone, Debug)]
pub struct DueInput<'a> {
    /// Local minutes after midnight, now.
    pub now_minutes: u32,
    /// Today, local ("2026-10-23").
    pub today: &'a str,
    pub mode: Mode,
    /// config `remind_at` (None, or a time it can't read: 18:00).
    pub at: Option<&'a str>,
    /// A tool is installed (Claude Code, Codex or Gemini CLI).
    pub has_agent: bool,
    /// auto.json `auto_day`: the day an automatic run last started.
    pub auto_day: Option<&'a str>,
    pub last: Option<LastRun<'a>>,
    /// Something was recorded today.
    pub recorded_today: bool,
    /// Recording right now.
    pub recording: bool,
    /// A run is going (this app's, or another's in auto.json).
    pub running: bool,
}

/// The time to use, in minutes after midnight: `remind_at`, else 18:00.
pub fn at_minutes(at: Option<&str>) -> u32 {
    at.and_then(Reminder::parse).unwrap_or(18 * 60)
}

/// "18:00": `remind_at` when it's a time, else 18:00.
pub fn at_text(at: Option<&str>) -> String {
    at.filter(|a| Reminder::parse(a).is_some()).unwrap_or(DEFAULT_AT).to_string()
}

/// Start an automatic run now? Automatically; a tool; past the time today; not already started today
/// (`auto_day`); no run for today started at or after the time (a click run then counts); something recorded
/// today; not recording; no run going. So a computer asleep at the time runs at the next wake the same day.
pub fn due(i: &DueInput) -> bool {
    let at = at_minutes(i.at);
    let ran_after_time = i.last.as_ref().is_some_and(|l| l.day == i.today && l.started_minutes.is_some_and(|m| m >= at));
    i.mode == Mode::Auto
        && i.has_agent
        && i.now_minutes >= at
        && i.auto_day != Some(i.today)
        && !ran_after_time
        && i.recorded_today
        && !i.recording
        && !i.running
}

// MARK: files

/// config.json's two settings for this: (mode, remind_at as written).
pub fn read_config(config: &Path) -> (Mode, Option<String>) {
    let v: Value = std::fs::read(config).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or(Value::Null);
    (Mode::parse(v.get(KEY_MAKE)), v.get("remind_at").and_then(Value::as_str).map(str::to_string))
}

/// auto.json, as written by `context-capsule-mcp make`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AutoFile {
    pub auto_day: Option<String>,
    /// `last`, as it is (the window shows it).
    pub last: Option<Value>,
}

impl AutoFile {
    pub fn path(paths: &Paths) -> std::path::PathBuf {
        paths.root.join("auto.json")
    }

    pub fn load(paths: &Paths) -> AutoFile {
        std::fs::read(Self::path(paths)).map(|d| Self::parse(&d)).unwrap_or_default()
    }

    pub fn parse(data: &[u8]) -> AutoFile {
        let Ok(Value::Object(o)) = serde_json::from_slice::<Value>(data) else { return AutoFile::default() };
        AutoFile {
            auto_day: o.get("auto_day").and_then(Value::as_str).map(str::to_string),
            last: o.get("last").filter(|l| l.is_object()).cloned(),
        }
    }

    fn last_str(&self, key: &str) -> Option<&str> {
        self.last.as_ref()?.get(key)?.as_str()
    }

    /// When the last run started.
    pub fn started(&self) -> Option<SystemTime> {
        DateTime::parse_from_rfc3339(self.last_str("started_at")?).ok().map(SystemTime::from)
    }

    /// A run another app (or this one) started is still going: `running`, under 15 minutes old, and its program is
    /// still there (`alive`; Windows and Linux check the pid).
    pub fn running(&self, now: SystemTime, alive: &dyn Fn(u32) -> bool) -> bool {
        if self.last_str("status") != Some("running") {
            return false;
        }
        let fresh = self.started().is_some_and(|t| now.duration_since(t).ok().is_none_or(|age| age < STALE));
        let pid = self.last.as_ref().and_then(|l| l.get("pid")).and_then(Value::as_u64).and_then(|p| u32::try_from(p).ok());
        fresh && pid.is_some_and(alive)
    }

    /// The last run for the decision: its day, and its start in local minutes when that was on `today`.
    pub fn last_run(&self, today: &str) -> Option<LastRun<'_>> {
        let day = self.last_str("day")?;
        let started = self.started().map(DateTime::<Local>::from);
        let started_minutes = started.filter(|t| t.format("%Y-%m-%d").to_string() == today).map(|t| t.hour() * 60 + t.minute());
        Some(LastRun { day, started_minutes })
    }

    /// `last` for the window: the documented fields only (no pid).
    pub fn last_json(&self) -> Value {
        let Some(l) = &self.last else { return Value::Null };
        let mut out = serde_json::Map::new();
        for k in ["day", "by", "status", "ai", "started_at", "finished_at", "capsules", "reason", "note"] {
            if let Some(v) = l.get(k) {
                out.insert(k.into(), v.clone());
            }
        }
        Value::Object(out)
    }
}

/// Linux: is this process there (`/proc/<pid>`)? Elsewhere the app passes its own check.
pub fn proc_alive(pid: u32) -> bool {
    pid != 0 && Path::new(&format!("/proc/{pid}")).exists()
}

// MARK: the program's answers

/// A tool that makes capsules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agent {
    pub id: String,
    pub name: String,
}

impl Agent {
    pub fn json(&self) -> Value {
        json!({ "id": self.id, "name": self.name })
    }
}

/// What `agents` found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// The one a run would use. None: no tool here, or none that can make capsules now.
    pub agent: Option<Agent>,
    /// Tools are here but none can make capsules now: why, in plain words ("Codex isn't signed in…").
    pub problem: Option<String>,
}

/// `agents` → what it found. None: not an answer.
pub fn parse_agents(out: &[u8]) -> Option<Found> {
    let v = anyai::json(out)?;
    let o = v.as_object()?;
    if !o.contains_key("agents") {
        return None;
    }
    let one = |a: &Value| Some(Agent { id: a.get("id")?.as_str()?.to_string(), name: a.get("name")?.as_str()?.to_string() });
    let problem = o.get("problem").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty()).map(str::to_string);
    Some(Found { agent: o.get("use").and_then(one), problem })
}

/// `make`'s answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MakeResult {
    /// "done" | "failed" | "running" | "none" | "nothing".
    pub status: String,
    pub name: Option<String>,
    pub capsules: u64,
    /// Plain words for a failure.
    pub reason: Option<String>,
}

/// When `make` gave no answer.
pub const NO_ANSWER: &str = "Context Capsule couldn't finish. Try again.";
pub const TOO_LONG: &str = "It took too long, so it was stopped.";

impl MakeResult {
    pub fn failed(reason: &str) -> MakeResult {
        MakeResult { status: "failed".into(), name: None, capsules: 0, reason: Some(reason.into()) }
    }
}

pub fn parse_make(out: &[u8]) -> Option<MakeResult> {
    let v = anyai::json(out)?;
    let status = v.get("status")?.as_str()?.to_string();
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty());
    let reason = text("reason").or_else(|| (status == "failed").then(|| NO_ANSWER.to_string()));
    Some(MakeResult { status, name: text("name"), capsules: v.get("capsules").and_then(Value::as_u64).unwrap_or(0), reason })
}

/// The notification after a run: (title, body). None: nothing to say (no tool, nothing recorded, already going).
pub fn notification(r: &MakeResult) -> Option<(String, String)> {
    match r.status.as_str() {
        "done" => Some((if r.capsules == 1 { "1 capsule ready".into() } else { format!("{} capsules ready", r.capsules) }, String::new())),
        "failed" => Some(("Couldn't make today's capsules".into(), r.reason.clone().unwrap_or_else(|| NO_ANSWER.into()))),
        _ => None,
    }
}

// MARK: running the program

/// Runs `context-capsule-mcp <args>` in the background (call off the UI thread): low priority (Windows: below
/// normal, no console window; Linux: through `nice`), stopped after `limit`. Its stdout; Err(true): too long.
pub fn run_low(launch: &Launch, args: &[&str], limit: Duration) -> Result<Vec<u8>, bool> {
    let (program, mut all): (std::path::PathBuf, Vec<std::ffi::OsString>) = match launch {
        Launch::Binary(p) => (p.clone(), vec![]),
        Launch::Node { node, script } => (node.clone(), vec![script.clone().into_os_string()]),
    };
    all.extend(args.iter().map(std::ffi::OsString::from));
    let nice = ["/usr/bin/nice", "/bin/nice"].into_iter().map(Path::new).find(|p| cfg!(unix) && p.is_file());
    let mut cmd = match nice {
        Some(n) => {
            let mut c = Command::new(n);
            c.args(["-n", "10"]).arg(&program);
            c
        }
        None => Command::new(&program),
    };
    cmd.args(&all).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
    let mut child = cmd.spawn().map_err(|_| false)?;
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        buf
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() >= limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(true);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(_) => return Err(false),
        }
    }
    Ok(reader.join().unwrap_or_default())
}

// MARK: the app's part

/// What each app does for automatic capsules.
pub trait AutoHost: Send + Sync {
    /// How to start context-capsule-mcp (None: missing).
    fn launch(&self) -> Option<Launch>;
    /// Is the process with this id still there (a run in auto.json)?
    fn alive(&self, pid: u32) -> bool;
    /// The status changed (a run started or ended, the tools changed): the window's `auto` event.
    fn changed(&self, status: Value);
    /// A run ended: the notification (see [`notification`]) and the window's `data-changed` capsules.
    fn finished(&self, by: By, result: &MakeResult);
}

#[derive(Default)]
struct State {
    /// The tool a run would use, once looked for.
    agent: Option<Option<Agent>>,
    /// Why none of the tools here can make capsules now (with no `agent`).
    problem: Option<String>,
    agents_at: Option<Instant>,
    /// This app's run is going.
    running: bool,
    /// A check is going (its thread).
    checking: bool,
    next_check: Option<Instant>,
    /// The day this app last started an automatic run: never twice a day, even when `make` couldn't say so in
    /// auto.json (it didn't start, or was stopped).
    auto_tried: Option<String>,
}

/// Automatic capsules for one app: once a minute (and at launch, and on wake) it checks `due` and starts a run;
/// `auto.run` starts one by click. Every run is on a thread of its own.
pub struct Auto {
    paths: Paths,
    log: AppLog,
    host: Box<dyn AutoHost>,
    state: Mutex<State>,
}

/// Today, local: ("2026-10-23", minutes after midnight).
fn local_now(now: SystemTime) -> (String, u32) {
    let t = DateTime::<Local>::from(now);
    (t.format("%Y-%m-%d").to_string(), t.hour() * 60 + t.minute())
}

impl Auto {
    pub fn new(paths: Paths, log: AppLog, host: Box<dyn AutoHost>) -> Arc<Auto> {
        Arc::new(Auto { paths, log, host, state: Mutex::default() })
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Look for the tools again (`context-capsule-mcp agents`); the window hears when that changed.
    pub fn refresh_agents(&self) -> Option<Agent> {
        let found = self
            .host
            .launch()
            .and_then(|l| anyai::run(&l, &["agents"], None, Duration::from_secs(30)).ok())
            .and_then(|out| parse_agents(&out));
        let (agent, changed) = {
            let mut s = self.state();
            s.agents_at = Some(Instant::now());
            // Unreadable: keep what was known (none, the first time).
            let found = found.unwrap_or_else(|| Found { agent: s.agent.clone().flatten(), problem: s.problem.clone() });
            let changed = s.agent.as_ref() != Some(&found.agent) || s.problem != found.problem;
            s.agent = Some(found.agent.clone());
            s.problem = found.problem;
            (found.agent, changed)
        };
        if changed {
            self.host.changed(self.status());
        }
        agent
    }

    /// The tool a run would use (looked for the first time if needed: blocks then).
    pub fn agent(&self) -> Option<Agent> {
        let known = self.state().agent.clone();
        match known {
            Some(a) => a,
            None => self.refresh_agents(),
        }
    }

    fn running_now(&self) -> bool {
        self.state().running || AutoFile::load(&self.paths).running(SystemTime::now(), &|pid| self.host.alive(pid))
    }

    /// `auto.status` (ui/BRIDGE.md).
    pub fn status(&self) -> Value {
        let (mode, at) = read_config(&self.paths.config());
        let file = AutoFile::load(&self.paths);
        let agent = self.agent();
        let problem = self.state().problem.clone();
        let running = self.state().running || file.running(SystemTime::now(), &|pid| self.host.alive(pid));
        let mut v = status_json(mode, at.as_deref(), agent.as_ref(), running, &file);
        if let (Some(p), None) = (problem, &agent) {
            v["problem"] = Value::String(p);
        }
        v
    }

    /// Automatically with a tool here: the evening reminder isn't shown.
    pub fn replaces_reminder(&self) -> bool {
        read_config(&self.paths.config()).0 == Mode::Auto && self.agent().is_some()
    }

    /// Call often (the engine's loop): at most once a minute, a check on a thread of its own.
    pub fn poll(self: &Arc<Self>, recording: bool) {
        {
            let mut s = self.state();
            let now = Instant::now();
            if s.checking || s.next_check.is_some_and(|n| now < n) {
                return;
            }
            s.checking = true;
            s.next_check = Some(now + EVERY);
        }
        let me = self.clone();
        let spawned = std::thread::Builder::new().name("auto".into()).spawn(move || {
            me.check(recording);
            me.state().checking = false;
        });
        if spawned.is_err() {
            self.state().checking = false;
        }
    }

    /// The computer woke up (or a setting changed): check at the next poll.
    pub fn wake(&self) {
        self.state().next_check = None;
    }

    /// One check, now (blocking: it may run `agents`, then the whole run).
    pub fn check(self: &Arc<Self>, recording: bool) {
        let stale = self.state().agents_at.is_none_or(|t| t.elapsed() >= AGENTS_EVERY);
        let agent = if stale { self.refresh_agents() } else { self.agent() };
        let (mode, at) = read_config(&self.paths.config());
        if mode != Mode::Auto {
            return;
        }
        let now = SystemTime::now();
        let (today, minutes) = local_now(now);
        let file = AutoFile::load(&self.paths);
        // Cheap checks first: the recordings folder is read only when the rest says yes.
        let mut input = DueInput {
            now_minutes: minutes,
            today: &today,
            mode,
            at: at.as_deref(),
            has_agent: agent.is_some(),
            auto_day: file.auto_day.as_deref(),
            last: file.last_run(&today),
            recorded_today: true,
            recording,
            running: self.running_now(),
        };
        if !due(&input) {
            return;
        }
        input.recorded_today = recorded_on(&self.paths.sessions(), &today.replace('-', ""));
        if !due(&input) || self.state().auto_tried.as_deref() == Some(today.as_str()) {
            return;
        }
        self.state().auto_tried = Some(today.clone());
        self.log.write("automatic capsules: starting");
        self.run(By::Auto);
    }

    /// `auto.run`: a run by click, in the background. Err "none" (no tool), "nothing" (nothing recorded today),
    /// "running".
    pub fn run_click(self: &Arc<Self>) -> Result<(), String> {
        if self.agent().is_none() {
            return Err("none".into());
        }
        let (today, _) = local_now(SystemTime::now());
        if !recorded_on(&self.paths.sessions(), &today.replace('-', "")) {
            return Err("nothing".into());
        }
        if self.running_now() {
            return Err("running".into());
        }
        let me = self.clone();
        std::thread::Builder::new().name("auto-run".into()).spawn(move || me.run(By::Click)).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// A setting the decision reads was saved: look for the tools again (Your AI picks which one) and check now.
    pub fn config_changed(self: &Arc<Self>, key: &str) {
        if !["ai", KEY_MAKE, "remind_at"].contains(&key) {
            return;
        }
        self.wake();
        let me = self.clone();
        let key = key.to_string();
        let _ = std::thread::Builder::new().name("auto-agents".into()).spawn(move || {
            if key == "remind_at" {
                me.host.changed(me.status());
            } else {
                me.state().agents_at = None;
                me.refresh_agents();
                // the mode changed even when the tool didn't
                me.host.changed(me.status());
            }
        });
    }

    /// One run, here and now (blocking up to 12 minutes).
    fn run(&self, by: By) {
        {
            let mut s = self.state();
            if s.running {
                return;
            }
            s.running = true;
        }
        self.host.changed(self.status());
        let result = match self.host.launch() {
            None => MakeResult::failed(NO_ANSWER),
            Some(l) => match run_low(&l, &["make", "--by", by.id()], TIME_LIMIT) {
                Ok(out) => parse_make(&out).unwrap_or_else(|| MakeResult::failed(NO_ANSWER)),
                Err(true) => MakeResult::failed(TOO_LONG),
                Err(false) => MakeResult::failed(NO_ANSWER),
            },
        };
        self.state().running = false;
        if by == By::Auto && !matches!(result.status.as_str(), "done" | "failed") {
            self.log.write(&format!("automatic capsules: not made ({})", result.status));
        }
        self.host.changed(self.status());
        self.host.finished(by, &result);
    }
}

/// The `auto.status` object.
pub fn status_json(mode: Mode, at: Option<&str>, agent: Option<&Agent>, running: bool, file: &AutoFile) -> Value {
    json!({
        "mode": mode.id(),
        "at": at_text(at),
        "agent": agent.map_or(Value::Null, Agent::json),
        "running": running,
        "autoDay": file.auto_day,
        "last": file.last_json(),
    })
}

/// `auto.status` without an app's state (no tool known, nothing running here).
pub fn status_from_files(paths: &Paths) -> Value {
    let (mode, at) = read_config(&paths.config());
    status_json(mode, at.as_deref(), None, false, &AutoFile::load(paths))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: &str = "2026-10-23";

    fn base() -> DueInput<'static> {
        DueInput {
            now_minutes: 18 * 60 + 5,
            today: TODAY,
            mode: Mode::Auto,
            at: Some("18:00"),
            has_agent: true,
            auto_day: None,
            last: None,
            recorded_today: true,
            recording: false,
            running: false,
        }
    }

    #[test]
    fn times() {
        assert!(due(&base()), "just after the time");
        assert!(!due(&DueInput { now_minutes: 17 * 60 + 59, ..base() }), "before the time");
        assert!(due(&DueInput { now_minutes: 18 * 60, ..base() }), "exactly at the time");
        assert!(due(&DueInput { now_minutes: 9 * 60, at: Some("08:30"), ..base() }), "an earlier time");
    }

    #[test]
    fn catches_up_after_sleep() {
        assert!(due(&DueInput { now_minutes: 22 * 60 + 40, ..base() }), "woke late the same day, not run yet");
        let yesterday = LastRun { day: "2026-10-22", started_minutes: None };
        assert!(due(&DueInput { now_minutes: 23 * 60, auto_day: Some("2026-10-22"), last: Some(yesterday), ..base() }));
    }

    #[test]
    fn once_a_day() {
        assert!(!due(&DueInput { auto_day: Some(TODAY), ..base() }), "already started today");
        let click_after = LastRun { day: TODAY, started_minutes: Some(18 * 60 + 2) };
        assert!(!due(&DueInput { last: Some(click_after), ..base() }), "a click run after the time counts");
        let click_before = LastRun { day: TODAY, started_minutes: Some(14 * 60) };
        assert!(due(&DueInput { last: Some(click_before), ..base() }), "a click run before the time doesn't");
        let other_day = LastRun { day: "2026-10-22", started_minutes: Some(19 * 60) };
        assert!(due(&DueInput { last: Some(other_day), ..base() }), "a run for another day doesn't");
    }

    #[test]
    fn skipped() {
        assert!(!due(&DueInput { recorded_today: false, ..base() }), "nothing recorded");
        assert!(!due(&DueInput { recording: true, ..base() }), "recording");
        assert!(!due(&DueInput { running: true, ..base() }), "a run is going");
        assert!(!due(&DueInput { has_agent: false, ..base() }), "no tool");
        assert!(!due(&DueInput { mode: Mode::Click, ..base() }), "When I click");
    }

    #[test]
    fn bad_or_missing_time_is_six() {
        for at in [None, Some("junk"), Some("25:00"), Some("")] {
            assert!(!due(&DueInput { at, now_minutes: 17 * 60 + 59, ..base() }), "{at:?} before 18:00");
            assert!(due(&DueInput { at, now_minutes: 18 * 60, ..base() }), "{at:?} at 18:00");
            assert_eq!(at_text(at), "18:00");
        }
        assert_eq!(at_text(Some("07:15")), "07:15");
        assert_eq!(Mode::parse(Some(&json!("auto"))), Mode::Auto);
        assert_eq!(Mode::parse(Some(&json!("click"))), Mode::Click);
        assert_eq!(Mode::parse(None), Mode::Click, "missing = When I click");
    }

    #[test]
    fn auto_json() {
        let text = br#"{"auto_day":"2026-10-23","last":{"day":"2026-10-23","by":"auto","status":"running","ai":"claude-code",
            "started_at":"2026-10-23T17:00:04Z","pid":4242}}"#;
        let f = AutoFile::parse(text);
        assert_eq!(f.auto_day.as_deref(), Some("2026-10-23"));
        let started = f.started().unwrap();
        let soon = started + Duration::from_secs(60);
        assert!(f.running(soon, &|pid| pid == 4242));
        assert!(!f.running(soon, &|_| false), "its program is gone");
        assert!(!f.running(started + Duration::from_secs(16 * 60), &|_| true), "over 15 minutes");
        assert_eq!(f.last_run("2026-10-23").unwrap().day, "2026-10-23");
        assert!(f.last_json().get("pid").is_none(), "no pid for the window");
        let done = AutoFile::parse(br#"{"last":{"day":"2026-10-23","status":"done","started_at":"2026-10-23T17:00:04Z"}}"#);
        assert!(!done.running(SystemTime::now(), &|_| true));
        assert_eq!(AutoFile::parse(b"not json"), AutoFile::default());
        assert_eq!(AutoFile::parse(b"{}").last_json(), Value::Null);
    }

    #[test]
    fn status_shape() {
        let f = AutoFile::parse(br#"{"auto_day":"2026-10-23","last":{"day":"2026-10-23","by":"auto","status":"done","ai":"claude-code",
            "started_at":"2026-10-23T17:00:04Z","finished_at":"2026-10-23T17:04:40Z","capsules":3}}"#);
        let agent = Agent { id: "claude-code".into(), name: "Claude Code".into() };
        let s = status_json(Mode::Auto, Some("bad"), Some(&agent), false, &f);
        assert_eq!(
            s,
            json!({"mode": "auto", "at": "18:00", "agent": {"id": "claude-code", "name": "Claude Code"}, "running": false,
                   "autoDay": "2026-10-23",
                   "last": {"day": "2026-10-23", "by": "auto", "status": "done", "ai": "claude-code",
                            "started_at": "2026-10-23T17:00:04Z", "finished_at": "2026-10-23T17:04:40Z", "capsules": 3}})
        );
        let empty = status_json(Mode::Click, None, None, false, &AutoFile::default());
        assert_eq!(empty, json!({"mode": "click", "at": "18:00", "agent": null, "running": false, "autoDay": null, "last": null}));
        // done with the next tool: the window says why
        let other = AutoFile::parse(br#"{"last":{"day":"2026-10-23","status":"done","ai":"codex","pid":7,
            "note":"Claude Code isn't signed in. Open it once and sign in. So Codex made them."}}"#);
        assert_eq!(other.last_json()["note"], "Claude Code isn't signed in. Open it once and sign in. So Codex made them.");
        assert!(other.last_json().get("pid").is_none());
    }

    #[test]
    fn program_answers() {
        let a = parse_agents(br#"{"agents":[{"id":"codex","name":"Codex","ready":true}],"use":{"id":"codex","name":"Codex"}}"#);
        assert_eq!(a, Some(Found { agent: Some(Agent { id: "codex".into(), name: "Codex".into() }), problem: None }));
        assert_eq!(parse_agents(br#"{"agents":[],"use":null}"#), Some(Found::default()));
        let none_ready = br#"{"agents":[{"id":"codex","name":"Codex","ready":false,"reason":"Codex isn't signed in. Open it once and sign in."}],
            "use":null,"problem":"Codex isn't signed in. Open it once and sign in."}"#;
        assert_eq!(
            parse_agents(none_ready),
            Some(Found { agent: None, problem: Some("Codex isn't signed in. Open it once and sign in.".into()) })
        );
        assert_eq!(parse_agents(b"oops"), None);
        let m = parse_make(br#"{"ok":true,"day":"2026-10-23","status":"done","ai":"claude-code","name":"Claude Code","capsules":3}"#).unwrap();
        assert_eq!((m.status.as_str(), m.capsules, m.reason.as_deref()), ("done", 3, None));
        assert_eq!(notification(&m), Some(("3 capsules ready".into(), String::new())));
        let one = MakeResult { capsules: 1, ..m };
        assert_eq!(notification(&one).unwrap().0, "1 capsule ready");
        let f = parse_make(br#"{"ok":false,"status":"failed","reason":"Claude Code isn't signed in. Open it once and sign in."}"#).unwrap();
        assert_eq!(notification(&f), Some(("Couldn't make today's capsules".into(), "Claude Code isn't signed in. Open it once and sign in.".into())));
        let crashed = parse_make(br#"{"ok":false,"status":"failed","error":"EACCES /x/y"}"#).unwrap();
        assert_eq!(crashed.reason.as_deref(), Some(NO_ANSWER), "never the raw error");
        for s in ["none", "nothing", "running"] {
            let r = parse_make(format!(r#"{{"ok":false,"status":"{s}","reason":"x"}}"#).as_bytes()).unwrap();
            assert_eq!(notification(&r), None, "{s}: log only");
        }
    }

    #[test]
    fn settings_from_config() {
        let dir = std::env::temp_dir().join(format!("cc-auto-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.json");
        std::fs::write(&config, br#"{"make_capsules":"auto","remind_at":"19:30"}"#).unwrap();
        assert_eq!(read_config(&config), (Mode::Auto, Some("19:30".into())));
        std::fs::write(&config, br#"{"remind_at":false}"#).unwrap();
        assert_eq!(read_config(&config), (Mode::Click, None));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(read_config(&dir.join("missing.json")), (Mode::Click, None));
    }
}
