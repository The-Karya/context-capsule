// context-capsule — capture: tiny test runner for Core.swift (redaction, exclusion, dedupe, file formats,
// picked-content membership, AX text assembly, AX-vs-OCR choice).

import Foundation

var failures = 0
var passes = 0

func check(_ ok: Bool, _ name: String, _ detail: @autoclosure () -> String = "") {
    if ok { passes += 1 } else { failures += 1; print("FAIL: \(name) \(detail())") }
}

func redacted(_ s: String) -> Bool { Redactor.redact(s).contains(Redactor.mark) }

// MARK: redaction

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
]
for s in secrets { check(redacted(s), "redacts", s + " -> " + Redactor.redact(s)) }

let pwd = Redactor.redact("user: saachi\npassword: hunter2\nnext line")
check(pwd == "user: saachi\n[redacted]\nnext line", "password line replaced, other lines kept", pwd)

let skKept = Redactor.redact("use key sk-proj-abcdefghijklmnop1234567890 here")
check(skKept == "use key [redacted] here", "only the token is replaced", skKept)

let safe = [
    "Pricing page redesign — Linear",
    "/Users/Saachi/Projects2024/context-capsule/capture/Sources",
    "internationalization-and-localization-settings-panel",
    "https://github.com/anthropics/claude-code/pull/1234",
    "Meeting at 10:30 with the design team about the new onboarding flow",
    "skip-this-step and sk-short",
    "Passwords should be rotated often",
]
for s in safe { check(!redacted(s), "keeps normal text", s + " -> " + Redactor.redact(s)) }

let long = String(repeating: "word ", count: 2000)
check(Redactor.clean(long).count == 4000, "caps text at 4000")
check(Redactor.clean(long).hasPrefix("word word"), "cap keeps the start")
check(Redactor.clean("short") == "short", "short text unchanged")

// MARK: exclusion

check(Exclusion.isExcluded(bundle: "com.1password.1password", appName: "1Password", userList: []), "1Password")
check(Exclusion.isExcluded(bundle: "com.bitwarden.desktop", appName: "Bitwarden", userList: []), "Bitwarden")
check(Exclusion.isExcluded(bundle: "com.apple.keychainaccess", appName: "Keychain Access", userList: []), "Keychain")
check(Exclusion.isExcluded(bundle: "org.whispersystems.signal-desktop", appName: "Signal", userList: []), "Signal")
check(Exclusion.isExcluded(bundle: "net.whatsapp.WhatsApp", appName: "WhatsApp", userList: []), "WhatsApp")
check(Exclusion.isExcluded(bundle: "com.agilebits.onepassword7", appName: "1Password 7", userList: []), "1Password by name")
check(Exclusion.isExcluded(bundle: nil, appName: "LastPass", userList: []), "no bundle, name match")
check(!Exclusion.isExcluded(bundle: "com.google.Chrome", appName: "Google Chrome", userList: []), "Chrome allowed")
check(Exclusion.isExcluded(bundle: "com.example.App", appName: "Example", userList: ["com.example.app"]), "user list, case-insensitive")
check(Exclusion.isExcluded(bundle: "com.example.app", appName: "Example", userList: [" Example "]), "user list by app name")
check(!Exclusion.isExcluded(bundle: "com.example.app.helper", appName: "Helper", userList: ["com.example.app"]), "user list is exact")
check(!Exclusion.isExcluded(bundle: "com.example.app", appName: "Ex", userList: [""]), "empty user entry ignored")

let tmp = FileManager.default.temporaryDirectory.appendingPathComponent("capsule-tests-\(getpid())", isDirectory: true)
try? FileManager.default.removeItem(at: tmp)
try! FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
let cfgURL = tmp.appendingPathComponent("config.json")
try! #"{"exclude_apps": ["com.example.app"], "author": "Saachi"}"#.write(to: cfgURL, atomically: true, encoding: .utf8)
check(Config.load(from: cfgURL).excludeApps == ["com.example.app"], "config.json exclude_apps")
check(Config.load(from: tmp.appendingPathComponent("missing.json")).excludeApps.isEmpty, "missing config")
try! "not json".write(to: cfgURL, atomically: true, encoding: .utf8)
check(Config.load(from: cfgURL).excludeApps.isEmpty, "broken config")

