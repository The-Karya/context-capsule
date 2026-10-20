// context-capsule — core: engine tests with a fake Platform and a fake VoiceEngine (no OS, no threads).

use context_capsule_core::clock;
use context_capsule_core::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn t0() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_792_404_063) // 2026-10-19T10:01:03Z
}

fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(x, y, w, h)
}

// MARK: fakes

#[derive(Default)]
struct Fake {
    presented: usize,
    stops: usize,
    picked: Option<PickedContent>,
    frame: Option<Frame>,
    front: Option<Front>,
    ui_text: Option<String>,
    ui_title: Option<String>,
    ocr_text: String,
    ocr_images: Vec<Image>,
    changed_at: Option<SystemTime>,
    indicators: Vec<Indicator>,
    toasts: Vec<String>,
    alerts: Vec<(String, String)>,
    location: Option<String>,
    /// Names given to `project_changed` (None: "not sure yet").
    project_changes: Vec<Option<String>>,
    status: Option<Status>,
    /// More shared screens (source ids 1, 2…) and their frames.
    more_picks: Vec<PickedContent>,
    frames_of: Vec<(u32, Frame)>,
    added: usize,
    reminded: usize,
    /// The mouse, the platform's own activity time (hooks), and the text of the window under it (None: OCR).
    pointer: Option<(f64, f64)>,
    pointer_activity: Option<SystemTime>,
    pointer_text: Option<String>,
    /// What ui_text was asked for: true = the window under the mouse.
    ui_asked: Vec<bool>,
}

impl Platform for Fake {
    fn present_picker(&mut self) {
        self.presented += 1;
    }
    fn stop_capture(&mut self) {
        self.stops += 1;
    }
    fn picked(&self) -> Vec<PickedContent> {
        self.picked.clone().into_iter().chain(self.more_picks.iter().cloned()).collect()
    }
    fn latest_frame(&self, source: u32) -> Option<Frame> {
        self.frames_of.iter().find(|(s, _)| *s == source).map(|(_, f)| f.clone()).or_else(|| self.frame.clone().filter(|_| source == 0))
    }
    fn add_source(&mut self) {
        self.added += 1;
    }
    fn remind(&mut self) {
        self.reminded += 1;
    }
    fn front(&mut self, is_excluded: &dyn Fn(Option<&str>, &str) -> bool) -> Option<Front> {
        let mut f = self.front.clone()?;
        for w in &mut f.list {
            w.excluded = is_excluded(None, &w.owner);
        }
        Some(f)
    }
    fn ui_text(&mut self, front: &Front) -> Option<String> {
        self.ui_asked.push(front.pointer);
        if front.pointer { self.pointer_text.clone() } else { self.ui_text.clone() }
    }
    fn pointer(&mut self) -> Option<(f64, f64)> {
        self.pointer
    }
    fn pointer_activity(&self) -> Option<SystemTime> {
        self.pointer_activity
    }
    fn pointer_front(&mut self, window: &ListedWindow, front: &Front, is_excluded: &dyn Fn(Option<&str>, &str) -> bool) -> Option<Front> {
        let id = format!("{}.exe", window.owner.to_lowercase());
        let excluded = is_excluded(Some(&id), &window.owner);
        Some(Front {
            app_name: window.owner.clone(),
            app_id: Some(id),
            pid: window.pid,
            window: Some(ListedWindow { excluded, ..window.clone() }),
            list: front.list.clone(),
            ..Default::default()
        })
    }
    fn ui_title(&mut self, front: &Front) -> Option<String> {
        if front.pointer { None } else { self.ui_title.clone() }
    }
    fn ocr(&mut self, image: &Image) -> String {
        self.ocr_images.push(image.clone());
        self.ocr_text.clone()
    }
    fn changed_at(&self) -> Option<SystemTime> {
        self.changed_at
    }
    fn set_indicator(&mut self, indicator: Indicator) {
        self.indicators.push(indicator);
    }
    fn toast(&mut self, text: &str) {
        self.toasts.push(text.into());
    }
    fn alert(&mut self, title: &str, body: &str) {
        self.alerts.push((title.into(), body.into()));
    }
    fn host_name(&self) -> String {
        "test-host".into()
    }
    fn status_changed(&mut self, status: &Status) {
        self.status = Some(status.clone());
    }
    fn location(&mut self, _front: &Front) -> Option<String> {
        self.location.clone()
    }
    fn project_changed(&mut self, project: Option<&Project>) {
        self.project_changes.push(project.map(|p| p.name.clone()));
    }
}

#[derive(Default)]
struct VoiceState {
    problem: Option<VoiceOutcome>,
    hold: bool,
    pending: Option<Replier>,
    mic_open: bool,
    clip: Option<VoiceClip>,
}

struct FakeVoice(Rc<RefCell<VoiceState>>);

impl VoiceEngine for FakeVoice {
    fn prepare(&mut self, _locale: &str, reply: Replier) {
        reply.send(VoiceReply::Prepared(self.0.borrow().problem.clone()));
    }
    fn start(&mut self) -> Result<(), String> {
        self.0.borrow_mut().mic_open = true;
        Ok(())
    }
    fn finish(&mut self, reply: Replier) {
        let mut s = self.0.borrow_mut();
        s.mic_open = false;
        if s.hold {
            s.pending = Some(reply);
        } else {
            let clip = s.clip.clone().unwrap_or(VoiceClip { text: "We keep three tiers".into(), peak: 0.2, seconds: 4.0 });
            drop(s);
            reply.send(VoiceReply::Finished(Ok(clip)));
        }
    }
    fn cancel(&mut self) {
        self.0.borrow_mut().mic_open = false;
    }
    fn device_name(&self) -> Option<String> {
        Some("USB Mic".into())
    }
}

struct Rig {
    engine: Engine<Fake>,
    voice: Rc<RefCell<VoiceState>>,
    clock: Arc<Mutex<SystemTime>>,
    root: PathBuf,
}

impl Rig {
    fn new(name: &str) -> Rig {
        Self::with_control(name, None)
    }

