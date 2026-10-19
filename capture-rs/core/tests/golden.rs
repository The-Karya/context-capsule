// context-capsule — core: golden vectors ported from capture/tests/main.swift (same inputs, same strings).
// Normalized rects are top-left origin here (Swift's are Vision's bottom-left): only those y values differ.

use context_capsule_core::clock;
use context_capsule_core::json::json_string;
use context_capsule_core::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn redacted(s: &str) -> bool {
    Redactor::redact(s).contains(Redactor::MARK)
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("capsule-golden-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn date() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_792_404_063) // 2026-10-19T10:01:03Z
}

fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(x, y, w, h)
}

fn near(a: Option<Rect>, b: Rect) -> bool {
    let Some(a) = a else { return false };
    (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6 && (a.width - b.width).abs() < 1e-6 && (a.height - b.height).abs() < 1e-6
}

fn reads(d: OcrDecision, region: Rect, masks: &[Rect]) -> bool {
    match d {
        OcrDecision::Read { region: got, masks: m } => {
            near(Some(got), region) && m.len() == masks.len() && m.iter().zip(masks).all(|(a, b)| near(Some(*a), *b))
        }
        OcrDecision::Skip(_) => false,
    }
}

// MARK: redaction

#[test]
fn redacts_secrets() {
    let secrets = [
        "key sk-proj-abcdefghijklmnop1234567890",
        "ANTHROPIC sk-ant-api03-AbCdEfGhIjKlMnOp",
        "token ghp_ABCDEFGHIJKLMNOPQRSTuvwxyz0123",
        "github_pat_11ABCDEFG0123456789_abcdefghijk",
        "slack xoxb-1234567890-abcdefghij",
        "slack xoxp-1234-5678-abcdefabcdef",
        "aws AKIAIOSFODNN7EXAMPLE",
        "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload",
        "sha 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
        "blob dGhpcyBpcyBhIHNlY3JldCBrZXkgZm9yIHRlc3Rpbmc0MjQy==",
        "password: hunter2",
        "Password = correct horse",
        "\"db_password\": \"s3cret\"",
    ];
    for s in secrets {
        assert!(redacted(s), "redacts {s} -> {}", Redactor::redact(s));
    }
    assert_eq!(Redactor::redact("user: saachi\npassword: hunter2\nnext line"), "user: saachi\n[redacted]\nnext line",
        "password line replaced, other lines kept");
    assert_eq!(Redactor::redact("use key sk-proj-abcdefghijklmnop1234567890 here"), "use key [redacted] here",
        "only the token is replaced");
}

#[test]
fn keeps_normal_text() {
    let safe = [
        "Pricing page redesign — Linear",
        "/Users/Saachi/Projects2024/context-capsule/capture/Sources",
        "internationalization-and-localization-settings-panel",
        "https://github.com/anthropics/claude-code/pull/1234",
        "Meeting at 10:30 with the design team about the new onboarding flow",
        "skip-this-step and sk-short",
        "Passwords should be rotated often",
    ];
    for s in safe {
        assert!(!redacted(s), "keeps normal text {s} -> {}", Redactor::redact(s));
    }
    // (?i) applies to the lookbehind too: an uppercase letter before "PASSWORD" blocks it.
    assert!(!redacted("XPASSWORD: x"));
}

#[test]
fn caps_text() {
    let long = "word ".repeat(2000);
    assert_eq!(redact::char_count(&Redactor::clean(&long)), 4000, "caps text at 4000");
    assert!(Redactor::clean(&long).starts_with("word word"), "cap keeps the start");
    assert_eq!(Redactor::clean("short"), "short", "short text unchanged");
}

// MARK: exclusion

#[test]
fn exclusion() {
    let none: &[String] = &[];
    let ex = |b: Option<&str>, n: &str, user: &[String]| Exclusion::is_excluded(b, Some(n), user);
    assert!(ex(Some("com.1password.1password"), "1Password", none), "1Password");
    assert!(ex(Some("com.bitwarden.desktop"), "Bitwarden", none), "Bitwarden");
    assert!(ex(Some("com.apple.keychainaccess"), "Keychain Access", none), "Keychain");
    assert!(ex(Some("org.whispersystems.signal-desktop"), "Signal", none), "Signal");
    assert!(ex(Some("net.whatsapp.WhatsApp"), "WhatsApp", none), "WhatsApp");
    assert!(ex(Some("com.agilebits.onepassword7"), "1Password 7", none), "1Password by name");
    assert!(ex(None, "LastPass", none), "no bundle, name match");
    assert!(!ex(Some("com.google.Chrome"), "Google Chrome", none), "Chrome allowed");
    assert!(ex(Some("com.example.App"), "Example", &["com.example.app".into()]), "user list, case-insensitive");
    assert!(ex(Some("com.example.app"), "Example", &[" Example ".into()]), "user list by app name");
    assert!(!ex(Some("com.example.app.helper"), "Helper", &["com.example.app".into()]), "user list is exact");
    assert!(!ex(Some("com.example.app"), "Ex", &["".into()]), "empty user entry ignored");
}

#[test]
fn config_file() {
    let dir = tmp("config");
    let cfg = dir.join("config.json");
    std::fs::write(&cfg, r#"{"exclude_apps": ["com.example.app"], "author": "Saachi"}"#).unwrap();
    assert_eq!(Config::load(&cfg).exclude_apps, vec!["com.example.app".to_string()], "config.json exclude_apps");
    assert_eq!(Config::load(&cfg).author.as_deref(), Some("Saachi"));
    assert!(Config::load(&dir.join("missing.json")).exclude_apps.is_empty(), "missing config");
    std::fs::write(&cfg, "not json").unwrap();
    assert!(Config::load(&cfg).exclude_apps.is_empty(), "broken config");
    std::fs::write(&cfg, r#"{"voice_locale": " en-GB "}"#).unwrap();
    assert_eq!(Config::load(&cfg).voice_locale.as_deref(), Some("en-GB"), "voice_locale from config");
    std::fs::write(&cfg, r#"{"voice_locale": ""}"#).unwrap();
    assert_eq!(Config::load(&cfg).voice_locale, None, "empty voice_locale means default");
    let _ = std::fs::remove_dir_all(&dir);
}

// MARK: times

#[test]
fn times() {
    assert_eq!(clock::iso(date()), "2026-10-19T10:01:03Z", "iso UTC");
    assert_eq!(clock::iso(date() + Duration::from_millis(999)), "2026-10-19T10:01:03Z", "whole seconds");
    assert_eq!(clock::session_id_at_offset(date(), 0), "20261019-100103", "session id");
    assert_eq!(clock::session_id_at_offset(date(), 5 * 3600 + 1800), "20261019-153103", "session id is local time");
    assert_eq!(clock::session_id(date()).len(), 15);
}

// MARK: event line format

#[test]
fn event_lines() {
    let e1 = Event {
        t: "2026-10-19T10:01:03Z".into(),
        end: "2026-10-19T10:01:03Z".into(),
        app: "Google Chrome".into(),
        bundle: "com.google.Chrome".into(),
        title: "Pricing — Linear".into(),
        text: "line 1\nsays \"hi\"\t\\ end".into(),
        ..Default::default()
    };
    let expected = r#"{"t":"2026-10-19T10:01:03Z","end":"2026-10-19T10:01:03Z","app":"Google Chrome","bundle":"com.google.Chrome","title":"Pricing — Linear","text":"line 1\nsays \"hi\"\t\\ end"}"#;
    assert_eq!(e1.line(), expected, "events.jsonl line format");
    let e2 = Event { t: "a".into(), end: "a".into(), app: "X".into(), bundle: "x".into(), title: "T".into(), src: "ax".into(), text: "hi".into(), kind: "".into() };
    assert_eq!(e2.line(), r#"{"t":"a","end":"a","app":"X","bundle":"x","title":"T","src":"ax","text":"hi"}"#, "src before text");
    assert_eq!(json_string("a\u{01}b"), r#""a\u0001b""#, "control chars escaped");
    assert_eq!(json_string("\u{1f}\r\u{2028}\u{2029}é"), "\"\\u001f\\r\\u2028\\u2029é\"", "U+2028/2029 escaped like Swift");
    let obj: HashMap<String, String> = serde_json::from_str(&e1.line()).expect("line is valid JSON");
    assert!(obj["text"] == e1.text && obj["title"] == e1.title, "line parses back");
}

// MARK: dedupe / extend

fn ev(t: &str, text: &str, app: &str) -> Event {
    Event {
        t: t.into(),
        end: t.into(),
        app: app.into(),
        bundle: "com.google.Chrome".into(),
        title: "Pricing — Linear".into(),
        text: text.into(),
        ..Default::default()
    }
}

fn lines(path: &std::path::Path) -> Vec<HashMap<String, String>> {
    let s = std::fs::read_to_string(path).unwrap();
    assert!(s.is_empty() || s.ends_with('\n'), "file ends with newline");
    s.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[test]
fn dedupe_and_extend() {
    let dir = tmp("dedupe");
    let path = dir.join("s/events.jsonl");
    let mut log = EventLog::open(&path).unwrap();
    let c = "Google Chrome";
    assert_eq!(log.record(&ev("2026-10-19T10:01:03Z", "héllo ✓", c)).unwrap(), Recorded::Appended, "first sample appends");
    assert_eq!(log.record(&ev("2026-10-19T10:01:06Z", "héllo ✓", c)).unwrap(), Recorded::Extended, "same sample extends");
    assert_eq!(log.record(&ev("2026-10-19T10:01:42Z", "héllo ✓", c)).unwrap(), Recorded::Extended, "extends again");
    let l = lines(&path);
    assert!(l.len() == 1 && l[0]["t"] == "2026-10-19T10:01:03Z" && l[0]["end"] == "2026-10-19T10:01:42Z", "end moved {l:?}");
    assert_eq!(log.record(&ev("2026-10-19T10:01:45Z", "new text", c)).unwrap(), Recorded::Appended, "changed text appends");
    assert_eq!(log.record(&ev("2026-10-19T10:01:48Z", "new text", "Figma")).unwrap(), Recorded::Appended, "changed app appends");
    assert_eq!(log.record(&ev("2026-10-19T10:01:51Z", "new text", "Figma")).unwrap(), Recorded::Extended, "extends the new last");
    let l = lines(&path);
    assert_eq!(l.len(), 3, "three lines");
    assert!(l[1]["end"] == "2026-10-19T10:01:45Z" && l[2]["end"] == "2026-10-19T10:01:51Z", "only the last line extended");
    log.break_run();
    assert_eq!(log.record(&ev("2026-10-19T10:01:54Z", "new text", "Figma")).unwrap(), Recorded::Appended, "after a break, a repeat appends");
    assert_eq!(lines(&path).len(), 4, "four lines");
    // A different-length end rewrites the whole last line.
    let mut odd = ev("x", "same", "A");
    log.record(&odd).unwrap();
    odd.end = "longer end".into();
    assert_eq!(log.record(&odd).unwrap(), Recorded::Extended);
    let l = lines(&path);
    assert!(l.len() == 5 && l[4]["end"] == "longer end");
    let _ = std::fs::remove_dir_all(&dir);
}

// MARK: session.json, state.json, control

#[test]
fn session_and_state_json() {
    let sj = session_json("20261019-100103", date(), None, "Mac");
    assert_eq!(sj, r#"{"id":"20261019-100103","started_at":"2026-10-19T10:01:03Z","ended_at":null,"host":"Mac","version":1}"#, "session.json");
    let sj2 = session_json("x", date(), Some(date() + Duration::from_secs(60)), "Mac");
    assert!(sj2.contains(r#""ended_at":"2026-10-19T10:02:03Z""#), "session.json ended_at {sj2}");
    let st = |s: StateJson| s.json();
    assert_eq!(st(StateJson { pid: 42, ..Default::default() }),
        r#"{"recording":false,"session_id":null,"started_at":null,"pid":42,"picking":false,"last_error":null,"events":0,"skip":null,"listening":false,"note_at":null}"#, "state.json idle");
    assert_eq!(st(StateJson { recording: true, session_id: Some("20261019-100103".into()), started_at: Some(date()), pid: 7, ..Default::default() }),
        r#"{"recording":true,"session_id":"20261019-100103","started_at":"2026-10-19T10:01:03Z","pid":7,"picking":false,"last_error":null,"events":0,"skip":null,"listening":false,"note_at":null}"#, "state.json recording");
    assert_eq!(st(StateJson { pid: 1, picking: true, ..Default::default() }),
        r#"{"recording":false,"session_id":null,"started_at":null,"pid":1,"picking":true,"last_error":null,"events":0,"skip":null,"listening":false,"note_at":null}"#, "state.json picking");
    assert_eq!(st(StateJson { pid: 1, last_error: Some("cancelled".into()), ..Default::default() }),
        r#"{"recording":false,"session_id":null,"started_at":null,"pid":1,"picking":false,"last_error":"cancelled","events":0,"skip":null,"listening":false,"note_at":null}"#, "state.json cancelled");
    assert!(st(StateJson { recording: true, session_id: Some("s".into()), pid: 1, events: 3, skip: Some(Skip::NotPicked), ..Default::default() })
        .ends_with(r#""events":3,"skip":"not_picked","listening":false,"note_at":null}"#), "state.json events + skip");
    assert!(st(StateJson { recording: true, session_id: Some("s".into()), pid: 1, listening: true, note_at: Some(5), ..Default::default() })
        .ends_with(r#""listening":true,"note_at":5}"#), "state.json listening + note_at");
}

#[test]
fn control_commands() {
    let p = |s: &str| ControlCommand::parse(s.as_bytes());
    assert_eq!(p(r#"{"command":"start","at":1792404063000}"#), Some(ControlCommand { kind: ControlKind::Start, at: 1_792_404_063_000, text: None }), "control start");
    assert_eq!(p(r#"{"command":"stop","at":5}"#).map(|c| c.kind), Some(ControlKind::Stop), "control stop");
    assert_eq!(p(r#"{"command":"pause","at":5}"#), None, "control unknown");
    assert_eq!(p("garbage"), None, "control garbage");
    assert_eq!(p(r#"{"command":"note","text":" decided X ","at":9}"#), Some(ControlCommand { kind: ControlKind::Note, at: 9, text: Some("decided X".into()) }), "control note");
    assert_eq!(p(r#"{"command":"note","text":"  ","at":9}"#), None, "empty note ignored");
    assert_eq!(p(r#"{"command":"note","at":9}"#), None, "note without text ignored");
    assert_eq!(p(r#"{"command":"START"}"#).map(|c| (c.kind, c.at)), Some((ControlKind::Start, 0)), "case-insensitive, at defaults to 0");
}

// MARK: picked content

#[test]
fn picked_content() {
    let display = r(0.0, 0.0, 1512.0, 982.0);
    let second = r(1512.0, 0.0, 1920.0, 1080.0);
    let win = ListedWindow::new(42, 900, r(100.0, 50.0, 800.0, 600.0));
    let win_on_second = ListedWindow::new(43, 900, r(1600.0, 100.0, 800.0, 600.0));
    assert!(Membership::contains(&PickedContent::display(display), &win), "display holds the window");
    assert!(!Membership::contains(&PickedContent::display(display), &win_on_second), "other display: no");
    assert!(Membership::contains(&PickedContent::display(second), &win_on_second), "second display");
    assert!(!Membership::contains(&PickedContent::display(Rect::ZERO), &win), "empty display rect: no");
    assert!(Membership::contains(&PickedContent::window(Rect::ZERO, Some(vec![7, 42])), &win), "picked window by id");
    assert!(!Membership::contains(&PickedContent::window(win.frame, Some(vec![7])), &win), "ids known: frame alone not enough");
    assert!(Membership::contains(&PickedContent::window(r(103.0, 48.0, 796.0, 604.0), None), &win), "no ids: same frame within tolerance");
    assert!(!Membership::contains(&PickedContent::window(r(100.0, 50.0, 400.0, 600.0), None), &win), "no ids: different size");

    assert!(near(Membership::ocr_region(&PickedContent::window(win.frame, None), &win.frame), Rect::UNIT), "window: OCR whole");
    let d = r(0.0, 0.0, 1000.0, 500.0);
    assert!(near(Membership::ocr_region(&PickedContent::display(d), &r(100.0, 0.0, 500.0, 250.0)), r(0.1, 0.0, 0.5, 0.5)),
        "display: crop, top-left origin (Swift: y 0.5, bottom-left)");
    assert!(near(Membership::ocr_region(&PickedContent::display(d), &r(800.0, 400.0, 500.0, 500.0)), r(0.8, 0.8, 0.2, 0.2)),
        "crop clipped to the display (Swift: y 0)");
    assert_eq!(Membership::ocr_region(&PickedContent::display(d), &r(2000.0, 0.0, 10.0, 10.0)), None, "window off the display: no region");
    assert!(near(Membership::ocr_region(&PickedContent::display(second), &r(1512.0, 0.0, 960.0, 1080.0)), r(0.0, 0.0, 0.5, 1.0)), "second display offset");
}

// MARK: front window choice

#[test]
fn front_window_choice() {
    let w = |id: u64, frame: Rect| ListedWindow { owner: "Google Chrome".into(), ..ListedWindow::new(id, 50, frame) };
    let chrome_strip = ListedWindow { alpha: 0.0, ..w(1, r(0.0, 16.0, 1440.0, 32.0)) };
    let zen_strip = w(2, r(0.0, 30.0, 1440.0, 68.0));
    let chrome_main = w(3, r(0.0, 30.0, 1440.0, 792.0));
    let badge = w(4, r(10.0, 10.0, 20.0, 20.0));
    let id = |l: &[ListedWindow]| WindowList::front(50, l).map(|w| w.id);
    assert_eq!(id(&[chrome_strip.clone(), chrome_main.clone()]), Some(3), "alpha-0 strip skipped");
    assert_eq!(id(&[zen_strip, chrome_main.clone()]), Some(3), "visible strip gives way to its window");
    assert_eq!(id(&[badge.clone(), chrome_main.clone()]), Some(3), "tiny window skipped");
    assert_eq!(id(&[chrome_strip, badge]), None, "no normal window");
    let floating = ListedWindow { layer: 3, ..ListedWindow::new(5, 50, r(0.0, 0.0, 300.0, 300.0)) };
    assert_eq!(id(&[floating, chrome_main.clone()]), Some(3), "floating panel is not the window");
    let dialog = ListedWindow::new(6, 50, r(2000.0, 100.0, 400.0, 300.0));
    assert_eq!(id(&[dialog, chrome_main]), Some(6), "separate front window kept");
}

// MARK: freshness

#[test]
fn freshness() {
    let switch_at = UNIX_EPOCH + Duration::from_secs(1000);
    assert!(!Freshness::ok(None, None), "no frame: not ok");
    assert!(Freshness::ok(Some(switch_at), None), "no switch yet: ok");
    assert!(!Freshness::ok(Some(switch_at - Duration::from_secs(2)), Some(switch_at)), "frame from before the switch");
    assert!(!Freshness::ok(Some(switch_at + Duration::from_millis(50)), Some(switch_at)), "frame during the switch");
    assert!(Freshness::ok(Some(switch_at + Duration::from_millis(500)), Some(switch_at)), "frame after the switch");
}

// MARK: OCR plan (crop + private masks)

#[test]
fn ocr_plan() {
    let main = r(0.0, 0.0, 1440.0, 900.0);
    let ext = r(1440.0, 48.0, 1920.0, 1080.0); // external display, offset like a real one
    let pick_main = PickedContent::display(main);
    let chrome = ListedWindow { owner: "Google Chrome".into(), ..ListedWindow::new(10, 50, r(0.0, 30.0, 720.0, 870.0)) };
    let private = |id: u64, layer: i32, frame: Rect| ListedWindow { owner: "WhatsApp".into(), layer, excluded: true, ..ListedWindow::new(id, 60, frame) };
    let whats_behind = private(11, 0, r(0.0, 30.0, 1440.0, 870.0));
    let whats_float = private(12, 3, r(0.0, 30.0, 200.0, 200.0));
    let whats_big = private(13, 0, r(0.0, 0.0, 1440.0, 900.0));
    let slack = ListedWindow { owner: "Slack".into(), layer: 3, ..ListedWindow::new(14, 70, r(0.0, 30.0, 200.0, 200.0)) };
    // Swift (bottom-left): region y 0; here (top-left): y 30/900.
    let crop = r(0.0, 30.0 / 900.0, 0.5, 870.0 / 900.0);

    assert!(reads(Membership::ocr_decision(&pick_main, &chrome, &[chrome.clone(), whats_behind.clone()]), crop, &[]),
        "crop to the front window; private window behind it is not in the crop");
    match Membership::ocr_decision(&pick_main, &chrome, &[whats_float.clone(), chrome.clone(), whats_behind]) {
        OcrDecision::Read { region, masks } => {
            assert_eq!(region.width, 0.5, "crop kept with a private window on top");
            // Swift: y 670/900 (bottom-left); here y 30/900.
            assert!(masks.len() == 1 && near(masks.first().copied(), r(0.0, 30.0 / 900.0, 200.0 / 1440.0, 200.0 / 900.0)),
                "private window on top is masked {masks:?}");
        }
        other => panic!("private window on top: read with a mask, got {other:?}"),
    }
    let banner = ListedWindow { owner: "Notification Center".into(), layer: 23, notification: true, ..ListedWindow::new(15, 80, r(1060.0, 40.0, 370.0, 90.0)) };
    let chrome_wide = ListedWindow::new(16, 50, r(0.0, 30.0, 1440.0, 870.0));
    match Membership::ocr_decision(&pick_main, &chrome_wide, &[banner, chrome_wide.clone()]) {
        OcrDecision::Read { masks, .. } => assert_eq!(masks.len(), 1, "notification banner is masked"),
        other => panic!("banner: read with a mask, got {other:?}"),
    }
    let widget = ListedWindow { owner: "Notification Center".into(), layer: -2147483601, ..ListedWindow::new(17, 80, r(8.0, 56.0, 360.0, 180.0)) };
    assert!(!WindowList::is_private(&widget), "desktop widget is not a banner");
    assert_eq!(Membership::ocr_decision(&pick_main, &chrome, &[whats_big, chrome.clone()]), OcrDecision::Skip(Skip::PrivateWindow),
        "private window covering the front window: skip");
    assert!(reads(Membership::ocr_decision(&pick_main, &chrome, &[slack, chrome.clone()]), crop, &[]), "normal app on top: no mask");
    assert_eq!(Membership::ocr_decision(&PickedContent::display(Rect::ZERO), &chrome, std::slice::from_ref(&chrome)), OcrDecision::Skip(Skip::UnknownDisplay),
        "unknown display: skip");
    let on_ext = ListedWindow::new(20, 50, r(1440.0, 48.0, 960.0, 1080.0));
    assert_eq!(Membership::ocr_decision(&pick_main, &on_ext, std::slice::from_ref(&on_ext)), OcrDecision::Skip(Skip::NotPicked), "window on another display");
    assert!(reads(Membership::ocr_decision(&PickedContent::display(ext), &on_ext, std::slice::from_ref(&on_ext)), r(0.0, 0.0, 0.5, 1.0), &[]),
        "external display crop");
    assert!(reads(Membership::ocr_decision(&PickedContent::window(chrome.frame, Some(vec![10])), &chrome, &[whats_float, chrome.clone()]), Rect::UNIT, &[]),
        "picked window: read whole, other apps are not in the image");
}

// MARK: picked display place

#[test]
fn resolve_display() {
    let main = r(0.0, 0.0, 1440.0, 900.0);
    let ext = r(1440.0, 48.0, 1920.0, 1080.0);
    let displays = [main, ext];
    assert_eq!(Membership::resolve_display(&ext, &displays), Some(ext), "global display rect kept");
    assert_eq!(Membership::resolve_display(&r(0.0, 0.0, 1920.0, 1080.0), &displays), Some(ext), "(0,0)-based external display found by size");
    assert_eq!(Membership::resolve_display(&r(0.0, 0.0, 1920.0, 1080.0), &[main, ext, r(3360.0, 0.0, 1920.0, 1080.0)]), None,
        "two same-size displays, no origin: unknown");
    assert_eq!(Membership::resolve_display(&Rect::ZERO, &displays), None, "empty rect: unknown");
}

// MARK: accessibility text

fn node(depth: usize, kind: NodeKind, value: &str, title: &str, desc: &str) -> TextNode {
    TextNode { depth, kind, value: value.into(), title: title.into(), desc: desc.into() }
}

#[test]
fn text_assembly() {
    use NodeKind::*;
    // Swift roles mapped to kinds: AXWindow/AXWebArea/AXButton/AXGroup → Other, AXHeading/AXLink/AXCell → Label,
    // AXStaticText/AXTextArea/AXTextField → Value, anything secure → Secure.
    let page = [
        node(0, Other, "", "Pricing — Linear", ""),
        node(1, Other, "", "Pricing", ""),
        node(2, Label, "", "Pricing page   redesign", ""),
        node(3, Value, "Pricing page redesign", "", ""),
        node(2, Value, "We keep three tiers.\nDrop the free plan.", "", ""),
        node(2, Other, "", "Submit", ""),
        node(2, Secure, "hunter2", "", ""),
        node(2, Secure, "", "", ""),
        node(3, Value, "inside-secure-group", "", ""),
        node(2, Secure, "s3cret", "", ""),
        node(2, Label, "", "", "Docs link"),
        node(2, Label, "Q4", "", ""),
        node(2, Value, "notes here", "", ""),
    ];
    let assembled = UiText::assemble(&page, UiText::MAX_CHARS);
    assert_eq!(assembled, "Pricing page redesign\nWe keep three tiers.\nDrop the free plan.\nDocs link\nQ4\nnotes here", "AX text assembly");
    assert!(!assembled.contains("hunter2") && !assembled.contains("s3cret") && !assembled.contains("inside-secure"), "secure skipped");
    assert!(!assembled.contains("Submit"), "buttons are not text");
    let many: Vec<TextNode> = (0..500).map(|i| node(1, Value, &format!("line number {i} with some words"), "", "")).collect();
    assert!(UiText::assemble(&many, 300).len() <= 340, "assembly stops near maxChars");
    assert_eq!(UiText::assemble(&[], UiText::MAX_CHARS), "", "empty tree");
    assert_eq!(UiText::assemble(&[node(0, Value, "a\r\nb\u{2028}c \t d", "", "")], 100), "a\nb\nc d", "newlines split, spaces collapse");
}

#[test]
fn ax_or_ocr() {
    let long_text = "Real words here. ".repeat(10);
    assert_eq!(UiText::choose(true, &long_text), TextSource::Ax, "enough AX text: ax");
    assert_eq!(UiText::choose(false, &long_text), TextSource::Ocr, "not trusted: ocr");
    assert_eq!(UiText::choose(true, "File Edit View"), TextSource::Ocr, "too little AX text: ocr");
    assert_eq!(UiText::choose(true, &"- ".repeat(200)), TextSource::Ocr, "punctuation does not count");
    assert_eq!(UiText::choose(true, &"a".repeat(80)), TextSource::Ax, "80 letters is enough");
    assert_eq!(UiText::choose(true, &"a".repeat(79)), TextSource::Ocr, "79 is not");
}

// MARK: voice notes — silence

#[test]
fn voice_silence() {
    assert_eq!(Voice::peak(&[0.0, -0.5, 0.25]), 0.5, "peak is the loudest absolute sample");
    assert_eq!(Voice::peak(&[]), 0.0, "empty clip peak 0");
    let clip = |text: &str, peak: f32, seconds: f64| VoiceClip { text: text.into(), peak, seconds };
    assert!(!Voice::is_empty(&clip("We keep three tiers", 0.2, 3.0)), "real speech is kept");
    assert!(Voice::is_empty(&clip("", 0.2, 3.0)), "no words: empty");
    assert!(Voice::is_empty(&clip("  \n", 0.2, 3.0)), "blank words: empty");
    assert!(Voice::is_empty(&clip("hi", 0.0003, 3.0)), "dead mic noise floor: silent");
    assert!(Voice::is_empty(&clip("hi", 0.0, 3.0)), "digital zeros: silent");
    assert!(!Voice::is_empty(&clip("hi", 0.03, 3.0)), "quiet speech is not silent");
    assert!(Voice::is_empty(&clip("hi", 0.5, 0.2)), "double press is too short");
}

// MARK: voice notes — event line

#[test]
fn voice_event_lines() {
    let start = date();
    let v = Voice::event("  Use annual pricing\npassword: hunter2  ", Event::VOICE, "voice", start, start + Duration::from_secs(9),
        "Figma", "com.figma.Desktop", "Pricing v2");
    assert_eq!(v.line(), r#"{"t":"2026-10-19T10:01:03Z","end":"2026-10-19T10:01:12Z","kind":"voice","src":"voice","app":"Figma","bundle":"com.figma.Desktop","title":"Pricing v2","text":"Use annual pricing\n[redacted]"}"#,
        "voice event line: kind + src, trimmed, redacted");
    let long_voice = Voice::event(&"word ".repeat(2000), Event::VOICE, "voice", start, start, "", "", "");
    assert_eq!(redact::char_count(&long_voice.text), 4000, "voice text capped at 4000");
    let note_line = Voice::event("typed context", Event::NOTE, "typed", start, start, "", "", "").line();
    assert!(note_line.contains(r#""kind":"note","src":"typed""#), "typed note line {note_line}");

    let dir = tmp("voice-log");
    let mut log = EventLog::open(&dir.join("v/events.jsonl")).unwrap();
    let screen = ev("2026-10-19T10:01:03Z", "same screen", "Google Chrome");
    assert_eq!(log.record(&screen).unwrap(), Recorded::Appended, "screen sample");
    assert_eq!(log.record(&v).unwrap(), Recorded::Appended, "voice note never extends a screen event");
    assert_eq!(log.record(&v).unwrap(), Recorded::Appended, "two identical voice notes stay two");
    assert_eq!(log.record(&ev("2026-10-19T10:01:20Z", "same screen", "Google Chrome")).unwrap(), Recorded::Appended,
        "screen after a note starts a new event");
    assert_eq!(log.appended(), 4, "four lines written");
    let _ = std::fs::remove_dir_all(&dir);
}

// MARK: voice notes — the indicator always resets

#[derive(Default)]
struct FakeVoice {
    problem: Option<VoiceOutcome>,
    start_error: Option<String>,
    result: Option<Result<VoiceClip, String>>,
    hold_finish: bool,
    pending: Option<Replier>,
    mic_open: bool,
    cancels: usize,
}

impl FakeVoice {
    fn new() -> FakeVoice {
        FakeVoice { result: Some(Ok(VoiceClip { text: "We keep three tiers".into(), peak: 0.2, seconds: 4.0 })), ..Default::default() }
    }
}

impl VoiceEngine for FakeVoice {
    fn prepare(&mut self, _locale: &str, reply: Replier) {
        reply.send(VoiceReply::Prepared(self.problem.clone()));
    }
    fn start(&mut self) -> Result<(), String> {
        if let Some(e) = &self.start_error {
            return Err(e.clone());
        }
        self.mic_open = true;
        Ok(())
    }
    fn finish(&mut self, reply: Replier) {
        self.mic_open = false;
        if self.hold_finish {
            self.pending = Some(reply);
        } else if let Some(r) = self.result.clone() {
            reply.send(VoiceReply::Finished(r));
        }
    }
    fn cancel(&mut self) {
        self.mic_open = false;
        self.cancels += 1;
    }
    fn device_name(&self) -> Option<String> {
        Some("MacBook Air Microphone".into())
    }
}

struct Rig {
    notes: VoiceNotes<FakeVoice>,
    indicator: Vec<bool>,
    outcomes: Vec<VoiceOutcome>,
    written: Vec<Event>,
    clock: std::sync::Arc<std::sync::Mutex<SystemTime>>,
}

impl Rig {
    fn new(engine: FakeVoice) -> Rig {
        let mut notes = VoiceNotes::new(engine);
        let clock = std::sync::Arc::new(std::sync::Mutex::new(date()));
        let c = clock.clone();
        notes.set_clock(Box::new(move || *c.lock().unwrap()));
        Rig { notes, indicator: vec![], outcomes: vec![], written: vec![], clock }
    }
    fn drain(&mut self) {
        for s in self.notes.take_signals() {
            match s {
                VoiceSignal::Indicator(on) => self.indicator.push(on),
                VoiceSignal::Outcome(o) => self.outcomes.push(o),
                VoiceSignal::Save { event, .. } => self.written.push(event),
            }
        }
    }
    fn press(&mut self, recording: bool) {
        let target = VoiceTarget { app: "Figma".into(), bundle: "com.figma.Desktop".into(), title: "Pricing".into(), session: "s".into() };
        self.notes.toggle("en-US", recording.then_some(target));
        self.drain();
    }
    /// Ended idle, with the mic shut and the last indicator call "off".
    fn reset_ok(&self) -> bool {
        self.notes.phase() == VoicePhase::Idle && !self.notes.engine().mic_open && self.indicator.last() == Some(&false)
    }
    fn advance(&mut self, secs: u64) {
        *self.clock.lock().unwrap() += Duration::from_secs(secs);
    }
}

#[test]
fn voice_press_and_save() {
    let mut r = Rig::new(FakeVoice::new());
    r.press(true);
    assert!(r.notes.phase() == VoicePhase::Listening && r.indicator == [true] && r.notes.engine().mic_open, "first press opens the mic, mic shown");
    r.advance(4);
    r.press(true);
    assert!(r.reset_ok() && r.outcomes == [VoiceOutcome::Saved { chars: 19, seconds: 4.0 }], "second press saves {:?}", r.outcomes);
    assert!(r.written.len() == 1 && r.written[0].kind == "voice" && r.written[0].app == "Figma", "note written with context");
    assert_eq!(r.written[0].t, "2026-10-19T10:01:03Z");
    assert_eq!(r.written[0].end, "2026-10-19T10:01:07Z");
}

#[test]
fn voice_silent() {
    let mut r = Rig::new(FakeVoice { result: Some(Ok(VoiceClip { text: "".into(), peak: 0.001, seconds: 3.0 })), ..FakeVoice::new() });
    r.press(true);
    r.press(true);
    assert!(r.reset_ok() && r.outcomes == [VoiceOutcome::Silent { device: Some("MacBook Air Microphone".into()) }] && r.written.is_empty(),
        "silent clip: reset, nothing written");
    let m = r.outcomes[0].message(&PlatformText::MACOS);
    assert!(m.contains("Didn't catch that") && m.contains("MacBook Air Microphone"), "silent message names the mic");
    assert_eq!(m, "Didn't catch that. Nothing was heard from “MacBook Air Microphone”. Check System Settings → Sound → Input.");
}

#[test]
fn voice_denied_and_unavailable() {
    let mut r = Rig::new(FakeVoice { problem: Some(VoiceOutcome::Denied(VoicePermission::Microphone)), ..FakeVoice::new() });
    r.press(true);
    assert!(r.reset_ok() && r.outcomes == [VoiceOutcome::Denied(VoicePermission::Microphone)] && !r.indicator.contains(&true),
        "mic denied: never shown, reset");
    assert!(r.outcomes[0].message(&PlatformText::MACOS).contains("System Settings"), "denied says where to fix it");
    assert!(r.outcomes[0].message(&PlatformText::WINDOWS).contains("Settings → Privacy & security → Microphone"));

    let mut r = Rig::new(FakeVoice { problem: Some(VoiceOutcome::Unavailable { locale: "ta-IN".into() }), ..FakeVoice::new() });
    r.press(true);
    assert!(r.reset_ok() && r.outcomes == [VoiceOutcome::Unavailable { locale: "ta-IN".into() }], "no on-device model: reset");
    assert!(r.outcomes[0].message(&PlatformText::MACOS).contains("nothing is sent to Apple"), "unavailable says nothing is sent");
    assert_eq!(r.outcomes[0].message(&PlatformText::MACOS),
        "On-device speech isn't available for ta-IN, so voice notes are off (nothing is sent to Apple). Set \"voice_locale\" in ~/.context-capsule/config.json, or add the language in System Settings → Keyboard → Dictation.",
        "same words as the Swift recorder");
}

#[test]
fn voice_errors() {
    let mut r = Rig::new(FakeVoice { start_error: Some("mic unplugged".into()), ..FakeVoice::new() });
    r.press(true);
    assert!(r.reset_ok() && r.outcomes == [VoiceOutcome::Failed("mic unplugged".into())] && r.notes.engine().cancels == 1, "mic start error: reset");

    let mut r = Rig::new(FakeVoice { result: Some(Err("mic unplugged".into())), ..FakeVoice::new() });
    r.press(true);
    r.press(true);
    assert!(r.reset_ok() && r.outcomes == [VoiceOutcome::Failed("mic unplugged".into())], "recognizer error: reset");
}

#[test]
fn voice_timeout() {
    let mut r = Rig::new(FakeVoice::new());
    r.press(true);
    r.advance(119);
    r.notes.check_timeout();
    r.drain();
    assert_eq!(r.notes.phase(), VoicePhase::Listening, "not yet");
    r.advance(1);
    r.notes.check_timeout();
    r.drain();
    assert!(r.reset_ok() && r.outcomes.len() == 1 && r.written.len() == 1, "timeout closes the mic and saves");
    r.advance(200);
    r.notes.check_timeout();
    r.drain();
    assert_eq!(r.outcomes.len(), 1, "a stale timeout does nothing");
}

#[test]
fn voice_not_recording() {
    let mut r = Rig::new(FakeVoice::new());
    r.press(false);
    assert!(r.reset_ok() || r.indicator.is_empty(), "not recording: mic never opens");
    assert!(r.outcomes == [VoiceOutcome::NotRecording] && r.outcomes[0].message(&PlatformText::MACOS) == "Start recording first (⌥⇧C)",
        "not recording message");
    assert_eq!(r.outcomes[0].message(&PlatformText::WINDOWS), "Start recording first (Ctrl+Alt+Shift+C)");
    assert!(r.notes.phase() == VoicePhase::Idle && !r.notes.engine().mic_open, "not recording: idle");
}

#[test]
fn voice_recording_stops_mid_note() {
    let mut r = Rig::new(FakeVoice::new());
    r.press(true);
    r.notes.recording_will_stop();
    r.drain();
    assert!(r.reset_ok() && r.written.len() == 1, "recording stops mid-note: the note is finished and saved");
}

#[test]
fn voice_late_transcript() {
    let mut r = Rig::new(FakeVoice { hold_finish: true, ..FakeVoice::new() });
    r.press(true);
    r.press(true);
    assert!(r.notes.phase() == VoicePhase::Finishing && r.indicator.last() == Some(&false) && !r.notes.engine().mic_open,
        "finishing: mic already off");
    r.press(true);
    assert_eq!(r.notes.phase(), VoicePhase::Finishing, "press while finishing is ignored");
    let reply = r.notes.engine_mut().pending.take().unwrap();
    reply.send(VoiceReply::Finished(Ok(VoiceClip { text: "late words".into(), peak: 0.3, seconds: 2.0 })));
    r.notes.pump();
    r.drain();
    assert!(r.reset_ok() && r.written.len() == 1, "late transcript still saved, then reset");
}

#[test]
fn voice_stale_reply_ignored() {
    // A reply for an older note (say, a finish that came back after a newer note began) does nothing.
    let mut r = Rig::new(FakeVoice { hold_finish: true, ..FakeVoice::new() });
    r.press(true);
    r.press(true);
    let old = r.notes.engine_mut().pending.take().unwrap();
    r.notes.recording_will_stop(); // finishing: ignored
    old.send(VoiceReply::Finished(Ok(VoiceClip { text: "x y z".into(), peak: 0.3, seconds: 2.0 })));
    r.notes.pump();
    r.drain();
    assert!(r.reset_ok() && r.written.len() == 1);
    // Now a duplicate send from a cloned token would be stale; the phase guard drops it too.
    r.notes.pump();
    assert_eq!(r.written.len(), 1);
}
