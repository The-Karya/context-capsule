// context-capsule — core: voice notes — press to talk, press again to stop. Silence rules, the note's
// event line, user messages, and the state machine that always turns the mic indicator off.
//! The mic and the on-device recognizer live in the platform ([`VoiceEngine`]); no audio is kept.

use crate::clock;
use crate::events::Event;
use crate::redact::{char_count, Redactor};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// One finished recording from the mic: the on-device transcript and how loud the clip got.
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceClip {
    pub text: String,
    /// Loudest sample, 0…1 (absolute value).
    pub peak: f32,
    pub seconds: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePermission {
    Microphone,
    Speech,
}

impl VoicePermission {
    pub fn as_str(&self) -> &'static str {
        match self {
            VoicePermission::Microphone => "microphone",
            VoicePermission::Speech => "speech",
        }
    }
}

/// The OS-specific words in user messages: hotkeys and where to fix things.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformText {
    pub record_hotkey: &'static str,
    pub voice_hotkey: &'static str,
    /// Where to check the microphone input.
    pub sound_input: &'static str,
    /// Where to allow the microphone.
    pub mic_privacy: &'static str,
    /// Where to allow speech recognition.
    pub speech_privacy: &'static str,
    /// Whose servers we never send audio to.
    pub cloud: &'static str,
    /// The config file, as the user would find it.
    pub config_file: &'static str,
    /// How to get on-device speech for another language.
    pub add_language: &'static str,
}

impl PlatformText {
    pub const MACOS: PlatformText = PlatformText {
        record_hotkey: "⌥⇧C",
        voice_hotkey: "⌥⇧V",
        sound_input: "System Settings → Sound → Input",
        mic_privacy: "System Settings → Privacy & Security → Microphone",
        speech_privacy: "System Settings → Privacy & Security → Speech Recognition",
        cloud: "Apple",
        config_file: "~/.context-capsule/config.json",
        add_language: "add the language in System Settings → Keyboard → Dictation",
    };
    pub const WINDOWS: PlatformText = PlatformText {
        record_hotkey: "Alt+Shift+C",
        voice_hotkey: "Alt+Shift+V",
        sound_input: "Settings → System → Sound → Input",
        mic_privacy: "Settings → Privacy & security → Microphone",
        speech_privacy: "Settings → Privacy & security → Speech",
        cloud: "Microsoft",
        config_file: "%USERPROFILE%\\.context-capsule\\config.json",
        add_language: "install a speech model for that language",
    };
    pub const LINUX: PlatformText = PlatformText {
        record_hotkey: "Alt+Shift+C",
        voice_hotkey: "Alt+Shift+V",
        sound_input: "your Sound settings (Input)",
        mic_privacy: "your Privacy settings (Microphone)",
        speech_privacy: "your Privacy settings",
        cloud: "any server",
        config_file: "config.json in your Context Capsule folder",
        add_language: "install a speech model for that language",
    };
}

/// How a voice note ended. Every case turns the mic indicator off.
#[derive(Clone, Debug, PartialEq)]
pub enum VoiceOutcome {
    Saved { chars: usize, seconds: f64 },
    /// Nothing heard (silent mic, too short, or no words).
    Silent { device: Option<String> },
    NotRecording,
    Denied(VoicePermission),
    /// No on-device recognition for this language.
    Unavailable { locale: String },
    Failed(String),
    /// Recording stopped and there was nowhere to write it.
    Discarded,
}

