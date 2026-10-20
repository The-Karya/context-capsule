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
check(Redactor.clean(long).count == 10_000, "normal long text is kept whole")
check(Redactor.clean(String(repeating: "word ", count: 30_000)).count == 100_000, "safety cap at 100,000")
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
      #"{"recording":false,"session_id":null,"started_at":null,"pid":42,"picking":false,"last_error":null,"events":0,"skip":null,"listening":false,"note_at":null,"project":null,"project_by":null}"#, "state.json idle")
check(stateJSON(recording: true, sessionID: "20261019-100103", startedAt: date, pid: 7) ==
      #"{"recording":true,"session_id":"20261019-100103","started_at":"2026-10-19T10:01:03Z","pid":7,"picking":false,"last_error":null,"events":0,"skip":null,"listening":false,"note_at":null,"project":null,"project_by":null}"#,
      "state.json recording")
check(stateJSON(recording: false, sessionID: nil, startedAt: nil, pid: 1, picking: true) ==
      #"{"recording":false,"session_id":null,"started_at":null,"pid":1,"picking":true,"last_error":null,"events":0,"skip":null,"listening":false,"note_at":null,"project":null,"project_by":null}"#, "state.json picking")
check(stateJSON(recording: false, sessionID: nil, startedAt: nil, pid: 1, lastError: "cancelled") ==
      #"{"recording":false,"session_id":null,"started_at":null,"pid":1,"picking":false,"last_error":"cancelled","events":0,"skip":null,"listening":false,"note_at":null,"project":null,"project_by":null}"#,
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
      .hasSuffix(#""events":3,"skip":"not_picked","listening":false,"note_at":null,"project":null,"project_by":null}"#), "state.json events + skip")

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

// MARK: voice notes — silence

check(Voice.peak([0, -0.5, 0.25]) == 0.5, "peak is the loudest absolute sample")
check(Voice.peak([]) == 0, "empty clip peak 0")
let words = VoiceClip(text: "We keep three tiers", peak: 0.2, seconds: 3)
check(!Voice.isEmpty(words), "real speech is kept")
check(Voice.isEmpty(VoiceClip(text: "", peak: 0.2, seconds: 3)), "no words: empty")
check(Voice.isEmpty(VoiceClip(text: "  \n", peak: 0.2, seconds: 3)), "blank words: empty")
check(Voice.isEmpty(VoiceClip(text: "hi", peak: 0.0003, seconds: 3)), "dead mic noise floor: silent")
check(Voice.isEmpty(VoiceClip(text: "hi", peak: 0, seconds: 3)), "digital zeros: silent")
check(!Voice.isEmpty(VoiceClip(text: "hi", peak: 0.03, seconds: 3)), "quiet speech is not silent")
check(Voice.isEmpty(VoiceClip(text: "hi", peak: 0.5, seconds: 0.2)), "double press is too short")

// MARK: voice notes — event line

let vStart = Date(timeIntervalSince1970: 1_792_404_063)
let vEvent = Voice.event(text: "  Use annual pricing\npassword: hunter2  ", kind: Event.voice, src: "voice", start: vStart,
                         end: vStart.addingTimeInterval(9), app: "Figma", bundle: "com.figma.Desktop", title: "Pricing v2")
check(vEvent.line == #"{"t":"2026-10-19T10:01:03Z","end":"2026-10-19T10:01:12Z","kind":"voice","src":"voice","app":"Figma","bundle":"com.figma.Desktop","title":"Pricing v2","text":"Use annual pricing\n[redacted]"}"#,
      "voice event line: kind + src, trimmed, redacted", vEvent.line)
let longVoice = Voice.event(text: String(repeating: "word ", count: 2000), kind: Event.voice, src: "voice",
                            start: vStart, end: vStart, app: "", bundle: "", title: "")
check(longVoice.text.count == 9_999, "voice text kept whole")
let noteLine = Voice.event(text: "typed context", kind: Event.note, src: "typed", start: vStart, end: vStart,
                           app: "", bundle: "", title: "").line
check(noteLine.contains(#""kind":"note","src":"typed""#), "typed note line", noteLine)

let vLogURL = tmp.appendingPathComponent("v/events.jsonl")
let vLog = try! EventLog(url: vLogURL)
let screen = ev("2026-10-19T10:01:03Z", "same screen")
check(try! vLog.record(screen) == .appended, "screen sample")
check(try! vLog.record(vEvent) == .appended, "voice note never extends a screen event")
check(try! vLog.record(vEvent) == .appended, "two identical voice notes stay two")
check(try! vLog.record(ev("2026-10-19T10:01:20Z", "same screen")) == .appended, "screen after a note starts a new event")
check(vLog.appended == 4, "four lines written")

check(ControlCommand.parse(Data(#"{"command":"note","text":" decided X ","at":9}"#.utf8)) ==
      ControlCommand(kind: .note, at: 9, text: "decided X"), "control note")
check(ControlCommand.parse(Data(#"{"command":"note","text":"  ","at":9}"#.utf8)) == nil, "empty note ignored")
check(ControlCommand.parse(Data(#"{"command":"note","at":9}"#.utf8)) == nil, "note without text ignored")
try! #"{"voice_locale": " en-GB "}"#.write(to: cfgURL, atomically: true, encoding: .utf8)
check(Config.load(from: cfgURL).voiceLocale == "en-GB", "voice_locale from config")
check(!Config.load(from: cfgURL).askProject, "ask_project is off by default")
try! #"{"ask_project": true}"#.write(to: cfgURL, atomically: true, encoding: .utf8)
check(Config.load(from: cfgURL).askProject, "ask_project can be switched on")
try! #"{"voice_locale": ""}"#.write(to: cfgURL, atomically: true, encoding: .utf8)
check(Config.load(from: cfgURL).voiceLocale == nil, "empty voice_locale means default")
check(stateJSON(recording: true, sessionID: "s", startedAt: nil, pid: 1, listening: true, noteAt: 5)
      .hasSuffix(#""listening":true,"note_at":5,"project":null,"project_by":null}"#), "state.json listening + note_at")

// MARK: voice notes — the indicator always resets

final class FakeEngine: VoiceEngine {
    enum Ready { case ok, problem(VoiceOutcome) }
    var ready: Ready = .ok
    var startError: Error?
    var result: Result<VoiceClip, Error>? = .success(VoiceClip(text: "We keep three tiers", peak: 0.2, seconds: 4))
    var holdFinish = false
    var pendingFinish: ((Result<VoiceClip, Error>) -> Void)?
    var micOpen = false
    var cancels = 0
    var deviceName: String? = "MacBook Air Microphone"
    func prepare(locale: Locale, _ done: @escaping (VoiceOutcome?) -> Void) {
        if case .problem(let p) = ready { done(p) } else { done(nil) }
    }
    func start() throws {
        if let startError { throw startError }
        micOpen = true
    }
    func finish(_ done: @escaping (Result<VoiceClip, Error>) -> Void) {
        micOpen = false
        if holdFinish { pendingFinish = done } else if let result { done(result) }
    }
    func cancel() { micOpen = false; cancels += 1 }
}
struct Boom: LocalizedError { var errorDescription: String? { "mic unplugged" } }

final class VoiceRig {
    let engine = FakeEngine()
    let notes: VoiceNotes
    var indicator: [Bool] = []
    var outcomes: [VoiceOutcome] = []
    var written: [Event] = []
    var timers: [() -> Void] = []
    init() {
        notes = VoiceNotes(engine: engine)
        notes.hop = { $0() }
        notes.log = { _ in }
        notes.onIndicator = { [unowned self] in self.indicator.append($0) }
        notes.onOutcome = { [unowned self] in self.outcomes.append($0) }
        notes.schedule = { [unowned self] _, block in self.timers.append(block); return {} }
    }
    var target: VoiceTarget {
        VoiceTarget(app: "Figma", bundle: "com.figma.Desktop", title: "Pricing") { [unowned self] in self.written.append($0) }
    }
    func press(recording: Bool = true) { notes.toggle(locale: Locale(identifier: "en-US"), target: recording ? target : nil) }
    /// Ended idle, with the mic shut and the last indicator call "off".
    var resetOK: Bool { notes.phase == .idle && !engine.micOpen && indicator.last == false }
}

do {
    let r = VoiceRig()
    r.press()
    check(r.notes.phase == .listening && r.indicator == [true] && r.engine.micOpen, "first press opens the mic, mic shown")
    r.press()
    check(r.resetOK && r.outcomes == [.saved(chars: 19, seconds: 4)], "second press saves", "\(r.outcomes)")
    check(r.written.count == 1 && r.written[0].kind == "voice" && r.written[0].app == "Figma", "note written with context")
}
do {
    let r = VoiceRig()
    r.engine.result = .success(VoiceClip(text: "", peak: 0.001, seconds: 3))
    r.press(); r.press()
    check(r.resetOK && r.outcomes == [.silent(device: "MacBook Air Microphone")] && r.written.isEmpty, "silent clip: reset, nothing written")
    check(r.outcomes[0].message.contains("Didn't catch that") && r.outcomes[0].message.contains("MacBook Air Microphone"),
          "silent message names the mic")
}
do {
    let r = VoiceRig()
    r.engine.ready = .problem(.denied(.microphone))
    r.press()
    check(r.resetOK && r.outcomes == [.denied(.microphone)] && !r.indicator.contains(true), "mic denied: never shown, reset")
    check(r.outcomes[0].message.contains("System Settings"), "denied says where to fix it")
}
do {
    let r = VoiceRig()
    r.engine.ready = .problem(.unavailable(locale: "ta-IN"))
    r.press()
    check(r.resetOK && r.outcomes == [.unavailable(locale: "ta-IN")], "no on-device model: reset")
    check(r.outcomes[0].message.contains("nothing is sent to Apple"), "unavailable says nothing is sent")
}
do {
    let r = VoiceRig()
    r.engine.startError = Boom()
    r.press()
    check(r.resetOK && r.outcomes == [.failed("mic unplugged")] && r.engine.cancels == 1, "mic start error: reset")
}
do {
    let r = VoiceRig()
    r.engine.result = .failure(Boom())
    r.press(); r.press()
    check(r.resetOK && r.outcomes == [.failed("mic unplugged")], "recognizer error: reset")
}
do {
    let r = VoiceRig()
    r.press()
    check(r.timers.count == 1, "2-minute timeout armed")
    r.timers[0]()
    check(r.resetOK && r.outcomes.count == 1 && r.written.count == 1, "timeout closes the mic and saves")
    r.timers[0]()
    check(r.outcomes.count == 1, "a stale timeout does nothing")
}
do {
    let r = VoiceRig()
    r.press(recording: false)
    check(r.resetOK || r.indicator.isEmpty, "not recording: mic never opens")
    check(r.outcomes == [.notRecording] && r.outcomes[0].message == "Start contextualizing first (⌥C)", "not recording message")
    check(r.notes.phase == .idle && !r.engine.micOpen, "not recording: idle")
}
do {
    let r = VoiceRig()
    r.press()
    r.notes.recordingWillStop()
    check(r.resetOK && r.written.count == 1, "recording stops mid-note: the note is finished and saved")
}
do {
    let r = VoiceRig()
    r.engine.holdFinish = true
    r.press(); r.press()
    check(r.notes.phase == .finishing && r.indicator.last == false && !r.engine.micOpen, "finishing: mic already off")
    r.press()
    check(r.notes.phase == .finishing, "press while finishing is ignored")
    r.engine.pendingFinish?(.success(VoiceClip(text: "late words", peak: 0.3, seconds: 2)))
    check(r.resetOK && r.written.count == 1, "late transcript still saved, then reset")
}

// MARK: diagnostics log

let appLogURL = tmp.appendingPathComponent("logs/app.log")
let appLog = AppLog(url: appLogURL)
appLog.write("recording started: x", at: date)
appLog.write("voice clip: 1 buffers", at: date)
appLog.flush()
let logText = (try? String(contentsOf: appLogURL, encoding: .utf8)) ?? ""
check(logText == "2026-10-19T10:01:03Z recording started: x\n2026-10-19T10:01:03Z voice clip: 1 buffers\n", "app.log lines", logText)
let big = String(repeating: "x", count: 1000) + "\n"
for _ in 0..<300 { AppLog.append(big, to: appLogURL) }
let size = ((try? FileManager.default.attributesOfItem(atPath: appLogURL.path))?[.size] as? Int) ?? 0
check(size <= AppLog.maxBytes + 2000, "app.log stays small", "\(size)")
check(VoiceOutcome.failed("speech recognition stopped (kAFAssistantErrorDomain 1101)").logSummary
      .contains("kAFAssistantErrorDomain 1101"), "failure log keeps the error code")

// MARK: projects

check(Projects.slug("Client Report (Q3)!") == "client-report-q3", "project slug", Projects.slug("Client Report (Q3)!"))
check(Projects.slug("Café déjà vu") == "cafe-deja-vu", "slug folds accents", Projects.slug("Café déjà vu"))
let longName = String(repeating: "a", count: 59) + " b"
check(Projects.slug(longName) == String(repeating: "a", count: 59), "long slug never ends on a dash", Projects.slug(longName))
let pj = [
    Project(id: "capsule", name: "Context Capsule", match: ["karya-capsule", "context capsule"]),
    Project(id: "report", name: "Client report", match: ["acme", "docs.google.com/document/d/1abc"]),
    Project(id: "design", name: "Design", match: ["Figma"]),
]
check(Projects.detect(pj, title: "main.swift — karya-capsule", location: "", app: "Xcode")?.id == "capsule", "clue in title")
check(Projects.detect(pj, title: "Q3 numbers", location: "https://docs.google.com/document/d/1AbC/edit", app: "Chrome")?.id == "report",
      "clue in location, case-insensitive")
check(Projects.detect(pj, title: "Untitled", location: "", app: "Figma")?.id == "design", "clue is the app")
check(Projects.detect(pj, title: "Inbox", location: "", app: "Mail") == nil, "no clue: no project")
check(Projects.detect(pj, title: "acme in Figma", location: "", app: "Figma")?.id == "design", "longest clue wins a tie")
check(Projects.detect([Project(id: "x", name: "X", match: ["a"])], title: "a", location: "", app: "") == nil, "1-letter clues ignored")
check(ProjectChoice(pinned: "report").resolve(pj, title: "karya-capsule", location: "", app: "")! == ("report", "you"), "pinned wins")
check(ProjectChoice(pinned: nil).resolve(pj, title: "karya-capsule", location: "", app: "")! == ("capsule", "auto"), "auto")
check(ProjectChoice(pinned: nil).resolve(pj, title: "Inbox", location: "", app: "Mail") == nil, "auto, nothing found")

check(Projects.clue(title: "x", location: "file:///Users/s/karya-capsule/SPEC.md") == "SPEC", "clue from a file name")
check(Projects.clue(title: "x", location: "https://linear.app/karya/issue/KAR-12") == "linear.app/karya/issue", "clue from a page")
check(Projects.clue(title: "main.swift — karya-capsule", location: "") == "karya-capsule" || Projects.clue(title: "main.swift — karya-capsule", location: "") == "main.swift",
      "clue from the title")

let pjURL = tmp.appendingPathComponent("projects.json")
try! Projects.save(pj, to: pjURL)
check(Projects.load(from: pjURL) == pj, "projects.json round trip")
try! #"{"projects":[{"name":"No Id Given","match":[" a b ",""]},{"id":"x"}]}"#.write(to: pjURL, atomically: true, encoding: .utf8)
check(Projects.load(from: pjURL) == [Project(id: "no-id-given", name: "No Id Given", match: ["a b"])], "load fills ids, drops bad entries")
check(Projects.load(from: tmp.appendingPathComponent("none.json")).isEmpty, "no projects.json: no projects")

check(Location.clean("https://x.com/a/b?token=sk-abc123&u=1#frag") == "https://x.com/a/b", "location drops query and fragment")
check(Location.clean("  ") == "" && Location.clean(nil) == "", "empty location")

var pe = Event(t: "a", end: "a", app: "Chrome", bundle: "c", title: "T", src: "ax", text: "hi")
pe.location = "https://x.com/a"
pe.project = "report"
pe.projectBy = "auto"
check(pe.line == #"{"t":"a","end":"a","app":"Chrome","bundle":"c","title":"T","where":"https://x.com/a","project":"report","project_by":"auto","src":"ax","text":"hi"}"#,
      "event line with where + project", pe.line)
var pv = Voice.event(text: "why", kind: Event.voice, src: "voice", start: date, end: date, app: "Figma", bundle: "f", title: "T")
pv.project = "design"
pv.projectBy = "you"
check(pv.line.contains(#""kind":"voice","src":"voice","app":"Figma","bundle":"f","title":"T","project":"design","project_by":"you","text":"why"}"#),
      "voice line with project", pv.line)
var pe2 = pe
pe2.project = "capsule"
check(!pe.sameContent(as: pe2), "another project is a new event")

check(ControlCommand.parse(Data(#"{"command":"project","project":"report","at":3}"#.utf8)) ==
      ControlCommand(kind: .project, at: 3, project: "report"), "control project")
check(ControlCommand.parse(Data(#"{"command":"project","at":3}"#.utf8)) == nil, "project without id ignored")
check(stateJSON(recording: true, sessionID: "s", startedAt: nil, pid: 1, project: "report", projectBy: "auto")
      .hasSuffix(#""project":"report","project_by":"auto"}"#), "state.json project")

// MARK: system UI is never the work window

check(WindowList.isSystemUI("com.apple.controlcenter") && WindowList.isSystemUI("com.apple.Spotlight"), "system UI bundles")
check(!WindowList.isSystemUI("com.google.Chrome") && !WindowList.isSystemUI(nil), "apps are not system UI")
let sysList = [
    ListedWindow(id: 1, pid: 10, owner: "Control Center", layer: 0, frame: CGRect(x: 0, y: 0, width: 300, height: 400)),
    ListedWindow(id: 2, pid: 20, owner: "WhatsApp", frame: CGRect(x: 0, y: 30, width: 800, height: 600), excluded: true),
    ListedWindow(id: 3, pid: 30, owner: "Chrome", frame: CGRect(x: 0, y: 30, width: 1400, height: 800)),
]
check(WindowList.topRegularPid(in: sysList, isSystem: { $0 == 10 }) == 20, "a private app on top is kept (then skipped as private)")
check(WindowList.topRegularPid(in: Array(sysList.dropFirst(2)), isSystem: { $0 == 10 }) == 30, "top real app found")
check(WindowList.topRegularPid(in: [sysList[0]], isSystem: { $0 == 10 }) == nil, "only system UI: none")

// MARK: capsule files

let capText = """
---
capsule: 1
title: "Website — 2026-10-19"
author: Saachi            # from config
created: 2026-10-19T18:00:00Z
project: website
kind: daily
date: 2026-10-19
draft: true
---

## In short
We redesigned the pricing page.
Teams is now $12.
## For Claude
You're picking up…
"""
let capURL = tmp.appendingPathComponent("capsules/website-2026-10-19.capsule.md")
let cs = CapsuleSummary.parse(capText, url: capURL)
check(cs.title == "Website — 2026-10-19" && cs.project == "website" && cs.kind == "daily" && cs.date == "2026-10-19",
      "capsule front matter", "\(cs)")
check(cs.author == "Saachi" && cs.draft, "comments stripped, draft read")
check(cs.inShort == "We redesigned the pricing page. Teams is now $12.", "In short line", cs.inShort)
check(CapsuleSummary.parse("no front matter", url: capURL).title == "website-2026-10-19", "title falls back to the file name")
try! FileManager.default.createDirectory(at: capURL.deletingLastPathComponent().appendingPathComponent(".history"), withIntermediateDirectories: true)
try! capText.write(to: capURL, atomically: true, encoding: .utf8)
try! capText.replacingOccurrences(of: "2026-10-19T18", with: "2026-10-20T09").write(
    to: capURL.deletingLastPathComponent().appendingPathComponent("newer.capsule.md"), atomically: true, encoding: .utf8)
try! "x".write(to: capURL.deletingLastPathComponent().appendingPathComponent("notes.md"), atomically: true, encoding: .utf8)
let listed = CapsuleSummary.all(in: capURL.deletingLastPathComponent())
check(listed.map(\.url.lastPathComponent) == ["newer.capsule.md", "website-2026-10-19.capsule.md"], "capsules listed newest first",
      "\(listed.map(\.url.lastPathComponent))")
check(ClaudeLink.newChat(prefill: "Make my capsules for today.")?.absoluteString
      == "claude://claude.ai/new?q=Make%20my%20capsules%20for%20today.", "Claude link with the message typed in")
check(ClaudeLink.dayPrompt.contains("Context Capsule"), "the request names the app")

// MARK: reminder and clean-up

check(Reminder.parse("18:00") == 1080 && Reminder.parse("7:5") == 425 && Reminder.parse("25:00") == nil, "reminder time")
check(Reminder.due(now: 1085, at: "18:00", recordedToday: true, capsuleToday: false, askedToday: false, recording: false), "reminder due")
check(!Reminder.due(now: 1000, at: "18:00", recordedToday: true, capsuleToday: false, askedToday: false, recording: false), "too early")
check(!Reminder.due(now: 1085, at: "18:00", recordedToday: false, capsuleToday: false, askedToday: false, recording: false), "nothing recorded")
check(!Reminder.due(now: 1085, at: "18:00", recordedToday: true, capsuleToday: true, askedToday: false, recording: false), "already made")
check(!Reminder.due(now: 1085, at: "18:00", recordedToday: true, capsuleToday: false, askedToday: true, recording: false), "asked already")
check(!Reminder.due(now: 1085, at: "18:00", recordedToday: true, capsuleToday: false, askedToday: false, recording: true), "still recording")
check(!Reminder.due(now: 1085, at: nil, recordedToday: true, capsuleToday: false, askedToday: false, recording: false), "reminder off")
try! #"{"remind_at": false, "keep_recordings_days": 7}"#.write(to: cfgURL, atomically: true, encoding: .utf8)
check(Config.load(from: cfgURL).remindAt == nil && Config.load(from: cfgURL).keepDays == 7, "reminder off, keep 7 days")
try! #"{"remind_at": "19:30"}"#.write(to: cfgURL, atomically: true, encoding: .utf8)
check(Config.load(from: cfgURL).remindAt == "19:30" && Config.load(from: cfgURL).keepDays == 30, "custom reminder, default keep")
let utc = TimeZone(identifier: "UTC")!
let cleanupNow = Date(timeIntervalSince1970: 1_792_404_063) // 2026-10-19T10:01:03Z
check(Cleanup.oldSessions(["20260918-090000", "20260919-000000", "20261019-100000", "junk"], keepDays: 30, now: cleanupNow, timeZone: utc)
      == ["20260918-090000"], "sessions older than 30 days", "\(Cleanup.oldSessions(["20260918-090000", "20260919-000000"], keepDays: 30, now: cleanupNow, timeZone: utc))")
check(Cleanup.oldSessions(["20200101-000000"], keepDays: 0, now: cleanupNow).isEmpty, "keep forever")

// MARK: several shared screens

let leftScreen = PickedContent(style: .display, rect: CGRect(x: 0, y: 0, width: 1440, height: 900), source: 1)
let rightScreen = PickedContent(style: .display, rect: CGRect(x: 1440, y: 48, width: 1920, height: 1080), source: 2)
let onRight = WindowInfo(id: 7, pid: 1, frame: CGRect(x: 1500, y: 100, width: 800, height: 600))
let onLeft = WindowInfo(id: 8, pid: 1, frame: CGRect(x: 100, y: 100, width: 800, height: 600))
check(Membership.pick(for: onRight, in: [leftScreen, rightScreen])?.source == 2, "the screen holding the window in front")
check(Membership.pick(for: onLeft, in: [leftScreen, rightScreen])?.source == 1, "the other screen")
check(Membership.pick(for: onRight, in: [leftScreen]) == nil, "window on a screen that isn't shared: not recorded")

// MARK: only what's new

let nl = NewLines()
check(nl.filter("Title\nLine one\n\nLine two\nLine one", window: "doc") == "Title\nLine one\nLine two", "first look: every line once")
check(nl.filter("Title\nLine two\nLine three", window: "doc") == "Line three", "next look: only the new line")
check(nl.filter("Title\nLine two", window: "doc") == "", "nothing new")
check(nl.filter("Title\nLine two", window: "other") == "Title\nLine two", "another window starts fresh")
for i in 0..<(NewLines.maxWindows + 1) { _ = nl.filter("x", window: "w\(i)") }
check(nl.filter("Title", window: "doc") == "Title", "least used window forgotten past the limit")
nl.reset()
check(nl.filter("Line two", window: "other") == "Line two", "reset for a new recording")

let extURL = tmp.appendingPathComponent("ext/events.jsonl")
let extLog = try! EventLog(url: extURL)
var w1 = Event(t: "2026-10-19T10:00:00Z", end: "2026-10-19T10:00:00Z", app: "Notes", bundle: "n", title: "Plan", src: "ax", text: "a")
try! extLog.record(w1)
var w1b = w1
w1b.t = "2026-10-19T10:00:03Z"; w1b.end = w1b.t; w1b.text = ""
check(try! extLog.extendIfSameWindow(w1b), "nothing new in the same window: extend")
var w2 = w1b
w2.title = "Other"
check(!(try! extLog.extendIfSameWindow(w2)), "another window: not extended")
let extLines = (try! String(contentsOf: extURL, encoding: .utf8)).split(separator: "\n")
check(extLines.count == 1 && extLines[0].contains(#""end":"2026-10-19T10:00:03Z""#), "end moved in place", "\(extLines)")

// MARK: settings written by the app

let setURL = tmp.appendingPathComponent("settings/config.json")
try! ConfigFile.set("sounds", false, in: setURL)
try! ConfigFile.set("remind_at", "19:30", in: setURL)
try! ConfigFile.set("exclude_apps", ["com.example.app"], in: setURL)
var setCfg = Config.load(from: setURL)
check(!setCfg.sounds && setCfg.remindAt == "19:30" && setCfg.excludeApps == ["com.example.app"], "settings saved from the app")
try! #"{"author": "Saachi", "future_key": 7}"#.write(to: setURL, atomically: true, encoding: .utf8)
try! ConfigFile.set("remind_at", false, in: setURL)
let setText = try! String(contentsOf: setURL, encoding: .utf8)
check(setText.contains("\"author\" : \"Saachi\"") && setText.contains("future_key"), "other keys kept", setText)
setCfg = Config.load(from: setURL)
check(setCfg.remindAt == nil, "reminder turned off")

// MARK: where you look

let scrA = PickedContent(style: .display, rect: CGRect(x: 0, y: 0, width: 1440, height: 900), source: 1)
let scrB = PickedContent(style: .display, rect: CGRect(x: 1440, y: 0, width: 1920, height: 1080), source: 2)
let typing = ListedWindow(id: 1, pid: 10, frame: CGRect(x: 100, y: 100, width: 800, height: 600))
let reading = ListedWindow(id: 2, pid: 20, frame: CGRect(x: 1600, y: 100, width: 900, height: 700))
let lookNow = Date(timeIntervalSince1970: 2000)
check(PointerRule.window(at: CGPoint(x: 1700, y: 300), in: [typing, reading])?.id == 2, "window under the mouse")
check(PointerRule.window(at: CGPoint(x: 5000, y: 300), in: [typing, reading]) == nil, "nothing under the mouse")
check(PointerRule.shouldRead(pointer: reading, pointerPick: scrB, front: typing, frontPick: scrA,
                             lastActivity: lookNow.addingTimeInterval(-5), now: lookNow), "reading on the other screen: read it too")
check(!PointerRule.shouldRead(pointer: reading, pointerPick: scrB, front: typing, frontPick: scrA,
                              lastActivity: lookNow.addingTimeInterval(-60), now: lookNow), "mouse parked a minute ago: not")
check(!PointerRule.shouldRead(pointer: reading, pointerPick: scrB, front: typing, frontPick: scrA,
                              lastActivity: nil, now: lookNow), "mouse never moved: not")
check(!PointerRule.shouldRead(pointer: typing, pointerPick: scrA, front: typing, frontPick: scrA,
                              lastActivity: lookNow, now: lookNow), "mouse on the window in use: once is enough")
check(!PointerRule.shouldRead(pointer: reading, pointerPick: scrA, front: typing, frontPick: scrA,
                              lastActivity: lookNow, now: lookNow), "same screen: the window in use is enough")
check(PointerRule.shouldRead(pointer: reading, pointerPick: scrB, front: typing, frontPick: nil,
                             lastActivity: lookNow, now: lookNow), "window in use isn't shared: read where you look")
check(!PointerRule.shouldRead(pointer: reading, pointerPick: nil, front: typing, frontPick: scrA,
                              lastActivity: lookNow, now: lookNow), "mouse over a screen that isn't shared: never")
var pointerEvent = Event(t: "a", end: "a", app: "Safari", bundle: "s", title: "Docs", src: "ax", text: "x")
pointerEvent.focus = "pointer"
check(pointerEvent.line == #"{"t":"a","end":"a","app":"Safari","bundle":"s","title":"Docs","focus":"pointer","src":"ax","text":"x"}"#,
      "pointer event line", pointerEvent.line)

// MARK: time per project

let statRoot = tmp.appendingPathComponent("stat-sessions")
let statDir = statRoot.appendingPathComponent("20261019-100000")
try! FileManager.default.createDirectory(at: statDir, withIntermediateDirectories: true)
try! """
{"t":"2026-10-19T10:00:00Z","end":"2026-10-19T10:30:00Z","app":"Code","bundle":"c","title":"x","project":"web","project_by":"auto","src":"ax","text":"a"}
{"t":"2026-10-19T10:30:00Z","end":"2026-10-19T10:30:00Z","app":"Code","bundle":"c","title":"x","project":"web","project_by":"auto","src":"ax","text":"b"}
{"t":"2026-10-19T10:31:00Z","end":"2026-10-19T10:41:00Z","app":"Slack","bundle":"s","title":"y","src":"ax","text":"c"}
{"t":"2026-10-19T10:35:00Z","end":"2026-10-19T10:45:00Z","app":"Safari","bundle":"f","title":"z","focus":"pointer","src":"ax","text":"d"}
{"t":"2026-10-19T10:40:00Z","end":"2026-10-19T10:41:00Z","kind":"voice","src":"voice","app":"Code","bundle":"c","title":"x","text":"why"}
{"t":"2026-10-20T09:00:00Z","end":"2026-10-20T09:10:00Z","app":"Code","bundle":"c","title":"x","project":"web","project_by":"auto","src":"ax","text":"e"}
""".write(to: statDir.appendingPathComponent("events.jsonl"), atomically: true, encoding: .utf8)
let dayStart = Date(timeIntervalSince1970: 1_792_368_000) // 2026-10-19T00:00:00Z
let stats = WorkStats.totals(sessions: statRoot, from: dayStart, to: dayStart.addingTimeInterval(86_400))
check(stats.byProject.map(\.id) == ["web", ""] && stats.byProject[0].seconds == 1803 && stats.byProject[1].seconds == 600,
      "time per project: notes and looked-at windows don't count", "\(stats.byProject)")
check(stats.seconds == 2403, "total time")
try! #"{"assign":[{"project":"chat","from":"2026-10-19T10:31:00Z","to":"2026-10-19T10:32:00Z"}]}"#
    .write(to: statDir.appendingPathComponent("assign.json"), atomically: true, encoding: .utf8)
let sorted = WorkStats.totals(sessions: statRoot, from: dayStart, to: dayStart.addingTimeInterval(86_400))
check(sorted.byProject.map(\.id) == ["web", "chat"], "Claude's corrections count", "\(sorted.byProject)")
try? FileManager.default.removeItem(at: statDir.appendingPathComponent("assign.json"))
let byClue = WorkStats.totals(sessions: statRoot, from: dayStart, to: dayStart.addingTimeInterval(86_400),
                              projects: [Project(id: "team", name: "Team", match: ["Slack"])])
check(byClue.byProject.map(\.id) == ["web", "team"], "clues made later count too", "\(byClue.byProject)")
check(WorkStats.duration(40) == "40 s" && WorkStats.duration(720) == "12 m" && WorkStats.duration(3900) == "1 h 05 m", "durations")
check(WorkStats.greeting(hour: 9) == "Good morning" && WorkStats.greeting(hour: 14) == "Good afternoon"
      && WorkStats.greeting(hour: 20) == "Good evening", "greetings")

try? FileManager.default.removeItem(at: tmp)
print("\(passes) passed, \(failures) failed")
exit(failures == 0 ? 0 : 1)