    /// `control`: a command already in the control file before launch.
    fn with_control(name: &str, control: Option<&str>) -> Rig {
        let root = std::env::temp_dir().join(format!("capsule-engine-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        if let Some(c) = control {
            std::fs::write(root.join("control"), c).unwrap();
        }
        let voice = Rc::new(RefCell::new(VoiceState::default()));
        let opts = Options { own_app_id: Some("context-capsule.exe".into()), text: PlatformText::WINDOWS, locale: "en-US".into() };
        let mut engine = Engine::new(Paths::new(&root), Fake::default(), Box::new(FakeVoice(voice.clone())), opts);
        let clock = Arc::new(Mutex::new(t0()));
        let c = clock.clone();
        engine.set_clock(Arc::new(move || *c.lock().unwrap()));
        engine.launch();
        Rig { engine, voice, clock, root }
    }

    fn now(&self) -> SystemTime {
        *self.clock.lock().unwrap()
    }
    fn advance(&self, d: Duration) {
        *self.clock.lock().unwrap() += d;
    }
    fn p(&mut self) -> &mut Fake {
        self.engine.platform_mut()
    }

    fn state(&self) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(self.root.join("state.json")).unwrap()).unwrap()
    }
    fn session_dir(&self, id: &str) -> PathBuf {
        self.root.join("sessions").join(id)
    }
    fn events(&self, id: &str) -> Vec<serde_json::Value> {
        let s = std::fs::read_to_string(self.session_dir(id).join("events.jsonl")).unwrap();
        s.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    /// A 100×50 display (1 px per point), Chrome's window on its left half, a fresh frame.
    fn scene(&mut self) {
        let chrome = ListedWindow { owner: "Google Chrome".into(), name: Some("Pricing — Linear".into()), ..ListedWindow::new(1, 50, r(0.0, 0.0, 50.0, 50.0)) };
        let private = ListedWindow { owner: "WhatsApp".into(), layer: 3, ..ListedWindow::new(2, 60, r(0.0, 0.0, 10.0, 10.0)) };
        let arrived = self.now() + Duration::from_secs(1);
        let p = self.p();
        p.picked = Some(PickedContent::display(r(0.0, 0.0, 100.0, 50.0)));
        p.front = Some(Front {
            app_name: "Google Chrome".into(),
            app_id: Some("chrome.exe".into()),
            pid: 50,
            window: Some(chrome.clone()),
            list: vec![private, chrome],
            overlay: false,
            pointer: false,
        });
        p.frame = Some(Frame { bgra: vec![200u8; 100 * 50 * 4].into(), width: 100, height: 50, stride: 400, arrived });
        p.ocr_text = "Pricing tiers\nkey sk-proj-abcdefghijklmnop1234567890".into();
    }

    /// Start, pick, and be recording.
    fn record(&mut self) -> String {
        self.engine.toggle_record();
        self.engine.on_pick(PickOutcome::Picked);
        assert!(self.engine.is_recording());
        self.engine.session_id().unwrap().to_string()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// MARK: start / stop

#[test]
fn picker_cancel_sets_last_error() {
    let mut r = Rig::new("cancel");
    assert_eq!(r.state()["recording"], false, "launch writes state.json");
    r.engine.toggle_record();
    assert_eq!(r.engine.platform().presented, 1);
    assert_eq!(r.state()["picking"], true);
    assert_eq!(r.engine.platform().indicators.last(), Some(&Indicator::Picking));
    r.engine.on_pick(PickOutcome::Cancelled);
    let s = r.state();
    assert!(s["picking"] == false && s["recording"] == false && s["last_error"] == "cancelled", "{s}");
    assert_eq!(r.engine.platform().indicators.last(), Some(&Indicator::Idle));

    // The hotkey while the picker is open cancels it too.
    r.engine.toggle_record();
    assert_eq!(r.state()["last_error"], serde_json::Value::Null, "a new start clears the error");
    r.engine.toggle_record();
    assert_eq!(r.state()["last_error"], "cancelled");
    assert_eq!(r.engine.platform().stops, 1);
    // A late pick after cancelling does not start recording.
    r.engine.on_pick(PickOutcome::Picked);
    assert!(!r.engine.is_recording());
    assert_eq!(r.engine.platform().stops, 2);
}

#[test]
fn picked_writes_session_and_state() {
    let mut r = Rig::new("picked");
    let id = r.record();
    assert_eq!(id, clock::session_id(t0()));
    let sj = std::fs::read_to_string(r.session_dir(&id).join("session.json")).unwrap();
    assert_eq!(sj, format!(r#"{{"id":"{id}","started_at":"2026-10-19T10:01:03Z","ended_at":null,"host":"test-host","version":1}}"#));
    assert!(r.session_dir(&id).join("events.jsonl").exists());
    let s = r.state();
    assert!(s["recording"] == true && s["session_id"] == id.as_str() && s["started_at"] == "2026-10-19T10:01:03Z" && s["picking"] == false);
    assert_eq!(s["pid"], std::process::id());
    assert_eq!(r.engine.platform().indicators.last(), Some(&Indicator::Recording));

    r.advance(Duration::from_secs(60));
    r.engine.toggle_record();
    let sj = std::fs::read_to_string(r.session_dir(&id).join("session.json")).unwrap();
    assert!(sj.contains(r#""ended_at":"2026-10-19T10:02:03Z""#), "{sj}");
    let s = r.state();
    assert!(s["recording"] == false && s["session_id"].is_null());
    assert_eq!(r.engine.platform().indicators.last(), Some(&Indicator::Idle));
}

#[test]
fn capture_failures_end_the_session() {
    let mut r = Rig::new("failures");
    r.record();
    r.engine.on_pick(PickOutcome::Stopped(None));
    assert!(!r.engine.is_recording());
    assert_eq!(r.state()["last_error"], serde_json::Value::Null, "user stopped sharing: no error");
    r.record();
    r.engine.on_pick(PickOutcome::Stopped(Some("display unplugged".into())));
    assert_eq!(r.state()["last_error"], "capture: display unplugged");
    r.engine.toggle_record();
    r.engine.on_pick(PickOutcome::Failed("picker: busy".into()));
    let s = r.state();
    assert!(s["last_error"] == "picker: busy" && s["recording"] == false && s["picking"] == false);
}

// MARK: sampling

#[test]
fn tick_writes_cropped_masked_ocr_event() {
    let mut r = Rig::new("ocr");
    r.scene();
    let id = r.record();
    r.advance(Duration::from_secs(3));
    r.engine.tick();
    let images = &r.engine.platform().ocr_images;
    assert_eq!(images.len(), 1);
    let img = &images[0];
    assert_eq!((img.width, img.height), (50, 50), "cropped to the front window");
    assert_eq!(img.pixel(0, 0), [0, 0, 0, 255], "private window above it painted black");
    assert_eq!(img.pixel(11, 11), [0, 0, 0, 255], "mask grown by 2 px");
    assert_eq!(img.pixel(12, 12), [200, 200, 200, 200], "rest untouched");
    let ev = r.events(&id);
    assert_eq!(ev.len(), 1);
    let line = std::fs::read_to_string(r.session_dir(&id).join("events.jsonl")).unwrap();
    assert_eq!(line, "{\"t\":\"2026-10-19T10:01:06Z\",\"end\":\"2026-10-19T10:01:06Z\",\"app\":\"Google Chrome\",\"bundle\":\"chrome.exe\",\"title\":\"Pricing — Linear\",\"src\":\"ocr\",\"text\":\"Pricing tiers\\nkey [redacted]\"}\n");
    let s = r.state();
    assert!(s["events"] == 1 && s["skip"].is_null());
}

#[test]
fn tick_prefers_accessibility_text() {
    let mut r = Rig::new("ax");
    r.scene();
    r.p().ui_text = Some("Real words here. ".repeat(10));
    r.p().ui_title = Some("From UIA".into());
    let id = r.record();
    r.engine.tick();
    assert!(r.engine.platform().ocr_images.is_empty(), "no OCR when accessibility text is useful");
    let ev = r.events(&id);
    assert!(ev[0]["src"] == "ax" && ev[0]["title"] == "From UIA");
    // Too little accessibility text: OCR instead.
    r.p().ui_text = Some("File Edit View".into());
    r.engine.tick();
    assert_eq!(r.events(&id)[1]["src"], "ocr");
}

#[test]
fn skips_write_nothing() {
    type Setup = fn(&mut Rig);
    let cases: [(&str, Setup, Skip); 7] = [
        ("private", |r| r.p().front.as_mut().unwrap().app_name = "1Password".into(), Skip::PrivateApp),
        ("own", |r| r.p().front.as_mut().unwrap().app_id = Some("context-capsule.exe".into()), Skip::PrivateApp),
        ("overlay", |r| r.p().front.as_mut().unwrap().overlay = true, Skip::Overlay),
        ("stale", |r| { let t = r.now(); r.p().frame.as_mut().unwrap().arrived = t - Duration::from_secs(5); }, Skip::WaitingFrame),
        ("switched", |r| { let t = r.now() + Duration::from_secs(2); r.p().changed_at = Some(t); }, Skip::WaitingFrame),
        ("not-picked", |r| r.p().picked = None, Skip::NotPicked),
        ("other-display", |g| g.p().picked = Some(PickedContent::display(r(200.0, 0.0, 100.0, 50.0))), Skip::NotPicked),
    ];
    for (name, setup, why) in cases {
        let mut rig = Rig::new(&format!("skip-{name}"));
        rig.scene();
        let id = rig.record();
        setup(&mut rig);
        rig.advance(Duration::from_secs(3));
        rig.engine.tick();
        assert!(rig.events(&id).is_empty(), "{name}: nothing written");
        assert!(rig.engine.platform().ocr_images.is_empty(), "{name}: nothing read");
        let s = rig.state();
        assert!(s["skip"] == why.as_str() && s["events"] == 0, "{name}: {s}");
    }
    let mut rig = Rig::new("skip-no-window");
    rig.scene();
    rig.record();
    rig.p().front.as_mut().unwrap().window = None;
    rig.engine.tick();
    assert_eq!(rig.engine.last_skip(), Some(Skip::NoWindow));
    rig.p().front = None;
    rig.engine.tick();
    assert_eq!(rig.engine.last_skip(), Some(Skip::NoWindow));
    rig.scene();
    rig.p().picked = Some(PickedContent::display(Rect::ZERO));
    rig.engine.tick();
    assert_eq!(rig.engine.last_skip(), Some(Skip::UnknownDisplay));
}

#[test]
fn overlay_makes_older_frames_stale() {
    let mut r = Rig::new("overlay-stale");
    r.scene();
    let id = r.record();
    r.advance(Duration::from_secs(3));
    r.p().front.as_mut().unwrap().overlay = true;
    r.engine.tick();
    r.p().front.as_mut().unwrap().overlay = false;
    r.advance(Duration::from_secs(3));
    r.engine.tick();
    assert_eq!(r.engine.last_skip(), Some(Skip::WaitingFrame), "the frame is from before the overview");
    let t = r.now() + Duration::from_millis(200);
    r.p().frame.as_mut().unwrap().arrived = t;
    r.engine.tick();
    assert_eq!(r.events(&id).len(), 1);
}

#[test]
fn same_sample_extends() {
    let mut r = Rig::new("dedupe");
    r.scene();
    let id = r.record();
    for _ in 0..3 {
        r.advance(Duration::from_secs(3));
        r.engine.tick();
    }
    let ev = r.events(&id);
    assert!(ev.len() == 1 && ev[0]["t"] == "2026-10-19T10:01:06Z" && ev[0]["end"] == "2026-10-19T10:01:12Z", "{ev:?}");
    // A skip in between breaks the run.
    r.p().front.as_mut().unwrap().overlay = true;
    r.engine.tick();
    r.p().front.as_mut().unwrap().overlay = false;
    let t = r.now() + Duration::from_secs(1);
    r.p().frame.as_mut().unwrap().arrived = t;
    r.advance(Duration::from_secs(3));
    r.engine.tick();
    assert_eq!(r.events(&id).len(), 2);
    assert_eq!(r.state()["events"], 2);
}

#[test]
fn user_exclude_list_from_config() {
    let mut r = Rig::new("config");
    std::fs::write(r.root.join("config.json"), r#"{"exclude_apps": ["chrome.exe"]}"#).unwrap();
    r.scene();
    let id = r.record();
    r.engine.tick();
    assert!(r.events(&id).is_empty());
    assert_eq!(r.engine.last_skip(), Some(Skip::PrivateApp));
}

// MARK: control file

#[test]
fn control_start_stop_note() {
    let mut r = Rig::with_control("control", Some(r#"{"command":"start","at":1}"#));
    r.engine.poll_control();
    assert_eq!(r.engine.platform().presented, 0, "a command from before launch is ignored");

    std::fs::write(r.root.join("control"), r#"{"command":"start","at":2}"#).unwrap();
    r.engine.poll_control();
    assert!(r.engine.platform().presented == 1 && r.state()["picking"] == true);
    r.engine.poll_control();
    assert_eq!(r.engine.platform().presented, 1, "each `at` acted on once");
    r.engine.on_pick(PickOutcome::Picked);
    let id = r.engine.session_id().unwrap().to_string();

    std::fs::write(r.root.join("control"), r#"{"command":"note","text":"  decided X\npassword: hunter2 ","at":3}"#).unwrap();
    r.engine.poll_control();
    let line = std::fs::read_to_string(r.session_dir(&id).join("events.jsonl")).unwrap();
    assert_eq!(line, "{\"t\":\"2026-10-19T10:01:03Z\",\"end\":\"2026-10-19T10:01:03Z\",\"kind\":\"note\",\"src\":\"typed\",\"app\":\"\",\"bundle\":\"\",\"title\":\"\",\"text\":\"decided X\\n[redacted]\"}\n");
    let s = r.state();
    assert!(s["note_at"] == 3 && s["events"] == 1, "{s}");

    std::fs::write(r.root.join("control"), r#"{"command":"stop","at":4}"#).unwrap();
    r.engine.poll_control();
    assert!(!r.engine.is_recording() && r.state()["recording"] == false);
    assert_eq!(r.state()["events"], 1, "after a stop, state describes the ended session");

    std::fs::write(r.root.join("control"), r#"{"command":"note","text":"late","at":5}"#).unwrap();
    r.engine.poll_control();
    assert_eq!(r.state()["note_at"], 3, "a note while not recording is ignored");
}

#[test]
fn run_loop_handles_messages() {
    let mut r = Rig::new("run");
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(Msg::ToggleRecord).unwrap();
    tx.send(Msg::Pick(PickOutcome::Picked)).unwrap();
    tx.send(Msg::Quit).unwrap();
    r.engine.run(rx);
    let s = r.state();
    assert!(s["recording"] == false && s["session_id"].is_null());
    assert!(std::fs::read_dir(r.root.join("sessions")).unwrap().count() == 1, "one session was made and ended");
}

// MARK: empty sessions

#[test]
fn empty_session_says_why() {
    let mut r = Rig::new("empty");
    r.scene();
    r.record();
    r.p().front.as_mut().unwrap().app_name = "1Password".into();
    r.engine.tick();
    r.engine.tick();
    r.p().front.as_mut().unwrap().app_name = "Google Chrome".into();
    r.p().picked = None;
    r.engine.tick();
    r.engine.toggle_record();
    let alerts = &r.engine.platform().alerts;
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].0, "Nothing was saved");
    assert!(alerts[0].1.starts_with("Every few seconds, a private app is in front (not recorded).\n\n"), "{}", alerts[0].1);

    // Too short to sample.
    r.record();
    r.engine.toggle_record();
    assert!(r.engine.platform().alerts[1].1.starts_with("The session was too short to take a sample."));

    // A session with an event: no alert.
    r.scene();
    r.record();
    r.engine.tick();
    r.engine.toggle_record();
    assert_eq!(r.engine.platform().alerts.len(), 2);
}

// MARK: voice notes through the engine

#[test]
fn voice_needs_recording() {
    let mut r = Rig::new("voice-idle");
    r.engine.toggle_voice();
    assert_eq!(r.engine.platform().toasts, ["Start contextualizing first (Alt+C)"]);
    assert!(!r.engine.platform().indicators.contains(&Indicator::Listening));
    assert!(!r.voice.borrow().mic_open);
}

#[test]
fn voice_note_saved_and_indicator_resets() {
    let mut r = Rig::new("voice-saved");
    r.scene();
    let id = r.record();
    r.engine.toggle_voice();
    assert!(r.engine.is_listening() && r.voice.borrow().mic_open);
    assert_eq!(r.engine.platform().indicators.last(), Some(&Indicator::Listening));
    assert_eq!(r.state()["listening"], true);
    r.advance(Duration::from_secs(4));
    r.engine.toggle_voice();
    assert!(!r.engine.is_listening() && !r.voice.borrow().mic_open);
    assert_eq!(r.engine.platform().indicators.last(), Some(&Indicator::Recording));
    assert!(r.engine.platform().toasts.is_empty(), "a saved note is a sound, not a message");
    let s = r.state();
    assert!(s["listening"] == false && s["events"] == 1);
    let ev = r.events(&id);
    assert!(ev[0]["kind"] == "voice" && ev[0]["app"] == "Google Chrome" && ev[0]["bundle"] == "chrome.exe" && ev[0]["title"] == "Pricing — Linear");
    assert!(ev[0]["t"] == "2026-10-19T10:01:03Z" && ev[0]["end"] == "2026-10-19T10:01:07Z");
}

#[test]
fn voice_note_about_a_private_app_has_no_context() {
    let mut r = Rig::new("voice-private");
    r.scene();
    let id = r.record();
    r.p().front.as_mut().unwrap().app_name = "Signal".into();
    r.engine.toggle_voice();
    r.engine.toggle_voice();
    let ev = r.events(&id);
    assert!(ev[0]["app"] == "" && ev[0]["bundle"] == "" && ev[0]["title"] == "");
}

#[test]
fn voice_denied_resets() {
    let mut r = Rig::new("voice-denied");
    r.voice.borrow_mut().problem = Some(VoiceOutcome::Denied(VoicePermission::Microphone));
    r.record();
    r.engine.toggle_voice();
    assert!(!r.engine.is_listening());
    assert!(!r.engine.platform().indicators.contains(&Indicator::Listening));
    assert_eq!(r.engine.platform().toasts, ["Context Capsule can't use the microphone. Turn it on in Settings → Privacy & security → Microphone."]);
}

#[test]
fn voice_silent_resets() {
    let mut r = Rig::new("voice-silent");
    r.voice.borrow_mut().clip = Some(VoiceClip { text: "".into(), peak: 0.0, seconds: 2.0 });
    let id = r.record();
    r.engine.toggle_voice();
    r.engine.toggle_voice();
    assert!(!r.engine.is_listening() && r.state()["listening"] == false);
    assert_eq!(r.engine.platform().indicators.last(), Some(&Indicator::Recording));
    assert!(r.engine.platform().toasts[0].contains("“USB Mic”"));
    assert!(r.events(&id).is_empty());
}

#[test]
fn voice_timeout_resets() {
    let mut r = Rig::new("voice-timeout");
    let id = r.record();
    r.engine.toggle_voice();
    r.advance(Duration::from_secs(121));
    r.engine.pump_voice();
    assert!(!r.engine.is_listening() && r.state()["listening"] == false);
    assert_eq!(r.events(&id).len(), 1, "the 2-minute limit saves what was said");
}

#[test]
fn stopping_mid_note_saves_it_and_resets() {
    let mut r = Rig::new("voice-stop");
    let id = r.record();
    r.engine.toggle_voice();
    r.engine.stop(); // Claude (the control file) or quitting stops mid-note
    assert!(!r.engine.is_listening() && !r.engine.is_recording());
    assert_eq!(r.engine.platform().indicators.last(), Some(&Indicator::Idle));
    assert_eq!(r.events(&id).len(), 1, "finished and saved into the session");
    assert!(r.engine.platform().alerts.is_empty(), "not an empty session");
    assert_eq!(r.state()["listening"], false);
}

#[test]
fn record_key_waits_while_a_voice_note_listens() {
    let mut r = Rig::new("voice-busy");
    let id = r.record();
    r.engine.toggle_voice();
    r.engine.toggle_record();
    assert!(r.engine.is_recording() && r.engine.is_listening(), "neither recording nor the note ends");
    assert_eq!(
        r.engine.platform().toasts,
        ["Finish the voice note first: press Alt+V. Then Alt+C stops contextualizing."]
    );
    r.engine.toggle_voice(); // the note ends
    r.engine.toggle_record(); // now recording stops
    assert!(!r.engine.is_recording());
    assert_eq!(r.events(&id).len(), 1);
}

#[test]
fn app_log_has_reasons_and_counts_never_words() {
    let mut r = Rig::new("applog");
    r.scene();
    r.engine.toggle_record();
    r.engine.on_pick(PickOutcome::Cancelled);
    let id = r.record();
    r.engine.tick();
    r.p().front = None;
    r.engine.tick();
    r.engine.toggle_voice();
    r.engine.toggle_voice();
    r.engine.toggle_record();
    let id2 = r.record();
    r.engine.on_pick(PickOutcome::Stopped(None));
    std::fs::write(r.root.join("control"), r#"{"command":"start","at":5}"#).unwrap();
    r.engine.poll_control();
    r.engine.on_pick(PickOutcome::Picked);
    std::fs::write(r.root.join("control"), r#"{"command":"stop","at":6}"#).unwrap();
    r.engine.poll_control();
    r.record();
    r.engine.shutdown();

    let log = std::fs::read_to_string(r.root.join("app.log")).unwrap();
    let lines: Vec<&str> = log.lines().map(|l| l.split_once(' ').unwrap().1).collect();
    assert_eq!(lines[0], "picker: cancelled");
    assert_eq!(lines[1], format!("recording started: {id}, picked a screen"));
    assert_eq!(lines[2], "voice note saved: 19 chars, 4.0 s");
    assert_eq!(lines[3], "stop: hotkey or menu");
    assert_eq!(lines[4], format!("recording ended: {id}, 2 events, skipped: no_window 1"));
    assert_eq!(lines[5], format!("recording started: {id2}, picked a screen"));
    assert_eq!(lines[6], "stop: capture closed (sharing was stopped from the system)");
    assert!(lines.contains(&"start: control file (Claude / MCP)"));
    assert!(lines.contains(&"stop: control file (Claude / MCP)"));
    assert_eq!(lines[lines.len() - 2], "stop: quit");
    for secret in ["Pricing", "Linear", "three tiers", "sk-proj"] {
        assert!(!log.contains(secret), "no screen text, titles or words in app.log: {secret}");
    }
}

#[test]
fn late_transcript_goes_to_its_session() {
    let mut r = Rig::new("voice-late");
    r.voice.borrow_mut().hold = true;
    let id = r.record();
    r.engine.toggle_voice();
    r.engine.stop(); // the mic closes; the transcript is still coming
    assert!(!r.engine.is_listening());
    assert_eq!(r.engine.voice_phase(), VoicePhase::Finishing);
    let reply = r.voice.borrow_mut().pending.take().unwrap();
    reply.send(VoiceReply::Finished(Ok(VoiceClip { text: "late words".into(), peak: 0.3, seconds: 2.0 })));
    r.engine.handle(Msg::VoiceReady);
    assert_eq!(r.engine.voice_phase(), VoicePhase::Idle);
    let ev = r.events(&id);
    assert!(ev.len() == 1 && ev[0]["text"] == "late words", "{ev:?}");
    assert_eq!(r.engine.platform().indicators.last(), Some(&Indicator::Idle));
}

// MARK: projects

const PROJECTS: &str = r#"{"projects":[
  {"id":"linear-work","name":"Linear work","match":["linear.app/karya"]},
  {"id":"report","name":"Client report","match":["acme"]}
]}"#;

impl Rig {
    fn projects(&self, json: &str) {
        std::fs::write(self.root.join("projects.json"), json).unwrap();
    }
    fn changes(&self) -> Vec<Option<String>> {
        self.engine.platform().project_changes.clone()
    }
    fn control(&mut self, json: &str) {
        std::fs::write(self.root.join("control"), json).unwrap();
        self.engine.poll_control();
    }
    fn title(&mut self, title: &str) {
        self.p().ui_title = Some(title.into());
    }
}

fn some(s: &str) -> Option<String> {
    Some(s.into())
}

#[test]
fn auto_project_from_where_and_title() {
    let mut r = Rig::new("project-auto");
    r.projects(PROJECTS);
    r.scene();
    r.p().location = some("https://linear.app/karya/issue/KAR-12?token=sk-abc#c");
    let id = r.record();
    assert!(r.state()["project"].is_null(), "nothing sampled yet");
    r.advance(Duration::from_secs(3));
    r.engine.tick();
    let line = std::fs::read_to_string(r.session_dir(&id).join("events.jsonl")).unwrap();
    assert_eq!(line, "{\"t\":\"2026-10-19T10:01:06Z\",\"end\":\"2026-10-19T10:01:06Z\",\"app\":\"Google Chrome\",\"bundle\":\"chrome.exe\",\"title\":\"Pricing — Linear\",\"where\":\"https://linear.app/karya/issue/KAR-12\",\"project\":\"linear-work\",\"project_by\":\"auto\",\"src\":\"ocr\",\"text\":\"Pricing tiers\\nkey [redacted]\"}\n");
    let s = r.state();
    assert!(s["project"] == "linear-work" && s["project_by"] == "auto", "{s}");
    assert_eq!(r.changes(), [some("Linear work")], "said once");
    r.engine.tick();
    assert_eq!(r.changes().len(), 1, "same project: no new message");
    assert_eq!(r.events(&id).len(), 1, "same sample extends");

    // A window with no clue: unassigned, but the current project stays (display only).
    r.p().location = None;
    r.title("Inbox");
    r.engine.tick();
    let ev = r.events(&id);
    assert!(ev[1].get("project").is_none() && ev[1].get("project_by").is_none() && ev[1].get("where").is_none(), "{:?}", ev[1]);
    assert_eq!(r.state()["project"], "linear-work");
    assert_eq!(r.changes().len(), 1);

    // Another project's clue in the title.
    r.title("ACME Q3 — Docs");
    r.engine.tick();
    assert_eq!(r.events(&id)[2]["project"], "report");
    assert_eq!(r.changes(), [some("Linear work"), some("Client report")]);
    let status = r.engine.platform().status.clone().unwrap();
    assert_eq!(status.project.as_deref(), Some("report"));
    assert_eq!(status.project_title(), "Project: Auto-detect (Client report)");
    assert_eq!(status.projects.len(), 2);
    assert_eq!(status.new_project_clue.as_deref(), Some("ACME Q3"));
}

#[test]
fn pinned_project_wins_and_auto_returns() {
    let mut r = Rig::new("project-pinned");
    r.projects(PROJECTS);
    r.scene();
    r.p().location = some("https://linear.app/karya/x");
    let id = r.record();
    r.control(r#"{"command":"project","project":"report","at":1}"#);
    let s = r.state();
    assert!(s["project"] == "report" && s["project_by"] == "you", "{s}");
    assert_eq!(r.changes(), [some("Client report")]);
    r.engine.tick();
    let ev = r.events(&id);
    assert!(ev[0]["project"] == "report" && ev[0]["project_by"] == "you", "the pick beats the clue {:?}", ev[0]);
    let status = r.engine.platform().status.clone().unwrap();
    assert_eq!(status.pinned.as_deref(), Some("report"));
    assert_eq!(status.project_title(), "Project: Client report");

    // Back to auto-detect: the picked project is dropped, then the clue finds one.
    r.control(r#"{"command":"project","project":"auto","at":2}"#);
    assert!(r.state()["project"].is_null() && r.state()["project_by"].is_null());
    assert_eq!(r.changes(), [some("Client report"), None]);
    r.engine.tick();
    assert_eq!(r.events(&id)[1]["project_by"], "auto");
    assert_eq!(r.changes(), [some("Client report"), None, some("Linear work")]);

    // An unknown id means auto-detect.
    r.engine.handle(Msg::PickProject(some("nope")));
    assert_eq!(r.engine.project_choice().pinned, None);
    assert_eq!(r.state()["project_by"], "auto", "a detected project is kept");
}

#[test]
fn pinned_before_recording() {
    let mut r = Rig::new("project-pin-idle");
    r.projects(PROJECTS);
    r.engine.handle(Msg::PickProject(some("report")));
    assert_eq!(r.state()["project"], "report");
    assert!(r.changes().is_empty(), "no message while not recording");
    r.scene();
    let id = r.record();
    assert!(r.state()["project"] == "report" && r.state()["project_by"] == "you");
    r.engine.tick();
    assert_eq!(r.events(&id)[0]["project"], "report");
    assert!(r.changes().is_empty(), "already the current project at the start");
}

#[test]
fn notes_get_the_current_project() {
    let mut r = Rig::new("project-notes");
    r.projects(PROJECTS);
    r.scene();
    r.p().location = some("https://linear.app/karya/x");
    let id = r.record();
    r.control(r#"{"command":"note","text":"before any sample","at":1}"#);
    r.engine.tick();
    r.control(r#"{"command":"note","text":"decided X","at":2}"#);
    r.engine.toggle_voice();
    r.engine.pin_project(Some("report")); // changed mid-note
    r.engine.toggle_voice();
    let lines: Vec<String> = std::fs::read_to_string(r.session_dir(&id).join("events.jsonl")).unwrap().lines().map(String::from).collect();
    assert_eq!(lines[0], "{\"t\":\"2026-10-19T10:01:03Z\",\"end\":\"2026-10-19T10:01:03Z\",\"kind\":\"note\",\"src\":\"typed\",\"app\":\"\",\"bundle\":\"\",\"title\":\"\",\"text\":\"before any sample\"}");
    assert_eq!(lines[2], "{\"t\":\"2026-10-19T10:01:03Z\",\"end\":\"2026-10-19T10:01:03Z\",\"kind\":\"note\",\"src\":\"typed\",\"app\":\"\",\"bundle\":\"\",\"title\":\"\",\"project\":\"linear-work\",\"project_by\":\"auto\",\"text\":\"decided X\"}");
    let ev = r.events(&id);
    assert!(ev[3]["kind"] == "voice" && ev[3]["project"] == "linear-work" && ev[3]["project_by"] == "auto",
        "a voice note keeps the project it was started in {:?}", ev[3]);
}

#[test]
fn projects_json_reloads_when_it_changes() {
    let mut r = Rig::new("project-reload");
    r.scene();
    let id = r.record();
    r.engine.tick();
    assert!(r.events(&id)[0].get("project").is_none());
    r.projects(PROJECTS);
    let later = SystemTime::now() + Duration::from_secs(5);
    std::fs::File::options().write(true).open(r.root.join("projects.json")).unwrap().set_modified(later).unwrap();
    r.title("Pricing — acme");
    r.engine.tick();
    assert_eq!(r.events(&id)[1]["project"], "report", "picked up without a restart");
    assert_eq!(r.engine.projects().len(), 2);
}

#[test]
fn new_project_from_this_window() {
    let mut r = Rig::new("project-new");
    r.scene();
    r.p().location = some("https://linear.app/karya/issue/KAR-12");
    let id = r.record();
    assert_eq!(r.engine.new_project_clue(), None, "no sample yet");
    r.engine.tick();
    let clue = r.engine.new_project_clue();
    assert_eq!(clue.as_deref(), Some("linear.app/karya/issue"));
    assert_eq!(r.engine.platform().status.as_ref().unwrap().new_project_clue, clue, "the tray knows the clue");
    r.engine.handle(Msg::NewProject { name: "  Karya issues ".into(), clue: clue.clone() });
    let saved = std::fs::read_to_string(r.root.join("projects.json")).unwrap();
    assert_eq!(saved, "{\n  \"projects\" : [\n    {\n      \"id\" : \"karya-issues\",\n      \"match\" : [\n        \"linear.app\\/karya\\/issue\"\n      ],\n      \"name\" : \"Karya issues\"\n    }\n  ]\n}\n");
    assert_eq!(r.engine.project_choice().pinned, None, "with a clue, auto-detect finds it");
    r.advance(Duration::from_secs(3));
    r.engine.tick();
    let ev = r.events(&id);
    assert!(ev[1]["project"] == "karya-issues" && ev[1]["project_by"] == "auto", "{:?}", ev[1]);
    assert_eq!(r.changes(), [some("Karya issues")]);

    // Again with the same name: the clue is not added twice; a new clue is.
    r.engine.handle(Msg::NewProject { name: "Karya Issues".into(), clue: some("LINEAR.APP/karya/issue") });
    r.engine.handle(Msg::NewProject { name: "Karya Issues".into(), clue: some("KAR-") });
    assert_eq!(r.engine.projects()[0].clues, ["linear.app/karya/issue", "KAR-"]);

    // Without a clue: stick to it.
    r.engine.handle(Msg::NewProject { name: "Side quest".into(), clue: None });
    assert_eq!(r.engine.project_choice().pinned.as_deref(), Some("side-quest"));
    assert!(r.state()["project"] == "side-quest" && r.state()["project_by"] == "you");
    // A name with no letters or digits makes nothing.
    assert_eq!(r.engine.new_project("!!!", None), None);
    assert_eq!(r.engine.projects().len(), 2);
}

#[test]
fn project_message_words() {
    let p = Project { id: "r".into(), name: "Client report".into(), clues: vec![] };
    assert_eq!(project_message(Some(&p)), "Project: Client report");
    assert_eq!(project_message(None), "Project: not sure yet");
    let s = Status { new_project_clue: some("acme"), ..Default::default() };
    assert_eq!(s.new_project_prompt(), "Windows with “acme” will count as this project from now on.");
    assert_eq!(Status::default().new_project_prompt(), "Name the project. Pick it from this menu while you work on it.");
    assert_eq!(Status::default().project_title(), "Project: Auto-detect");
    let gone = Status { pinned: some("deleted"), project: some("deleted"), ..Default::default() };
    assert_eq!(gone.project_title(), "Project: deleted", "a pinned project missing from the file shows its id");

    assert_eq!(Status::default().project_items(), [ProjectItem::Auto { checked: true }, ProjectItem::Separator, ProjectItem::New]);
    let two = Status {
        projects: vec![p.clone(), Project { id: "d".into(), name: "Design".into(), clues: vec![] }],
        pinned: some("d"),
        project: some("d"),
        ..Default::default()
    };
    assert_eq!(two.project_items(), [
        ProjectItem::Auto { checked: false },
        ProjectItem::Separator,
        ProjectItem::Project { id: "r".into(), name: "Client report".into(), checked: false },
        ProjectItem::Project { id: "d".into(), name: "Design".into(), checked: true },
        ProjectItem::Separator,
        ProjectItem::New,
    ]);
    assert_eq!(two.project_title(), "Project: Design");
}

// MARK: several shared screens

#[test]
fn several_screens_each_sample_uses_the_one_in_front() {
    let mut g = Rig::new("screens");
    g.scene();
    let id = g.record();
    assert_eq!(g.engine.status().sources, 1);
    g.engine.handle(Msg::AddScreen);
    assert_eq!(g.engine.platform().added, 1, "the picker again");
    // A second screen to the right, with its own frame.
    let arrived = g.now() + Duration::from_secs(1);
    g.p().more_picks = vec![PickedContent::display(r(100.0, 0.0, 100.0, 50.0)).with_source(2)];
    g.p().frames_of = vec![(2, Frame { bgra: vec![90u8; 100 * 50 * 4].into(), width: 100, height: 50, stride: 400, arrived })];
    g.engine.on_pick(PickOutcome::Added(2));
    assert_eq!(g.engine.status().sources, 2);
    assert_eq!(g.engine.status().add_screen_label(), "Add a screen (2 shared)");
    assert!(g.engine.platform().toasts.is_empty(), "adding a screen shows no text");

    // The window in front moves to the second screen: that screen's frame is read.
    let w = g.p().front.as_mut().unwrap();
    w.window.as_mut().unwrap().frame = r(120.0, 0.0, 50.0, 50.0);
    w.list[1].frame = r(120.0, 0.0, 50.0, 50.0);
    g.p().ocr_text = "On the right".into();
    g.advance(Duration::from_secs(3));
    g.engine.tick();
    let img = g.engine.platform().ocr_images.last().unwrap().clone();
    assert_eq!(img.pixel(25, 25), [90, 90, 90, 90], "the second screen's frame, cropped to the window");
    assert_eq!(g.events(&id).last().unwrap()["text"], "On the right");

    // Off both screens: not recorded.
    let w = g.p().front.as_mut().unwrap();
    w.window.as_mut().unwrap().frame = r(400.0, 0.0, 50.0, 50.0);
    g.engine.tick();
    assert_eq!(g.engine.last_skip(), Some(Skip::NotPicked));

    // One stops on its own: recording goes on with the other.
    g.engine.on_pick(PickOutcome::Removed(1));
    assert!(g.engine.is_recording());
    assert_eq!(g.engine.status().sources, 1);
    assert!(g.engine.platform().toasts.is_empty(), "a screen leaving shows no text");
    g.engine.on_pick(PickOutcome::AddCancelled);
    assert!(g.engine.is_recording(), "closing the picker when adding changes nothing");
    // The last one stopping ends the recording.
    g.engine.on_pick(PickOutcome::Stopped(None));
    assert!(!g.engine.is_recording());
    assert_eq!(g.engine.status().sources, 0);
}

#[test]
fn add_screen_only_while_recording_and_up_to_four() {
    let mut r = Rig::new("screens-limit");
    r.engine.handle(Msg::AddScreen);
    assert_eq!(r.engine.platform().added, 0, "not recording");
    r.scene();
    r.record();
    r.engine.on_pick(PickOutcome::Added(MAX_SOURCES));
    r.engine.handle(Msg::AddScreen);
    assert_eq!(r.engine.platform().added, 0, "already 4");
    assert!(!r.engine.status().can_add_screen());
}

// MARK: only what's new

#[test]
fn samples_save_only_new_lines() {
    let mut r = Rig::new("new-lines");
    r.scene();
    r.p().ui_text = Some(format!("{}\nIntro paragraph", "Real words here. ".repeat(10)));
    let id = r.record();
    r.engine.tick();
    r.advance(Duration::from_secs(3));
    r.engine.tick();
    let ev = r.events(&id);
    assert_eq!(ev.len(), 1, "nothing new: the same event, longer");
    assert_eq!(ev[0]["end"], "2026-10-19T10:01:06Z");
    // Scrolled: only the new line is saved.
    r.p().ui_text = Some(format!("{}\nIntro paragraph\nSecond section", "Real words here. ".repeat(10)));
    r.advance(Duration::from_secs(3));
    r.engine.tick();
    let ev = r.events(&id);
    assert_eq!(ev.len(), 2);
    assert_eq!(ev[1]["text"], "Second section");
    // After a skip, nothing new is written as its own event with empty text.
    r.p().front.as_mut().unwrap().overlay = true;
    r.engine.tick();
    r.p().front.as_mut().unwrap().overlay = false;
    r.advance(Duration::from_secs(3));
    r.engine.tick();
    let ev = r.events(&id);
    assert_eq!(ev.len(), 3);
    assert_eq!(ev[2]["text"], "");
    // A new recording starts fresh.
    r.engine.toggle_record();
    r.advance(Duration::from_secs(1));
    let id2 = r.record();
    r.engine.tick();
    assert!(r.events(&id2)[0]["text"].as_str().unwrap().contains("Intro paragraph"));
}

// MARK: evening reminder

#[test]
fn reminder_asks_the_platform() {
    let mut r = Rig::new("remind");
    let now = r.now();
    let day = clock::session_id(now);
    std::fs::create_dir_all(r.session_dir(&day)).unwrap();
    std::fs::write(r.session_dir(&day).join("events.jsonl"), "{}\n").unwrap();
    std::fs::write(r.root.join("config.json"), r#"{"remind_at": "00:00"}"#).unwrap();
    assert!(r.engine.daily_now());
    assert_eq!(r.engine.platform().reminded, 1);
    assert!(!r.engine.daily_now(), "once a day");
    assert_eq!(r.engine.platform().reminded, 1);
}

// MARK: where you look

impl Rig {
    /// `scene`, plus a second shared screen to the right (source 2, its own frame) with a docs window on it.
    fn two_screens(&mut self) {
        self.scene();
        let docs = ListedWindow { owner: "Safari".into(), name: Some("ACME Q3 — Docs".into()), ..ListedWindow::new(3, 70, r(110.0, 0.0, 80.0, 50.0)) };
        let arrived = self.now() + Duration::from_secs(1);
        let p = self.p();
        p.more_picks = vec![PickedContent::display(r(100.0, 0.0, 100.0, 50.0)).with_source(2)];
        p.frames_of = vec![(2, Frame { bgra: vec![90u8; 100 * 50 * 4].into(), width: 100, height: 50, stride: 400, arrived })];
        p.front.as_mut().unwrap().list.push(docs);
        p.ui_text = Some("Real words here. ".repeat(10));
        p.pointer_text = Some("Pointer docs line. ".repeat(10));
    }
}

#[test]
fn reads_where_you_look_too() {
    let mut g = Rig::new("pointer");
    g.projects(PROJECTS);
    g.two_screens();
    let id = g.record();
    g.engine.on_pick(PickOutcome::Added(2));
    g.p().pointer = Some((150.0, 25.0));
    g.advance(Duration::from_secs(3));
    g.engine.tick();
    assert_eq!(g.events(&id).len(), 1, "the mouse hasn't moved yet: only the window in use");
    assert_eq!(g.engine.platform().ui_asked, [false]);

    // The mouse moves over the docs on the other screen: read it too, marked as where you look.
    g.p().pointer = Some((160.0, 20.0));
    g.advance(Duration::from_secs(3));
    g.engine.tick();
    let ev = g.events(&id);
    assert_eq!(ev.len(), 2, "{ev:?}");
    assert!(ev[0].get("focus").is_none() && ev[0]["end"] == "2026-10-19T10:01:09Z", "the window in use, extended");
    assert!(ev[1]["focus"] == "pointer" && ev[1]["app"] == "Safari" && ev[1]["bundle"] == "safari.exe", "{:?}", ev[1]);
    assert!(ev[1]["title"] == "ACME Q3 — Docs" && ev[1]["src"] == "ax" && ev[1]["text"].as_str().unwrap().starts_with("Pointer docs"));
    assert_eq!(ev[1]["project"], "report", "its own project");
    assert!(g.state()["project"].is_null() && g.changes().is_empty(), "the project state is the window in use's");
    assert_ne!(g.engine.status().new_project_clue.as_deref(), Some("ACME Q3"), "nor the new-project clue");
    let line = std::fs::read_to_string(g.session_dir(&id).join("events.jsonl")).unwrap();
    assert!(line.contains(r#""project":"report","project_by":"auto","focus":"pointer","src":"ax","text":"#), "{line}");
    assert!(g.state()["skip"].is_null());

    // Nothing new under the mouse: nothing written for it (never an empty extend).
    g.advance(Duration::from_secs(3));
    g.engine.tick();
    let ev = g.events(&id);
    assert_eq!(ev.len(), 3);
    assert!(ev[2].get("focus").is_none() && ev[2]["text"] == "", "the window in use, nothing new");

    // New lines under the mouse are saved; then the mouse rests for more than 30 s: it isn't read any more.
    g.p().pointer_text = Some(format!("{}\nMore docs", "Pointer docs line. ".repeat(10)));
    g.advance(Duration::from_secs(3));
    g.engine.tick();
    assert_eq!(g.events(&id).last().unwrap()["text"], "More docs");
    g.p().pointer_text = Some(format!("{}\nMore docs\nEven more", "Pointer docs line. ".repeat(10)));
    g.advance(Duration::from_secs(31));
    let asked = g.engine.platform().ui_asked.len();
    g.engine.tick();
    assert_eq!(g.engine.platform().ui_asked.len(), asked + 1, "only the window in use");
    // A scroll the platform saw counts as looking.
    let t = g.now();
    g.p().pointer_activity = Some(t);
    g.engine.tick();
    assert_eq!(g.events(&id).last().unwrap()["text"], "Even more");
}

#[test]
fn where_you_look_only_on_another_shared_screen() {
    let mut g = Rig::new("pointer-rules");
    g.two_screens();
    let id = g.record();
    g.engine.on_pick(PickOutcome::Added(2));
    let t = g.now();
    g.p().pointer_activity = Some(t);
    let notes = ListedWindow { owner: "Notes".into(), ..ListedWindow::new(4, 80, r(55.0, 0.0, 40.0, 50.0)) };
    g.p().front.as_mut().unwrap().list.push(notes);
    // On the window in use, or another window on the same screen: the window in use is enough.
    for at in [(20.0, 20.0), (70.0, 20.0)] {
        g.p().pointer = Some(at);
        g.engine.tick();
    }
    // Over the second screen but not over a window, or unknown (Wayland): nothing more.
    g.p().pointer = Some((105.0, 20.0));
    g.engine.tick();
    g.p().pointer = None;
    g.engine.tick();
    assert!(g.engine.platform().ui_asked.iter().all(|p| !p), "never the mouse's window");
    assert_eq!(g.events(&id).len(), 1);
    // A never-record app under the mouse is never read.
    g.p().front.as_mut().unwrap().list[2].owner = "1Password".into();
    g.p().pointer = Some((150.0, 25.0));
    g.engine.tick();
    assert!(g.engine.platform().ui_asked.iter().all(|p| !p), "private: not read");
    assert_eq!(g.events(&id).len(), 1);
}

#[test]
fn where_you_look_while_the_window_in_use_is_skipped() {
    let mut g = Rig::new("pointer-skip");
    g.two_screens();
    let id = g.record();
    g.engine.on_pick(PickOutcome::Added(2));
    let t = g.now();
    g.p().pointer_activity = Some(t);
    g.p().pointer = Some((150.0, 25.0));
    // The window in use is a private app: the docs are read, and the skip reported is still the private app.
    g.p().front.as_mut().unwrap().app_name = "1Password".into();
    g.engine.tick();
    let ev = g.events(&id);
    assert!(ev.len() == 1 && ev[0]["focus"] == "pointer", "{ev:?}");
    let s = g.state();
    assert!(s["skip"] == "private_app" && s["events"] == 1, "{s}");

    // Read by OCR: the second screen's frame, cropped to the window under the mouse.
    g.p().front.as_mut().unwrap().app_name = "Google Chrome".into();
    g.p().pointer_text = None;
    g.p().ocr_text = "Seen by OCR".into();
    g.advance(Duration::from_secs(3));
    g.engine.tick();
    let img = g.engine.platform().ocr_images.last().unwrap().clone();
    assert_eq!((img.width, img.height), (80, 50), "cropped to the docs window");
    assert_eq!(img.pixel(40, 25), [90, 90, 90, 90], "the second screen's frame");
    let ev = g.events(&id);
    assert!(ev.last().unwrap()["focus"] == "pointer" && ev.last().unwrap()["text"] == "Seen by OCR", "{ev:?}");
    assert!(g.state()["skip"].is_null(), "the window in use was read");

    // The mouse's screen has no fresh frame: its skip isn't reported; the window in use is still written.
    g.p().frames_of[0].1.arrived = UNIX_EPOCH;
    g.p().ocr_text = "Other".into();
    let n = g.events(&id).len();
    g.advance(Duration::from_secs(3));
    g.engine.tick();
    assert!(g.state()["skip"].is_null());
    assert_eq!(g.events(&id).len(), n + 1, "only the window in use");
}

#[test]
fn adding_a_screen_already_shared_is_refused() {
    let mut g = Rig::new("already-shared");
    g.scene();
    g.record();
    g.engine.on_pick(PickOutcome::AlreadyShared);
    assert!(g.engine.is_recording());
    assert_eq!(g.engine.status().sources, 1);
    assert_eq!(g.engine.platform().toasts.last().unwrap(), "That screen is already shared. Pick a different one.");
}