impl VoiceOutcome {
    /// What the user sees (a short toast).
    pub fn message(&self, t: &PlatformText) -> String {
        match self {
            VoiceOutcome::Saved { .. } => "Voice note saved".into(),
            VoiceOutcome::Silent { device } => {
                let mic = device.as_ref().map(|d| format!("“{d}”")).unwrap_or_else(|| "the microphone".into());
                format!("Didn't catch that. Nothing was heard from {mic}. Check {}.", t.sound_input)
            }
            VoiceOutcome::NotRecording => format!("Start recording first ({})", t.record_hotkey),
            VoiceOutcome::Denied(VoicePermission::Microphone) => {
                format!("Context Capsule can't use the microphone. Turn it on in {}.", t.mic_privacy)
            }
            VoiceOutcome::Denied(VoicePermission::Speech) => {
                format!("Context Capsule can't use speech recognition. Turn it on in {}.", t.speech_privacy)
            }
            VoiceOutcome::Unavailable { locale } => format!(
                "On-device speech isn't available for {locale}, so voice notes are off (nothing is sent to {}). \
                 Set \"voice_locale\" in {}, or {}.",
                t.cloud, t.config_file, t.add_language
            ),
            VoiceOutcome::Failed(why) => format!("Voice note failed: {why}"),
            VoiceOutcome::Discarded => "Voice note not saved: recording had stopped".into(),
        }
    }

    /// One log line: counts and durations only, never the words.
    pub fn log_summary(&self) -> String {
        match self {
            VoiceOutcome::Saved { chars, seconds } => format!("voice note saved: {chars} chars, {seconds:.1} s"),
            VoiceOutcome::Silent { .. } => "voice note: nothing heard".into(),
            VoiceOutcome::NotRecording => "voice note: not recording".into(),
            VoiceOutcome::Denied(p) => format!("voice note: {} permission denied", p.as_str()),
            VoiceOutcome::Unavailable { .. } => "voice note: on-device recognition unavailable".into(),
            VoiceOutcome::Failed(_) => "voice note: failed".into(),
            VoiceOutcome::Discarded => "voice note: discarded".into(),
        }
    }
}

pub struct Voice;

impl Voice {
    /// Longest note; the mic closes on its own after this.
    pub const MAX_DURATION: Duration = Duration::from_secs(120);
    /// Shorter than this is a double press, not speech.
    pub const MIN_SECONDS: f64 = 0.35;
    /// A clip whose loudest sample stays under this is a dead or muted mic.
    pub const SILENT_PEAK: f32 = 0.01;

    pub fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |p, s| p.max(s.abs()))
    }

    /// Nothing worth writing: silent, too short, or no words came back.
    pub fn is_empty(clip: &VoiceClip) -> bool {
        clip.peak < Self::SILENT_PEAK || clip.seconds < Self::MIN_SECONDS || clip.text.trim().is_empty()
    }

    /// The events.jsonl line for a note: same redaction and cap as screen text.
    #[allow(clippy::too_many_arguments)]
    pub fn event(text: &str, kind: &str, src: &str, start: SystemTime, end: SystemTime, app: &str, bundle: &str, title: &str) -> Event {
        Event {
            t: clock::iso(start),
            end: clock::iso(end),
            app: app.into(),
            bundle: bundle.into(),
            title: Redactor::clean(title),
            src: src.into(),
            text: Redactor::clean(text.trim()),
            kind: kind.into(),
        }
    }
}

/// An engine's answer to `prepare` or `finish`.
#[derive(Clone, Debug, PartialEq)]
pub enum VoiceReply {
    /// None = ready to start.
    Prepared(Option<VoiceOutcome>),
    Finished(Result<VoiceClip, String>),
}

