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
}

impl Platform for Fake {
    fn present_picker(&mut self) {
        self.presented += 1;
    }
    fn stop_capture(&mut self) {
        self.stops += 1;
    }
    fn picked(&self) -> Option<PickedContent> {
        self.picked.clone()
    }
    fn latest_frame(&self) -> Option<Frame> {
        self.frame.clone()
    }
    fn front(&mut self, is_excluded: &dyn Fn(Option<&str>, &str) -> bool) -> Option<Front> {
        let mut f = self.front.clone()?;
        for w in &mut f.list {
            w.excluded = is_excluded(None, &w.owner);
        }
        Some(f)
    }
    fn ui_text(&mut self, _front: &Front) -> Option<String> {
        self.ui_text.clone()
    }
    fn ui_title(&mut self, _front: &Front) -> Option<String> {
        self.ui_title.clone()
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
    assert_eq!(alerts[0].0, "Nothing was recorded");
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
    assert_eq!(r.engine.platform().toasts, ["Start recording first (Alt+Shift+C)"]);
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
    assert_eq!(r.engine.platform().toasts, ["Voice note saved"]);
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
    r.engine.toggle_record();
    assert!(!r.engine.is_listening() && !r.engine.is_recording());
    assert_eq!(r.engine.platform().indicators.last(), Some(&Indicator::Idle));
    assert_eq!(r.events(&id).len(), 1, "finished and saved into the session");
    assert!(r.engine.platform().alerts.is_empty(), "not an empty session");
    assert_eq!(r.state()["listening"], false);
}

#[test]
fn late_transcript_goes_to_its_session() {
    let mut r = Rig::new("voice-late");
    r.voice.borrow_mut().hold = true;
    let id = r.record();
    r.engine.toggle_voice();
    r.engine.toggle_record(); // the mic closes; the transcript is still coming
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