// MARK: times

let date = Date(timeIntervalSince1970: 1_792_404_063) // 2026-10-19T10:01:03Z
check(Clock.iso(date) == "2026-10-19T10:01:03Z", "iso UTC", Clock.iso(date))
check(Clock.sessionID(date, timeZone: TimeZone(identifier: "UTC")!) == "20261019-100103", "session id",
      Clock.sessionID(date, timeZone: TimeZone(identifier: "UTC")!))
check(Clock.sessionID(date, timeZone: TimeZone(identifier: "Asia/Kolkata")!) == "20261019-153103", "session id is local time")

// MARK: event line format

let e1 = Event(t: "2026-10-19T10:01:03Z", end: "2026-10-19T10:01:03Z", app: "Google Chrome",
               bundle: "com.google.Chrome", title: "Pricing — Linear", text: "line 1\nsays \"hi\"\t\\ end")
let expected = #"{"t":"2026-10-19T10:01:03Z","end":"2026-10-19T10:01:03Z","app":"Google Chrome","bundle":"com.google.Chrome","title":"Pricing — Linear","text":"line 1\nsays \"hi\"\t\\ end"}"#
check(e1.line == expected, "events.jsonl line format", "\n got: \(e1.line)\nwant: \(expected)")
let e2 = Event(t: "a", end: "a", app: "X", bundle: "x", title: "T", src: "ax", text: "hi")
check(e2.line == #"{"t":"a","end":"a","app":"X","bundle":"x","title":"T","src":"ax","text":"hi"}"#, "src before text", e2.line)
check(jsonString("a\u{01}b") == #""a\u0001b""#, "control chars escaped")
if let obj = try? JSONSerialization.jsonObject(with: Data(e1.line.utf8)) as? [String: String] {
    check(obj["text"] == e1.text && obj["title"] == e1.title, "line parses back")
} else { check(false, "line is valid JSON") }

// MARK: dedupe / extend

let logURL = tmp.appendingPathComponent("s/events.jsonl")
let log = try! EventLog(url: logURL)
func ev(_ t: String, _ text: String, app: String = "Google Chrome") -> Event {
    Event(t: t, end: t, app: app, bundle: "com.google.Chrome", title: "Pricing — Linear", text: text)
}
func lines() -> [[String: String]] {
    let s = try! String(contentsOf: logURL, encoding: .utf8)
    check(s.isEmpty || s.hasSuffix("\n"), "file ends with newline")
    return s.split(separator: "\n").map { try! JSONSerialization.jsonObject(with: Data($0.utf8)) as! [String: String] }
}
check(try! log.record(ev("2026-10-19T10:01:03Z", "héllo ✓")) == .appended, "first sample appends")
check(try! log.record(ev("2026-10-19T10:01:06Z", "héllo ✓")) == .extended, "same sample extends")
check(try! log.record(ev("2026-10-19T10:01:42Z", "héllo ✓")) == .extended, "extends again")
var l = lines()
check(l.count == 1 && l[0]["t"] == "2026-10-19T10:01:03Z" && l[0]["end"] == "2026-10-19T10:01:42Z", "end moved", "\(l)")
check(try! log.record(ev("2026-10-19T10:01:45Z", "new text")) == .appended, "changed text appends")
check(try! log.record(ev("2026-10-19T10:01:48Z", "new text", app: "Figma")) == .appended, "changed app appends")
check(try! log.record(ev("2026-10-19T10:01:51Z", "new text", app: "Figma")) == .extended, "extends the new last")
l = lines()
check(l.count == 3, "three lines", "\(l.count)")
check(l[1]["end"] == "2026-10-19T10:01:45Z" && l[2]["end"] == "2026-10-19T10:01:51Z", "only the last line extended")
log.breakRun()
check(try! log.record(ev("2026-10-19T10:01:54Z", "new text", app: "Figma")) == .appended, "after a break, a repeat appends")
check(lines().count == 4, "four lines")

// MARK: session.json, state.json, control

let sj = sessionJSON(id: "20261019-100103", startedAt: date, endedAt: nil, host: "Mac")
check(sj == #"{"id":"20261019-100103","started_at":"2026-10-19T10:01:03Z","ended_at":null,"host":"Mac","version":1}"#, "session.json", sj)
let sj2 = sessionJSON(id: "x", startedAt: date, endedAt: date.addingTimeInterval(60), host: "Mac")
check(sj2.contains(#""ended_at":"2026-10-19T10:02:03Z""#), "session.json ended_at", sj2)
check(stateJSON(recording: false, sessionID: nil, startedAt: nil, pid: 42) ==
      #"{"recording":false,"session_id":null,"started_at":null,"pid":42,"picking":false,"last_error":null,"events":0,"skip":null}"#, "state.json idle")
check(stateJSON(recording: true, sessionID: "20261019-100103", startedAt: date, pid: 7) ==
      #"{"recording":true,"session_id":"20261019-100103","started_at":"2026-10-19T10:01:03Z","pid":7,"picking":false,"last_error":null,"events":0,"skip":null}"#,
      "state.json recording")
check(stateJSON(recording: false, sessionID: nil, startedAt: nil, pid: 1, picking: true) ==
      #"{"recording":false,"session_id":null,"started_at":null,"pid":1,"picking":true,"last_error":null,"events":0,"skip":null}"#, "state.json picking")
check(stateJSON(recording: false, sessionID: nil, startedAt: nil, pid: 1, lastError: "cancelled") ==
      #"{"recording":false,"session_id":null,"started_at":null,"pid":1,"picking":false,"last_error":"cancelled","events":0,"skip":null}"#,
      "state.json cancelled")

check(ControlCommand.parse(Data(#"{"command":"start","at":1792404063000}"#.utf8)) ==
      ControlCommand(kind: .start, at: 1_792_404_063_000), "control start")
check(ControlCommand.parse(Data(#"{"command":"stop","at":5}"#.utf8))?.kind == .stop, "control stop")
check(ControlCommand.parse(Data(#"{"command":"pause","at":5}"#.utf8)) == nil, "control unknown")
check(ControlCommand.parse(Data("garbage".utf8)) == nil, "control garbage")


// MARK: picked content

let display = CGRect(x: 0, y: 0, width: 1512, height: 982)
let second = CGRect(x: 1512, y: 0, width: 1920, height: 1080)
let win = WindowInfo(id: 42, pid: 900, frame: CGRect(x: 100, y: 50, width: 800, height: 600))
let winOnSecond = WindowInfo(id: 43, pid: 900, frame: CGRect(x: 1600, y: 100, width: 800, height: 600))
check(Membership.contains(PickedContent(style: .display, rect: display), win), "display holds the window")
check(!Membership.contains(PickedContent(style: .display, rect: display), winOnSecond), "other display: no")
check(Membership.contains(PickedContent(style: .display, rect: second), winOnSecond), "second display")
check(!Membership.contains(PickedContent(style: .display, rect: .zero), win), "empty display rect: no")
check(Membership.contains(PickedContent(style: .window, rect: .zero, windowIDs: [7, 42]), win), "picked window by id")
check(!Membership.contains(PickedContent(style: .window, rect: win.frame, windowIDs: [7]), win), "ids known: frame alone not enough")
check(Membership.contains(PickedContent(style: .window, rect: CGRect(x: 103, y: 48, width: 796, height: 604)), win),
      "no ids: same frame within tolerance")
check(!Membership.contains(PickedContent(style: .window, rect: CGRect(x: 100, y: 50, width: 400, height: 600)), win),
      "no ids: different size")
check(Membership.contains(PickedContent(style: .application, rect: display, pids: [900]), win), "picked app by pid")
check(!Membership.contains(PickedContent(style: .application, rect: display, pids: [1]), win), "other app")
check(!Membership.contains(PickedContent(style: .application, rect: display), win), "app, pids unknown: no")
check(!Membership.contains(PickedContent(style: .unknown, rect: display), win), "unknown style: no")

func near(_ a: CGRect?, _ b: CGRect) -> Bool {
    guard let a else { return false }
    return abs(a.minX - b.minX) < 1e-6 && abs(a.minY - b.minY) < 1e-6
        && abs(a.width - b.width) < 1e-6 && abs(a.height - b.height) < 1e-6
}
let full = CGRect(x: 0, y: 0, width: 1, height: 1)
check(near(Membership.ocrRegion(PickedContent(style: .window, rect: win.frame), window: win.frame), full), "window: OCR whole")
let d = CGRect(x: 0, y: 0, width: 1000, height: 500)
check(near(Membership.ocrRegion(PickedContent(style: .display, rect: d), window: CGRect(x: 100, y: 0, width: 500, height: 250)),
           CGRect(x: 0.1, y: 0.5, width: 0.5, height: 0.5)), "display: crop, bottom-left origin")
check(near(Membership.ocrRegion(PickedContent(style: .display, rect: d), window: CGRect(x: 800, y: 400, width: 500, height: 500)),
           CGRect(x: 0.8, y: 0, width: 0.2, height: 0.2)), "crop clipped to the display")
check(Membership.ocrRegion(PickedContent(style: .display, rect: d), window: CGRect(x: 2000, y: 0, width: 10, height: 10)) == nil,
      "window off the display: no region")
check(near(Membership.ocrRegion(PickedContent(style: .display, rect: second), window: CGRect(x: 1512, y: 0, width: 960, height: 1080)),
           CGRect(x: 0, y: 0, width: 0.5, height: 1)), "second display offset")

check(stateJSON(recording: true, sessionID: "s", startedAt: nil, pid: 1, events: 3, skip: .notPicked)
      .hasSuffix(#""events":3,"skip":"not_picked"}"#), "state.json events + skip")

// MARK: front window choice

let chromeStrip = ListedWindow(id: 1, pid: 50, owner: "Google Chrome", alpha: 0,
                               frame: CGRect(x: 0, y: 16, width: 1440, height: 32))
let zenStrip = ListedWindow(id: 2, pid: 50, owner: "Google Chrome", frame: CGRect(x: 0, y: 30, width: 1440, height: 68))
let chromeMain = ListedWindow(id: 3, pid: 50, owner: "Google Chrome", frame: CGRect(x: 0, y: 30, width: 1440, height: 792))
let badge = ListedWindow(id: 4, pid: 50, owner: "Google Chrome", frame: CGRect(x: 10, y: 10, width: 20, height: 20))
check(WindowList.front(pid: 50, in: [chromeStrip, chromeMain])?.id == 3, "alpha-0 strip skipped")
check(WindowList.front(pid: 50, in: [zenStrip, chromeMain])?.id == 3, "visible strip gives way to its window")
check(WindowList.front(pid: 50, in: [badge, chromeMain])?.id == 3, "tiny window skipped")
check(WindowList.front(pid: 50, in: [chromeStrip, badge]) == nil, "no normal window")
let floating = ListedWindow(id: 5, pid: 50, layer: 3, frame: CGRect(x: 0, y: 0, width: 300, height: 300))
check(WindowList.front(pid: 50, in: [floating, chromeMain])?.id == 3, "floating panel is not the window")
let dialog = ListedWindow(id: 6, pid: 50, frame: CGRect(x: 2000, y: 100, width: 400, height: 300))
check(WindowList.front(pid: 50, in: [dialog, chromeMain])?.id == 6, "separate front window kept")

let dockNormal = ListedWindow(id: 90, pid: 1, owner: "Dock", layer: 20, frame: CGRect(x: 0, y: 0, width: 1440, height: 900))
let missionControl = ListedWindow(id: 91, pid: 1, owner: "Dock", layer: 18, frame: CGRect(x: 0, y: 0, width: 1440, height: 900))
check(!WindowList.overlayActive([dockNormal, chromeMain]), "normal Dock is not an overlay")
check(WindowList.overlayActive([dockNormal, missionControl, chromeMain]), "Mission Control detected")

// MARK: freshness

let switchAt = Date(timeIntervalSince1970: 1000)
check(!Freshness.ok(frameAt: nil, changedAt: nil), "no frame: not ok")
check(Freshness.ok(frameAt: switchAt, changedAt: nil), "no switch yet: ok")
check(!Freshness.ok(frameAt: switchAt.addingTimeInterval(-2), changedAt: switchAt), "frame from before the switch")
check(!Freshness.ok(frameAt: switchAt.addingTimeInterval(0.05), changedAt: switchAt), "frame during the switch")
check(Freshness.ok(frameAt: switchAt.addingTimeInterval(0.5), changedAt: switchAt), "frame after the switch")

// MARK: OCR plan (crop + private masks)

let main = CGRect(x: 0, y: 0, width: 1440, height: 900)
let ext = CGRect(x: 1440, y: 48, width: 1920, height: 1080) // external display, offset like a real one
let pickMain = PickedContent(style: .display, rect: main)
let chrome = ListedWindow(id: 10, pid: 50, owner: "Google Chrome", frame: CGRect(x: 0, y: 30, width: 720, height: 870))
let whatsBehind = ListedWindow(id: 11, pid: 60, owner: "WhatsApp", frame: CGRect(x: 0, y: 30, width: 1440, height: 870),
                               excluded: true)
let whatsFloat = ListedWindow(id: 12, pid: 60, owner: "WhatsApp", layer: 3,
                              frame: CGRect(x: 0, y: 30, width: 200, height: 200), excluded: true)
let whatsBig = ListedWindow(id: 13, pid: 60, owner: "WhatsApp", frame: CGRect(x: 0, y: 0, width: 1440, height: 900),
                            excluded: true)
let slack = ListedWindow(id: 14, pid: 70, owner: "Slack", layer: 3, frame: CGRect(x: 0, y: 30, width: 200, height: 200))

check(Membership.ocrDecision(pickMain, front: chrome, list: [chrome, whatsBehind]) ==
      .read(region: CGRect(x: 0, y: 0, width: 0.5, height: 870.0 / 900), masks: []),
      "crop to the front window; private window behind it is not in the crop")
if case .read(let region, let masks) = Membership.ocrDecision(pickMain, front: chrome, list: [whatsFloat, chrome, whatsBehind]) {
    check(region.width == 0.5, "crop kept with a private window on top")
    check(masks.count == 1 && near(masks.first, CGRect(x: 0, y: 670.0 / 900, width: 200.0 / 1440, height: 200.0 / 900)),
          "private window on top is masked", "\(masks)")
} else { check(false, "private window on top: read with a mask") }
let banner = ListedWindow(id: 15, pid: 80, owner: "Notification Center", layer: 23,
                          frame: CGRect(x: 1060, y: 40, width: 370, height: 90))
let chromeWide = ListedWindow(id: 16, pid: 50, frame: CGRect(x: 0, y: 30, width: 1440, height: 870))
if case .read(_, let masks) = Membership.ocrDecision(pickMain, front: chromeWide, list: [banner, chromeWide]) {
    check(masks.count == 1, "notification banner is masked")
} else { check(false, "banner: read with a mask") }
let widget = ListedWindow(id: 17, pid: 80, owner: "Notification Center", layer: -2147483601,
                          frame: CGRect(x: 8, y: 56, width: 360, height: 180))
check(!WindowList.isPrivate(widget), "desktop widget is not a banner")
check(Membership.ocrDecision(pickMain, front: chrome, list: [whatsBig, chrome]) == .skip(.privateWindow),
      "private window covering the front window: skip")
check(Membership.ocrDecision(pickMain, front: chrome, list: [slack, chrome]) ==
      .read(region: CGRect(x: 0, y: 0, width: 0.5, height: 870.0 / 900), masks: []), "normal app on top: no mask")
check(Membership.ocrDecision(PickedContent(style: .display, rect: .zero), front: chrome, list: [chrome]) ==
      .skip(.unknownDisplay), "unknown display: skip")
let onExt = ListedWindow(id: 20, pid: 50, frame: CGRect(x: 1440, y: 48, width: 960, height: 1080))
check(Membership.ocrDecision(pickMain, front: onExt, list: [onExt]) == .skip(.notPicked), "window on another display")
check(Membership.ocrDecision(PickedContent(style: .display, rect: ext), front: onExt, list: [onExt]) ==
      .read(region: CGRect(x: 0, y: 0, width: 0.5, height: 1), masks: []), "external display crop")
check(Membership.ocrDecision(PickedContent(style: .window, rect: chrome.frame, windowIDs: [10]), front: chrome,
                             list: [whatsFloat, chrome]) == .read(region: full, masks: []),
      "picked window: read whole, other apps are not in the image")

// MARK: picked display place

let displays = [main, ext]
check(Membership.resolveDisplay(ext, displays: displays) == ext, "global display rect kept")
check(Membership.resolveDisplay(CGRect(x: 0, y: 0, width: 1920, height: 1080), displays: displays) == ext,
      "(0,0)-based external display found by size")
check(Membership.resolveDisplay(CGRect(x: 0, y: 0, width: 1920, height: 1080),
                                displays: [main, ext, CGRect(x: 3360, y: 0, width: 1920, height: 1080)]) == nil,
      "two same-size displays, no origin: unknown")
check(Membership.resolveDisplay(.zero, displays: displays) == nil, "empty rect: unknown")

// MARK: accessibility text

let page: [AXNode] = [
    AXNode(depth: 0, role: "AXWindow", title: "Pricing — Linear"),
    AXNode(depth: 1, role: "AXWebArea", title: "Pricing"),
    AXNode(depth: 2, role: "AXHeading", title: "Pricing page   redesign"),
    AXNode(depth: 3, role: "AXStaticText", value: "Pricing page redesign"),
    AXNode(depth: 2, role: "AXStaticText", value: "We keep three tiers.\nDrop the free plan."),
    AXNode(depth: 2, role: "AXButton", title: "Submit"),
    AXNode(depth: 2, role: "AXTextField", subrole: "AXSecureTextField", value: "hunter2"),
    AXNode(depth: 2, role: "AXGroup", subrole: "AXSecureTextField"),
    AXNode(depth: 3, role: "AXStaticText", value: "inside-secure-group"),
    AXNode(depth: 2, role: "AXSecureTextField", value: "s3cret"),
    AXNode(depth: 2, role: "AXLink", title: "", desc: "Docs link"),
    AXNode(depth: 2, role: "AXCell", value: "Q4"),
    AXNode(depth: 2, role: "AXTextArea", value: "notes here"),
]
let assembled = AXText.assemble(page)
check(assembled == "Pricing page redesign\nWe keep three tiers.\nDrop the free plan.\nDocs link\nQ4\nnotes here",
      "AX text assembly", assembled)
check(!assembled.contains("hunter2") && !assembled.contains("s3cret") && !assembled.contains("inside-secure"), "secure skipped")
check(!assembled.contains("Submit"), "buttons are not text")
check(AXText.isSecure(role: "AXTextField", subrole: "AXSecureTextField"), "secure subrole")
check(AXText.isSecure(role: "AXSecureTextField", subrole: ""), "secure role")
check(!AXText.isSecure(role: "AXTextField", subrole: "AXSearchField"), "search field is not secure")
let many = (0..<500).map { AXNode(depth: 1, role: "AXStaticText", value: "line number \($0) with some words") }
check(AXText.assemble(many, maxChars: 300).count <= 340, "assembly stops near maxChars")
check(AXText.assemble([]) == "", "empty tree")

// MARK: AX or OCR

let longText = String(repeating: "Real words here. ", count: 10)
check(AXText.choose(trusted: true, axText: longText) == .ax, "enough AX text: ax")
check(AXText.choose(trusted: false, axText: longText) == .ocr, "not trusted: ocr")
check(AXText.choose(trusted: true, axText: "File Edit View") == .ocr, "too little AX text: ocr")
check(AXText.choose(trusted: true, axText: String(repeating: "- ", count: 200)) == .ocr, "punctuation does not count")
check(AXText.choose(trusted: true, axText: String(repeating: "a", count: 80)) == .ax, "80 letters is enough")
check(AXText.choose(trusted: true, axText: String(repeating: "a", count: 79)) == .ocr, "79 is not")

try? FileManager.default.removeItem(at: tmp)
print("\(passes) passed, \(failures) failed")
exit(failures == 0 ? 0 : 1)