/// Hands one reply back to [`VoiceNotes`], from any thread. Replies for an older note are dropped.
pub struct Replier {
    token: u64,
    tx: Sender<(u64, VoiceReply)>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Replier {
    pub fn send(self, reply: VoiceReply) {
        let _ = self.tx.send((self.token, reply));
        (self.wake)();
    }
}

/// The mic and the on-device recognizer (platform code; tests use a fake). Replies may come later,
/// from any thread, through the [`Replier`]; the owner then calls [`VoiceNotes::pump`].
pub trait VoiceEngine {
    /// Check (or ask for) microphone + speech permission and on-device support for `locale`.
    /// Reply `Prepared(None)` when ready, else the problem (`Denied`, `Unavailable`, `Failed`).
    fn prepare(&mut self, locale: &str, reply: Replier);
    /// Open the mic and start recognizing. Err if the mic cannot start.
    fn start(&mut self) -> Result<(), String>;
    /// Close the mic now, then reply `Finished` with the final transcript (give up after ~10 s).
    fn finish(&mut self, reply: Replier);
    /// Close the mic and drop everything.
    fn cancel(&mut self);
    /// Name of the input device, for "nothing heard" messages.
    fn device_name(&self) -> Option<String>;
}

impl<T: VoiceEngine + ?Sized> VoiceEngine for Box<T> {
    fn prepare(&mut self, locale: &str, reply: Replier) {
        (**self).prepare(locale, reply)
    }
    fn start(&mut self) -> Result<(), String> {
        (**self).start()
    }
    fn finish(&mut self, reply: Replier) {
        (**self).finish(reply)
    }
    fn cancel(&mut self) {
        (**self).cancel()
    }
    fn device_name(&self) -> Option<String> {
        (**self).device_name()
    }
}

/// Where a note goes and what it is about, captured when the user starts talking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceTarget {
    pub app: String,
    pub bundle: String,
    pub title: String,
    /// The session the note belongs to (it is written there even if recording stopped meanwhile).
    pub session: String,
}

/// What happened, for the owner to act on (drained with `take_signals`).
#[derive(Clone, Debug, PartialEq)]
pub enum VoiceSignal {
    /// The mic is open (true) or closed (false). Always ends with false.
    Indicator(bool),
    Outcome(VoiceOutcome),
    /// Write this note into `session`'s events.jsonl.
    Save { session: String, event: Event },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePhase {
    Idle,
    Preparing,
    Listening,
    Finishing,
}

/// Press to talk, press again to stop. Guarantees the indicator turns off on every way out: saved,
/// nothing heard, denied, unavailable, error, timeout, recording stopped. Single-threaded.
pub struct VoiceNotes<E: VoiceEngine> {
    engine: E,
    phase: VoicePhase,
    target: Option<VoiceTarget>,
    started_at: Option<SystemTime>,
    ended_at: Option<SystemTime>,
    deadline: Option<SystemTime>,
    token: u64, // a newer note makes older replies no-ops
    tx: Sender<(u64, VoiceReply)>,
    rx: Receiver<(u64, VoiceReply)>,
    wake: Arc<dyn Fn() + Send + Sync>,
    now: Box<dyn Fn() -> SystemTime>,
    signals: Vec<VoiceSignal>,
}

impl<E: VoiceEngine> VoiceNotes<E> {
    pub fn new(engine: E) -> Self {
        let (tx, rx) = channel();
        VoiceNotes {
            engine,
            phase: VoicePhase::Idle,
            target: None,
            started_at: None,
            ended_at: None,
            deadline: None,
            token: 0,
            tx,
            rx,
            wake: Arc::new(|| {}),
            now: Box::new(SystemTime::now),
            signals: vec![],
        }
    }

    /// The clock (tests move time by hand).
    pub fn set_clock(&mut self, now: Box<dyn Fn() -> SystemTime>) {
        self.now = now;
    }
    /// Called (on the engine's thread) after each reply, so the owner can `pump` promptly.
    pub fn set_wake(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        self.wake = wake;
    }

    pub fn phase(&self) -> VoicePhase {
        self.phase
    }
    pub fn is_active(&self) -> bool {
        self.phase != VoicePhase::Idle
    }
    pub fn engine(&self) -> &E {
        &self.engine
    }
    pub fn engine_mut(&mut self) -> &mut E {
        &mut self.engine
    }
    pub fn take_signals(&mut self) -> Vec<VoiceSignal> {
        std::mem::take(&mut self.signals)
    }

    /// The hotkey / menu item. `target` None = not recording.
    pub fn toggle(&mut self, locale: &str, target: Option<VoiceTarget>) {
        match self.phase {
            VoicePhase::Idle => self.begin(locale, target),
            VoicePhase::Listening => self.finish(),
            VoicePhase::Preparing | VoicePhase::Finishing => {} // a press while busy does nothing
        }
        self.pump();
    }

    /// Recording is stopping: finish what was said (it still goes into that session).
    pub fn recording_will_stop(&mut self) {
        match self.phase {
            VoicePhase::Listening => self.finish(),
            VoicePhase::Preparing => self.end(VoiceOutcome::Discarded, true),
            VoicePhase::Idle | VoicePhase::Finishing => {}
        }
        self.pump();
    }

    /// The 2-minute limit: call now and then (the recorder does every 0.5 s).
    pub fn check_timeout(&mut self) {
        if self.phase == VoicePhase::Listening && self.deadline.is_some_and(|d| (self.now)() >= d) {
            self.finish();
            self.pump();
        }
    }

    /// Handle the engine's replies that have arrived.
    pub fn pump(&mut self) {
        while let Ok((token, reply)) = self.rx.try_recv() {
            if token != self.token {
                continue;
            }
            match reply {
                VoiceReply::Prepared(problem) => self.prepared(problem),
                VoiceReply::Finished(result) => self.finished(result),
            }
        }
    }

    fn replier(&self) -> Replier {
        Replier { token: self.token, tx: self.tx.clone(), wake: self.wake.clone() }
    }

    fn begin(&mut self, locale: &str, target: Option<VoiceTarget>) {
        let Some(target) = target else { return self.report(VoiceOutcome::NotRecording) };
        self.token += 1;
        self.phase = VoicePhase::Preparing;
        self.target = Some(target);
        let reply = self.replier();
        self.engine.prepare(locale, reply);
    }

    fn prepared(&mut self, problem: Option<VoiceOutcome>) {
        if self.phase != VoicePhase::Preparing {
            return;
        }
        if let Some(problem) = problem {
            return self.end(problem, false);
        }
        if let Err(e) = self.engine.start() {
            return self.end(VoiceOutcome::Failed(e), true);
        }
        let now = (self.now)();
        self.phase = VoicePhase::Listening;
        self.started_at = Some(now);
        self.deadline = Some(now + Voice::MAX_DURATION);
        self.signals.push(VoiceSignal::Indicator(true));
    }

    fn finish(&mut self) {
        if self.phase != VoicePhase::Listening {
            return;
        }
        self.phase = VoicePhase::Finishing;
        self.deadline = None;
        self.signals.push(VoiceSignal::Indicator(false)); // the mic is closing now
        self.ended_at = Some((self.now)());
        let reply = self.replier();
        self.engine.finish(reply);
    }

    fn finished(&mut self, result: Result<VoiceClip, String>) {
        if self.phase != VoicePhase::Finishing {
            return;
        }
        let clip = match result {
            Err(e) => return self.end(VoiceOutcome::Failed(e), false),
            Ok(clip) => clip,
        };
        if Voice::is_empty(&clip) {
            let device = self.engine.device_name();
            return self.end(VoiceOutcome::Silent { device }, false);
        }
        let Some(target) = self.target.clone() else { return self.end(VoiceOutcome::Discarded, false) };
        let ended = self.ended_at.unwrap_or_else(|| (self.now)());
        let started = self.started_at.unwrap_or(ended);
        let event = Voice::event(&clip.text, Event::VOICE, "voice", started, ended, &target.app, &target.bundle, &target.title);
        let chars = char_count(&event.text);
        self.signals.push(VoiceSignal::Save { session: target.session, event });
        self.end(VoiceOutcome::Saved { chars, seconds: clip.seconds }, false);
    }

    /// The one way out. Always turns the indicator off.
    fn end(&mut self, outcome: VoiceOutcome, cancel_engine: bool) {
        if cancel_engine {
            self.engine.cancel();
        }
        self.deadline = None;
        self.phase = VoicePhase::Idle;
        self.target = None;
        self.started_at = None;
        self.ended_at = None;
        self.signals.push(VoiceSignal::Indicator(false));
        self.report(outcome);
    }

    fn report(&mut self, outcome: VoiceOutcome) {
        eprintln!("Context Capsule: {}", outcome.log_summary());
        self.signals.push(VoiceSignal::Outcome(outcome));
    }
}
