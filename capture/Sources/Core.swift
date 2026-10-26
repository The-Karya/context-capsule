// context-capsule — capture: pure logic (paths, JSON, times, redaction, exclusion, dedupe, event log,
// picked-content membership, Accessibility text assembly, AX-vs-OCR choice, voice-note logic).
// No UI or screen access here, so the tests can compile this file on its own.

import CoreGraphics
import Foundation

// MARK: - The app's name

enum AppName {
    /// Where people see the app's name: titles, menus, the menu-bar label, About, the disk image.
    static let full = "Karya Context Capsule"
    /// The bundle's file name stays "Context Capsule.app": AI apps connected before 0.3 start
    /// Contents/Resources/context-capsule-mcp by its full path, and updates swap the bundle in place.
    /// Finder, the Dock and Spotlight show `full` (en.lproj/InfoPlist.strings, see build.sh).
    static let bundleFile = "Context Capsule.app"
}

// MARK: - Paths

struct Paths {
    let root: URL
    var sessions: URL { root.appendingPathComponent("sessions", isDirectory: true) }
    var config: URL { root.appendingPathComponent("config.json") }
    var control: URL { root.appendingPathComponent("control") }
    var state: URL { root.appendingPathComponent("state.json") }
    /// Diagnostics: counts, durations, error codes and reasons only — never screen text or words.
    var log: URL { root.appendingPathComponent("app.log") }
    var projects: URL { root.appendingPathComponent("projects.json") }
    var capsules: URL { root.appendingPathComponent("capsules", isDirectory: true) }

    /// `~/.context-capsule`, or CONTEXT_CAPSULE_HOME when it is set (as on Windows and Linux; for tests and renders).
    static let standard = Paths(root: root(env: ProcessInfo.processInfo.environment["CONTEXT_CAPSULE_HOME"]))

    static func root(env: String?) -> URL {
        let home = FileManager.default.homeDirectoryForCurrentUser
        guard let dir = env?.trimmingCharacters(in: .whitespaces), !dir.isEmpty else {
            return home.appendingPathComponent(".context-capsule", isDirectory: true)
        }
        if dir == "~" { return home }
        if dir.hasPrefix("~/") { return home.appendingPathComponent(String(dir.dropFirst(2)), isDirectory: true) }
        return URL(fileURLWithPath: dir, isDirectory: true)
    }
}

// MARK: - JSON (hand-written so key order matches SPEC.md)

enum JSONValue {
    case string(String), int(Int), bool(Bool), null
}

func jsonString(_ s: String) -> String {
    var out = "\""
    for u in s.unicodeScalars {
        switch u {
        case "\"": out += "\\\""
        case "\\": out += "\\\\"
        case "\n": out += "\\n"
        case "\r": out += "\\r"
        case "\t": out += "\\t"
        case "\u{2028}": out += "\\u2028"
        case "\u{2029}": out += "\\u2029"
        default:
            if u.value < 0x20 { out += String(format: "\\u%04x", u.value) } else { out.unicodeScalars.append(u) }
        }
    }
    return out + "\""
}

func jsonObject(_ pairs: [(String, JSONValue)]) -> String {
    let body = pairs.map { key, value -> String in
        let v: String
        switch value {
        case .string(let s): v = jsonString(s)
        case .int(let i): v = String(i)
        case .bool(let b): v = b ? "true" : "false"
        case .null: v = "null"
        }
        return jsonString(key) + ":" + v
    }
    return "{" + body.joined(separator: ",") + "}"
}

/// Write a whole file at once, so readers never see half of it.
func writeAtomically(_ text: String, to url: URL) throws {
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try Data(text.utf8).write(to: url, options: .atomic)
}

// MARK: - Times

enum Clock {
    /// ISO-8601 UTC, whole seconds: 2026-10-19T10:01:03Z
    static func iso(_ date: Date) -> String {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        f.timeZone = TimeZone(identifier: "UTC")
        return f.string(from: date)
    }

    /// Session id in local time: YYYYMMDD-HHMMSS
    static func sessionID(_ date: Date, timeZone: TimeZone = .current) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.calendar = Calendar(identifier: .gregorian)
        f.timeZone = timeZone
        f.dateFormat = "yyyyMMdd-HHmmss"
        return f.string(from: date)
    }
}

// MARK: - Redaction

enum Redactor {
    static let mark = "[redacted]"
    /// A safety net only (a whole book read through Accessibility), not a working limit: only new lines are saved.
    static let textCap = 100_000

    private static func re(_ p: String) -> NSRegularExpression {
        // Patterns are constants; a typo should fail loudly in the tests.
        try! NSRegularExpression(pattern: p)
    }

    /// Whole lines like `password: hunter2` or `"db_password": "x"`.
    private static let passwordLine = re(#"(?im)^.*(?<![a-z])(password|passwd|passphrase|pwd)["']?\s*[:=].*$"#)

    /// Known token shapes, always redacted.
    private static let tokens: [NSRegularExpression] = [
        re(#"sk-[A-Za-z0-9_\-]{16,}"#),                    // OpenAI / Anthropic style
        re(#"gh[pousr]_[A-Za-z0-9]{20,}"#),                // GitHub tokens
        re(#"github_pat_[A-Za-z0-9_]{20,}"#),
        re(#"xox[bpoasr]-[A-Za-z0-9\-]{10,}"#),            // Slack
        re(#"(?:AKIA|ASIA)[0-9A-Z]{16}"#),                 // AWS access key id
        re(#"(?i)bearer\s+[A-Za-z0-9._\-+/=]{16,}"#),      // Authorization headers
        re(#"(?<![A-Za-z0-9])[0-9a-fA-F]{33,}(?![A-Za-z0-9])"#), // long hex
    ]

    /// Long base64-ish runs; only redacted when they look random (see `looksRandom`).
    private static let base64ish = re(#"[A-Za-z0-9+/_\-]{33,}={0,2}"#)

    /// Mixed case + digits and a long unbroken alphanumeric run: a key, not a path or a word.
    static func looksRandom(_ s: String) -> Bool {
        let hasDigit = s.contains { $0.isASCII && $0.isNumber }
        let hasUpper = s.contains { $0.isASCII && $0.isUppercase }
        let hasLower = s.contains { $0.isASCII && $0.isLowercase }
        let longestRun = s.split(whereSeparator: { !($0.isASCII && ($0.isLetter || $0.isNumber)) })
            .map(\.count).max() ?? 0
        return hasDigit && hasUpper && hasLower && longestRun >= 16
    }

    static func redact(_ text: String) -> String {
        var s = text
        s = replace(passwordLine, in: s)
        for r in tokens { s = replace(r, in: s) }
        let ns = NSMutableString(string: s)
        for m in base64ish.matches(in: s, range: NSRange(location: 0, length: ns.length)).reversed() {
            if looksRandom(ns.substring(with: m.range)) { ns.replaceCharacters(in: m.range, with: mark) }
        }
        return ns as String
    }

    /// Redact, then keep the first `cap` characters.
    static func clean(_ text: String, cap: Int = textCap) -> String {
        let r = redact(text)
        return r.count > cap ? String(r.prefix(cap)) : r
    }

    private static func replace(_ r: NSRegularExpression, in s: String) -> String {
        r.stringByReplacingMatches(in: s, range: NSRange(s.startIndex..., in: s),
                                   withTemplate: NSRegularExpression.escapedTemplate(for: mark))
    }
}

// MARK: - Never-record list

struct Config {
    var excludeApps: [String] = []
    /// Locale for voice notes, e.g. "en-GB". nil: the user's locale.
    var voiceLocale: String? = nil
    /// Ask "What are you working on?" when a recording's first window matches no project. Off by default:
    /// known projects are detected from clues, and Claude sorts new work at the end of the day.
    var askProject = false
    /// Evening reminder "Make today's capsules?" at this local time ("18:00"); nil = off.
    var remindAt: String? = "18:00"
    /// Raw recordings older than this many days are deleted (capsules are kept); 0 = keep forever.
    var keepDays = 30
    /// Soft sounds when recording / the mic starts and stops.
    var sounds = true
    /// Download and prepare new versions by themselves (they install only when you say Restart).
    var autoUpdate = true
    /// Base URL of latest.json and latest.json.sig; nil = the GitHub release (Update.defaultFeedBase).
    var updateFeed: String? = nil
    /// "share_screens": "all" = starting shares every screen at once, with no picker (needs Screen Recording);
    /// anything else = the picker, one screen or window.
    var shareAllScreens = false
    /// "make_capsules": "auto" = Automatic capsules at remindAt (18:00 when off); anything else = "click".
    var makeCapsules = "click"
    /// "appearance": "light" or "dark"; anything else = "system" (follows the Mac).
    var appearance = "system"

    /// Reads `{"exclude_apps": [...], "voice_locale": "en-GB"}`; a missing or broken file means defaults.
    static func load(from url: URL) -> Config {
        guard let data = try? Data(contentsOf: url),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return Config() }
        let locale = (obj["voice_locale"] as? String)?.trimmingCharacters(in: .whitespaces)
        var remind: String? = "18:00"
        if obj["remind_at"] is NSNull || (obj["remind_at"] as? Bool) == false { remind = nil }
        if let r = obj["remind_at"] as? String { remind = Reminder.parse(r) == nil ? "18:00" : r }
        let keep = (obj["keep_recordings_days"] as? NSNumber)?.intValue ?? 30
        return Config(excludeApps: (obj["exclude_apps"] as? [String]) ?? [],
                      voiceLocale: locale?.isEmpty == false ? locale : nil,
                      askProject: obj["ask_project"] as? Bool ?? false,
                      remindAt: remind, keepDays: max(0, keep), sounds: obj["sounds"] as? Bool ?? true,
                      autoUpdate: obj["auto_update"] as? Bool ?? true, updateFeed: obj["update_feed"] as? String,
                      shareAllScreens: (obj["share_screens"] as? String) == "all",
                      makeCapsules: (obj["make_capsules"] as? String) == "auto" ? "auto" : "click",
                      appearance: ["light", "dark"].contains(obj["appearance"] as? String ?? "") ? obj["appearance"] as! String : "system")
    }
}

enum Exclusion {
    /// Password managers, private messengers, lock screen. Matched as substrings.
    static let builtIn = ["1password", "bitwarden", "keychain", "lastpass", "dashlane", "signal", "whatsapp",
                          "com.apple.passwords", "com.apple.loginwindow", "com.apple.screensaver"]

    /// Built-ins match anywhere in the bundle id or app name; user entries match a bundle id or app name exactly.
    static func isExcluded(bundle: String?, appName: String?, userList: [String]) -> Bool {
        let b = (bundle ?? "").lowercased()
        let n = (appName ?? "").lowercased()
        for word in builtIn where b.contains(word) || n.contains(word) { return true }
        for entry in userList {
            let e = entry.lowercased().trimmingCharacters(in: .whitespaces)
            if !e.isEmpty && (e == b || e == n) { return true }
        }
        return false
    }
}

// MARK: - Events

struct Event: Equatable {
    var t: String
    var end: String
    var app: String
    var bundle: String
    var title: String
    /// Where `text` came from: "ax" (Accessibility) or "ocr". Empty = not written (older files).
    var src: String = ""
    var text: String
    /// "voice" or "note" for what the user said or typed; empty for screen samples (written without it).
    var kind: String = ""
    /// The window's page address or file path, when known (see `Location.clean`).
    var location: String = ""
    /// Project id, and how it was chosen: "auto" (matched a clue) or "you" (picked by the user).
    var project: String = ""
    var projectBy: String = ""
    /// "pointer": read because the mouse was over it on another shared screen (not the window in use).
    /// Empty for the window in use. Pointer samples don't count as time spent.
    var focus: String = ""

    static let voice = "voice"
    static let note = "note"

    /// Spoken or typed by the user (not a screen sample). Never merged with another event.
    var isNote: Bool { kind == Event.voice || kind == Event.note }

    func sameContent(as o: Event) -> Bool {
        !isNote && !o.isNote && kind == o.kind && src == o.src
            && app == o.app && bundle == o.bundle && title == o.title && text == o.text
            && location == o.location && project == o.project && projectBy == o.projectBy && focus == o.focus
    }

    /// One events.jsonl line (no newline), keys in SPEC order.
    /// Key order: t, end, [kind, src,] app, bundle, title, [where], [project, project_by], [focus], [src,] text.
    var line: String {
        var pairs: [(String, JSONValue)] = [("t", .string(t)), ("end", .string(end))]
        if !kind.isEmpty { pairs += [("kind", .string(kind)), ("src", .string(src))] }
        pairs += [("app", .string(app)), ("bundle", .string(bundle)), ("title", .string(title))]
        if !location.isEmpty { pairs.append(("where", .string(location))) }
        if !project.isEmpty { pairs += [("project", .string(project)), ("project_by", .string(projectBy))] }
        if !focus.isEmpty { pairs.append(("focus", .string(focus))) }
        if kind.isEmpty && !src.isEmpty { pairs.append(("src", .string(src))) }
        pairs.append(("text", .string(text)))
        return jsonObject(pairs)
    }

    /// Byte offset of the `end` value (after its opening quote) inside `line`.
    var endValueOffset: Int { ("{\"t\":" + jsonString(t) + ",\"end\":\"").utf8.count }
}

/// Append-only events.jsonl. A repeat of the last sample only moves the last line's `end`.
final class EventLog {
    let url: URL
    private(set) var last: Event?
    /// Lines written by this log (new events, not extensions).
    private(set) var appended = 0
    private var lastLineStart: UInt64 = 0

    init(url: URL) throws {
        self.url = url
        if !FileManager.default.fileExists(atPath: url.path) {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data().write(to: url)
        }
    }

    enum Outcome { case appended, extended }

    @discardableResult
    func record(_ e: Event) throws -> Outcome {
        if var prev = last, prev.sameContent(as: e) {
            prev.end = e.end
            try rewriteLast(prev)
            return .extended
        }
        let h = try FileHandle(forWritingTo: url)
        defer { try? h.close() }
        lastLineStart = try h.seekToEnd()
        try h.write(contentsOf: Data((e.line + "\n").utf8))
        last = e
        appended += 1
        return .appended
    }

    /// Something else happened (excluded app, failed capture): the next sample starts a new event.
    func breakRun() { last = nil }

    /// A sample with nothing new on screen: if the last event is the same window (app, title, place, project),
    /// just move its `end` and return true. Otherwise the caller writes the sample (with empty text) itself.
    func extendIfSameWindow(_ e: Event) throws -> Bool {
        guard var prev = last, !prev.isNote, !e.isNote, prev.app == e.app, prev.bundle == e.bundle, prev.title == e.title,
              prev.location == e.location, prev.project == e.project, prev.focus == e.focus else { return false }
        prev.end = e.end
        try rewriteLast(prev)
        return true
    }

    private func rewriteLast(_ e: Event) throws {
        let h = try FileHandle(forWritingTo: url)
        defer { try? h.close() }
        if let old = last, old.end.utf8.count == e.end.utf8.count {
            // Same length (always, for ISO times): overwrite just the end value in place.
            try h.seek(toOffset: lastLineStart + UInt64(e.endValueOffset))
            try h.write(contentsOf: Data(e.end.utf8))
        } else {
            try h.truncate(atOffset: lastLineStart)
            try h.seek(toOffset: lastLineStart)
            try h.write(contentsOf: Data((e.line + "\n").utf8))
        }
        last = e
    }
}

// MARK: - session.json, state.json, control

func sessionJSON(id: String, startedAt: Date, endedAt: Date?, host: String) -> String {
    jsonObject([("id", .string(id)), ("started_at", .string(Clock.iso(startedAt))),
                ("ended_at", endedAt.map { .string(Clock.iso($0)) } ?? .null),
                ("host", .string(host)), ("version", .int(1))])
}

/// `picking`: the system picker is open, waiting for the user. `lastError`: why the last start did not
/// happen ("cancelled" when the user closed the picker); cleared on the next start.
/// While recording: `events` written so far and `skip`, why the latest sample wrote nothing (nil if it
/// wrote something). After a stop they describe the session that just ended.
func stateJSON(recording: Bool, sessionID: String?, startedAt: Date?, pid: Int32,
               picking: Bool = false, lastError: String? = nil, events: Int = 0, skip: Skip? = nil,
               listening: Bool = false, noteAt: Int64? = nil, project: String? = nil, projectBy: String? = nil) -> String {
    jsonObject([("recording", .bool(recording)),
                ("session_id", sessionID.map { .string($0) } ?? .null),
                ("started_at", startedAt.map { .string(Clock.iso($0)) } ?? .null),
                ("pid", .int(Int(pid))),
                ("picking", .bool(picking)),
                ("last_error", lastError.map { .string($0) } ?? .null),
                ("events", .int(events)),
                ("skip", skip.map { .string($0.rawValue) } ?? .null),
                ("listening", .bool(listening)),
                ("note_at", noteAt.map { .int(Int($0)) } ?? .null),
                ("project", project.map { .string($0) } ?? .null),
                ("project_by", projectBy.map { .string($0) } ?? .null)])
}

struct ControlCommand: Equatable {
    enum Kind: String { case start, stop, note, project }
    var kind: Kind
    var at: Int64
    /// The typed note, for `note`.
    var text: String? = nil
    /// A project id, or "auto", for `project`.
    var project: String? = nil

    /// `{"command":"start"|"stop","at":<ms>}` or `{"command":"note","text":"…","at":<ms>}`;
    /// anything else (a note without text too) is ignored.
    static func parse(_ data: Data) -> ControlCommand? {
        guard let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let name = obj["command"] as? String, let kind = Kind(rawValue: name.lowercased()) else { return nil }
        let at = (obj["at"] as? NSNumber)?.int64Value ?? 0
        var text: String?
        if kind == .note {
            guard let t = (obj["text"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines), !t.isEmpty
            else { return nil }
            text = t
        }
        var project: String?
        if kind == .project {
            guard let p = (obj["project"] as? String)?.trimmingCharacters(in: .whitespaces), !p.isEmpty else { return nil }
            project = p
        }
        return ControlCommand(kind: kind, at: at, text: text, project: project)
    }
}

// MARK: - Picked content (what the user chose in the system picker)

enum PickStyle { case display, window, application, unknown }

/// What the user shared. `rect` is its place on screen in global points (top-left origin).
/// `windowIDs` / `pids` are only known on macOS 15.2+ (SCContentFilter.includedWindows/Applications).
struct PickedContent: Equatable {
    var style: PickStyle
    var rect: CGRect
    var windowIDs: [UInt32]? = nil
    var pids: [Int32]? = nil
    /// Which shared screen/window this is, when several are shared (one stream each).
    var source = 0
}

extension Membership {
    /// The shared content that holds the window in front, when several screens or windows are shared. The
    /// window with keyboard focus decides — not the mouse: people type on one screen with the pointer on another.
    static func pick(for w: WindowInfo, in picks: [PickedContent]) -> PickedContent? {
        picks.first { contains($0, w) }
    }
}

/// The frontmost window: CGWindowList id, owner pid, frame in global points (top-left origin).
struct WindowInfo: Equatable {
    var id: UInt32
    var pid: Int32
    var frame: CGRect
}

/// One on-screen window from CGWindowList (front to back). `excluded`: its app is never recorded.
/// `name` is only there when macOS gives it out (it never asks for permission for it).
struct ListedWindow: Equatable {
    var id: UInt32
    var pid: Int32
    var owner: String = ""
    var layer: Int = 0
    var alpha: Double = 1
    var frame: CGRect
    var name: String? = nil
    var excluded = false

    var info: WindowInfo { WindowInfo(id: id, pid: pid, frame: frame) }
}

/// Why a sample wrote nothing. Shown in the menu and state.json so the user knows what is going on.
enum Skip: String {
    case privateApp = "private_app"           // the front app is never recorded
    case noWindow = "no_window"               // the front app has no normal window on screen
    case notPicked = "not_picked"             // the front window is not in what the user picked
    case overlay = "overlay"                  // Mission Control / Exposé is showing every window
    case privateWindow = "private_window"     // a never-record window covers most of the front window
    case waitingFrame = "waiting_frame"       // no frame yet since the last app switch
    case unknownDisplay = "unknown_display"   // cannot tell where the picked display is

    /// Plain words for the menu.
    var message: String {
        switch self {
        case .privateApp: return "a private app is in front (not recorded)"
        case .noWindow: return "the front app has no window"
        case .notPicked: return "the front window is not in what you picked"
        case .overlay: return "Mission Control is open"
        case .privateWindow: return "a private app covers the window"
        case .waitingFrame: return "waiting for the screen to update"
        case .unknownDisplay: return "cannot find the picked screen"
        }
    }
}

enum WindowList {
    /// Smaller than this (points) is a strip, badge or helper window, not a real window.
    static let minSide: CGFloat = 40

    static func isNormal(_ w: ListedWindow) -> Bool {
        w.layer == 0 && w.alpha > 0.01 && w.frame.width >= minSide && w.frame.height >= minSide
    }

    /// The app's front window. Invisible (alpha 0) and tiny windows are skipped, and a strip that sits
    /// inside a bigger window of the same app (Chrome, Zen and Electron tab bars) gives way to that window.
    static func front(pid: Int32, in list: [ListedWindow]) -> ListedWindow? {
        let mine = list.filter { $0.pid == pid && isNormal($0) }
        guard let first = mine.first else { return nil }
        let t = Membership.tolerance
        let holder = mine.dropFirst().first {
            $0.frame.insetBy(dx: -t, dy: -t).contains(first.frame)
                && $0.frame.width * $0.frame.height > first.frame.width * first.frame.height
        }
        return holder ?? first
    }

    /// Never to be read: windows of never-record apps, and notification banners (they show message
    /// text from any app, WhatsApp and Signal too).
    static func isPrivate(_ w: ListedWindow) -> Bool {
        w.excluded || (w.owner == "Notification Center" && w.layer > 0)
    }

    /// macOS's own menus and bars: never "the app you work in". When one is frontmost (you clicked the
    /// menu bar, Control Center, Spotlight…), the work window is the top normal window of another app.
    static let systemUI: Set<String> = [
        "com.apple.controlcenter", "com.apple.systemuiserver", "com.apple.dock", "com.apple.notificationcenterui",
        "com.apple.Spotlight", "com.apple.WindowManager", "com.apple.TextInputMenuAgent", "com.apple.TextInputSwitcher",
        "com.apple.wifi.WiFiAgent", "com.apple.AirPlayUIAgent", "com.apple.UserNotificationCenter",
    ]

    static func isSystemUI(_ bundle: String?) -> Bool { bundle.map { systemUI.contains($0) } ?? false }

    /// The pid that owns the top normal window, skipping system UI processes (`isSystem`). A private app's
    /// window is NOT skipped: if it's on top, it's what the user is looking at, and the sample is skipped.
    static func topRegularPid(in list: [ListedWindow], isSystem: (Int32) -> Bool) -> Int32? {
        list.first { isNormal($0) && !isSystem($0.pid) }?.pid
    }

    /// Mission Control / Exposé: the Dock puts full-screen windows below its usual layer (20) and shows
    /// every window, private ones too. Nothing may be read then.
    static func overlayActive(_ list: [ListedWindow]) -> Bool {
        list.contains { $0.owner == "Dock" && $0.layer > 0 && $0.layer < 20 && $0.alpha > 0.01 }
    }
}

/// Frames come from the stream only when the screen changes, so right after an app switch the latest
/// frame can still show the previous app. Only use a frame that arrived after the last switch.
enum Freshness {
    static let settle: TimeInterval = 0.1

    static func ok(frameAt: Date?, changedAt: Date?) -> Bool {
        guard let frameAt else { return false }
        guard let changedAt else { return true }
        return frameAt >= changedAt.addingTimeInterval(settle)
    }
}

/// What to OCR: `region` and `masks` are normalized to the captured image, origin bottom left (Vision).
/// Masks are painted black before OCR.
enum OCRDecision: Equatable {
    case read(region: CGRect, masks: [CGRect])
    case skip(Skip)
}

enum Membership {
    /// Points of slack when matching a window frame to the picked rect (shadows, rounding).
    static let tolerance: CGFloat = 8

    /// Is the front window part of what the user picked? If not, the sample writes nothing.
    static func contains(_ pick: PickedContent, _ w: WindowInfo) -> Bool {
        switch pick.style {
        case .window:
            if let ids = pick.windowIDs { return ids.contains(w.id) }
            return sameFrame(pick.rect, w.frame) // macOS 14 – 15.1: match by place and size
        case .application:
            if let pids = pick.pids { return pids.contains(w.pid) }
            return false
        case .display:
            guard !pick.rect.isEmpty, !w.frame.isEmpty else { return false }
            return pick.rect.contains(CGPoint(x: w.frame.midX, y: w.frame.midY))
        case .unknown:
            return false
        }
    }

    static func sameFrame(_ a: CGRect, _ b: CGRect) -> Bool {
        guard !a.isEmpty, !b.isEmpty else { return false }
        return abs(a.minX - b.minX) <= tolerance && abs(a.minY - b.minY) <= tolerance
            && abs(a.width - b.width) <= tolerance && abs(a.height - b.height) <= tolerance
    }

    /// The part of the captured image to OCR, normalized with the origin at the bottom left (Vision's
    /// `regionOfInterest`). A picked window is read whole; a display or app capture is cropped to the
    /// front window. nil when the window is not on the captured content at all.
    static func ocrRegion(_ pick: PickedContent, window: CGRect) -> CGRect? {
        if pick.style == .window { return CGRect(x: 0, y: 0, width: 1, height: 1) }
        let c = pick.rect
        guard !c.isEmpty else { return nil }
        let i = c.intersection(window)
        guard !i.isNull, i.width >= 1, i.height >= 1 else { return nil }
        return normalized(i, in: c)
    }

    /// `r` (global points, top-left origin) as a part of `c`, normalized with the origin at the bottom left.
    static func normalized(_ r: CGRect, in c: CGRect) -> CGRect {
        let full = CGRect(x: 0, y: 0, width: 1, height: 1)
        let n = CGRect(x: (r.minX - c.minX) / c.width, y: (c.maxY - r.maxY) / c.height,
                       width: r.width / c.width, height: r.height / c.height)
        return n.intersection(full)
    }

    /// More than this share of the window hidden by private windows: skip the sample.
    static let maxMasked: CGFloat = 0.5

    /// What to OCR for this sample. For a picked display: crop to the front window, and black out any
    /// never-record window above it (floating or overlapping); skip if they hide most of it.
    /// A picked window or app is captured on its own (other apps are not in the image), so no masks.
    /// `list` is the whole on-screen window list, front to back; `front` is one of its entries.
    static func ocrDecision(_ pick: PickedContent, front: ListedWindow, list: [ListedWindow]) -> OCRDecision {
        if pick.style == .display && pick.rect.isEmpty { return .skip(.unknownDisplay) }
        guard let region = ocrRegion(pick, window: front.frame) else { return .skip(.notPicked) }
        guard pick.style == .display else { return .read(region: region, masks: []) }

        let crop = pick.rect.intersection(front.frame)
        let above = list.prefix { $0.id != front.id } // CGWindowList is ordered front to back
        var masks: [CGRect] = []
        var hidden: CGFloat = 0
        for w in above where WindowList.isPrivate(w) && w.alpha > 0.01 {
            let i = w.frame.intersection(crop)
            guard !i.isNull, i.width >= 1, i.height >= 1 else { continue }
            masks.append(normalized(i, in: pick.rect))
            hidden += i.width * i.height
        }
        if hidden > maxMasked * crop.width * crop.height { return .skip(.privateWindow) }
        return .read(region: region, masks: masks)
    }

    /// The picked display's place in global points. `rect` may be the display's own (0,0)-based size on
    /// older macOS; then match it by size against the real displays. nil when it cannot be told apart.
    static func resolveDisplay(_ rect: CGRect, displays: [CGRect]) -> CGRect? {
        guard !rect.isEmpty else { return nil }
        if let exact = displays.first(where: { sameFrame($0, rect) }) { return exact }
        let sized = displays.filter {
            abs($0.width - rect.width) <= tolerance && abs($0.height - rect.height) <= tolerance
        }
        return sized.count == 1 ? sized[0] : nil
    }
}

// MARK: - Accessibility text

/// One element of a window's accessibility tree, in document order (depth-first, parents first).
struct AXNode: Equatable {
    var depth: Int
    var role: String
    var subrole: String = ""
    var value: String = ""
    var title: String = ""
    var desc: String = ""
}

enum TextSource: String { case ax, ocr }

enum AXText {
    /// Below this many letters and digits, AX text is not useful (canvas apps, video): use OCR.
    static let minUseful = 80
    /// Walk limits for one sample.
    /// The walk stops at whichever comes first: this many elements, or the time budget (keeps the Mac smooth).
    static let maxNodes = 20_000
    static let budget: TimeInterval = 0.4
    static let maxDepth = 60
    /// Stop collecting past this (the event cap is 4000 after redaction).
    static let maxChars = 100_000

    static let valueRoles: Set<String> = ["AXStaticText", "AXTextArea", "AXTextField", "AXComboBox"]
    static let labelRoles: Set<String> = ["AXHeading", "AXLink", "AXCell"]

    /// Password fields and anything marked secure: never read, nor anything inside them.
    static func isSecure(role: String, subrole: String) -> Bool {
        role == "AXSecureTextField" || subrole == "AXSecureTextField"
            || role.lowercased().contains("secure") || subrole.lowercased().contains("secure")
    }

    /// The text one node contributes, or "" if its role carries none.
    static func text(of n: AXNode) -> String {
        if isSecure(role: n.role, subrole: n.subrole) { return "" }
        if valueRoles.contains(n.role) { return [n.value, n.title, n.desc].first { !blank($0) } ?? "" }
        if labelRoles.contains(n.role) { return [n.title, n.value, n.desc].first { !blank($0) } ?? "" }
        return ""
    }

    /// Lines in document order; secure subtrees skipped; repeated lines kept once.
    static func assemble(_ nodes: [AXNode], maxChars: Int = maxChars) -> String {
        var out: [String] = []
        var seen = Set<String>()
        var total = 0
        var secureDepth: Int?
        for n in nodes {
            if let d = secureDepth {
                if n.depth > d { continue }
                secureDepth = nil
            }
            if isSecure(role: n.role, subrole: n.subrole) { secureDepth = n.depth; continue }
            for raw in text(of: n).split(whereSeparator: \.isNewline) {
                let line = raw.split(whereSeparator: { $0 == " " || $0 == "\t" }).joined(separator: " ")
                guard !line.isEmpty, !seen.contains(line) else { continue }
                seen.insert(line)
                out.append(line)
                total += line.count + 1
                if total >= maxChars { return out.joined(separator: "\n") }
            }
        }
        return out.joined(separator: "\n")
    }

    /// Letters and digits: what makes text worth keeping.
    static func usefulCount(_ s: String) -> Int { s.reduce(0) { $0 + ($1.isLetter || $1.isNumber ? 1 : 0) } }

    /// AX when it is allowed and gave enough text; otherwise OCR of the picked content.
    static func choose(trusted: Bool, axText: String) -> TextSource {
        trusted && usefulCount(axText) >= minUseful ? .ax : .ocr
    }

    private static func blank(_ s: String) -> Bool { s.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
}

// MARK: - Voice notes

/// One finished recording from the mic: the on-device transcript and how loud the clip got.
/// The audio itself is never kept.
struct VoiceClip: Equatable {
    var text: String
    /// Loudest sample, 0…1 (absolute value).
    var peak: Float
    var seconds: Double
}

/// How a voice note ended. Every case turns the mic indicator off.
enum VoiceOutcome: Equatable {
    case saved(chars: Int, seconds: Double)
    case silent(device: String?)   // nothing heard (silent mic, too short, or no words)
    case notRecording
    case denied(VoicePermission)
    case unavailable(locale: String) // no on-device recognition for this language
    case failed(String)
    case discarded                   // recording stopped and there was nowhere to write it

    /// What the user sees (a short toast).
    var message: String {
        switch self {
        case .saved: return "Voice note saved"
        case .silent(let device):
            let mic = device.map { "“\($0)”" } ?? "the microphone"
            return "Didn't catch that. Nothing was heard from \(mic). Check System Settings → Sound → Input."
        case .notRecording: return "Start contextualizing first (\(Shortcut.current.record.symbols))"
        case .denied(.microphone):
            return "Context Capsule can't use the microphone. Turn it on in System Settings → Privacy & Security → Microphone."
        case .denied(.speech):
            return "Context Capsule can't use speech recognition. Turn it on in System Settings → Privacy & Security → Speech Recognition."
        case .unavailable(let locale):
            return "On-device speech isn't available for \(locale), so voice notes are off (nothing is sent to Apple). "
                + "Set \"voice_locale\" in ~/.context-capsule/config.json, or add the language in System Settings → Keyboard → Dictation."
        case .failed(let why): return "Voice note failed: \(why)"
        case .discarded: return "Voice note not saved: contextualizing had stopped"
        }
    }

    /// One log line: counts and durations only, never the words.
    var logSummary: String {
        switch self {
        case .saved(let chars, let seconds): return "voice note saved: \(chars) chars, \(String(format: "%.1f", seconds)) s"
        case .silent: return "voice note: nothing heard"
        case .notRecording: return "voice note: not recording"
        case .denied(let p): return "voice note: \(p.rawValue) permission denied"
        case .unavailable: return "voice note: on-device recognition unavailable"
        case .failed(let why): return "voice note: failed (\(why))" // system error text, never the user's words
        case .discarded: return "voice note: discarded"
        }
    }
}

enum VoicePermission: String { case microphone, speech }

enum Voice {
    /// Longest note; the mic closes on its own after this.
    static let maxSeconds: TimeInterval = 120
    /// Shorter than this is a double press, not speech.
    static let minSeconds: Double = 0.35
    /// A clip whose loudest sample stays under this is a dead or muted mic.
    static let silentPeak: Float = 0.01

    static func peak(_ samples: UnsafeBufferPointer<Float>) -> Float {
        var p: Float = 0
        for s in samples { p = max(p, abs(s)) }
        return p
    }

    static func peak(_ samples: [Float]) -> Float { samples.withUnsafeBufferPointer(peak) }

    /// Nothing worth writing: silent, too short, or no words came back.
    static func isEmpty(_ clip: VoiceClip) -> Bool {
        clip.peak < silentPeak || clip.seconds < minSeconds
            || clip.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// The events.jsonl line for a note: same redaction and cap as screen text.
    static func event(text: String, kind: String, src: String, start: Date, end: Date,
                      app: String, bundle: String, title: String) -> Event {
        Event(t: Clock.iso(start), end: Clock.iso(end), app: app, bundle: bundle, title: Redactor.clean(title),
              src: src, text: Redactor.clean(text.trimmingCharacters(in: .whitespacesAndNewlines)), kind: kind)
    }
}

/// The mic and the on-device recognizer. The real one is in Voice.swift; tests use a fake.
/// Completions may come on any thread; `VoiceNotes` hops to its own.
protocol VoiceEngine: AnyObject {
    /// Ask for (or check) microphone + speech permission and on-device support. nil = ready.
    func prepare(locale: Locale, _ done: @escaping (VoiceOutcome?) -> Void)
    /// Open the mic and start recognizing. Throws if the mic cannot start.
    func start() throws
    /// Close the mic and hand back the final transcript.
    func finish(_ done: @escaping (Result<VoiceClip, Error>) -> Void)
    /// Close the mic and drop everything.
    func cancel()
    /// Name of the input device, for "nothing heard" messages.
    var deviceName: String? { get }
}

/// Where a note goes and what it is about, captured when the user starts talking.
struct VoiceTarget {
    var app: String
    var bundle: String
    var title: String
    /// Writes the finished event into the session (the recorder's work queue).
    var write: (Event) -> Void
}

/// Press to talk, press again to stop. Owns the mic indicator and guarantees it turns off on every
/// way out: saved, nothing heard, denied, unavailable, error, timeout, recording stopped.
/// Main thread only.
final class VoiceNotes {
    enum Phase: Equatable { case idle, preparing, listening, finishing }

    private(set) var phase: Phase = .idle
    /// true while the mic is open. Called on every change, and always with false at the end.
    var onIndicator: (Bool) -> Void = { _ in }
    var onOutcome: (VoiceOutcome) -> Void = { _ in }
    /// Runs a block after a delay; returns a cancel. Injected so tests control time.
    var schedule: (TimeInterval, @escaping () -> Void) -> () -> Void = { delay, block in
        let item = DispatchWorkItem(block: block)
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: item)
        return { item.cancel() }
    }
    /// Hop for engine completions (main queue in the app, inline in tests).
    var hop: (@escaping () -> Void) -> Void = { block in DispatchQueue.main.async(execute: block) }
    var now: () -> Date = Date.init

    private let engine: VoiceEngine
    private var target: VoiceTarget?
    private var startedAt: Date?
    private var cancelTimeout: (() -> Void)?
    private var token = 0 // a newer note makes older callbacks no-ops

    init(engine: VoiceEngine) { self.engine = engine }

    var isActive: Bool { phase != .idle }

    /// The hotkey / menu item. `target` nil = not recording.
    func toggle(locale: Locale, target: VoiceTarget?) {
        switch phase {
        case .idle: begin(locale: locale, target: target)
        case .listening: finish()
        case .preparing, .finishing: break // a press while busy does nothing
        }
    }

    /// Recording is stopping: finish what was said (it still goes into that session).
    func recordingWillStop() {
        switch phase {
        case .listening: finish()
        case .preparing: end(.discarded, cancelEngine: true)
        case .idle, .finishing: break
        }
    }

    private func begin(locale: Locale, target: VoiceTarget?) {
        guard let target else { return report(.notRecording) }
        token += 1
        let mine = token
        phase = .preparing
        self.target = target
        engine.prepare(locale: locale) { [weak self] problem in
            self?.hop {
                guard let self, self.token == mine, self.phase == .preparing else { return }
                if let problem { return self.end(problem, cancelEngine: false) }
                do { try self.engine.start() } catch {
                    return self.end(.failed(error.localizedDescription), cancelEngine: true)
                }
                self.phase = .listening
                self.startedAt = self.now()
                self.onIndicator(true)
                self.cancelTimeout = self.schedule(Voice.maxSeconds) { [weak self] in
                    guard let self, self.token == mine, self.phase == .listening else { return }
                    self.finish()
                }
            }
        }
    }

    private func finish() {
        guard phase == .listening else { return }
        let mine = token
        phase = .finishing
        cancelTimeout?()
        cancelTimeout = nil
        onIndicator(false) // the mic is closing now
        let started = startedAt ?? now()
        let ended = now()
        engine.finish { [weak self] result in
            self?.hop {
                guard let self, self.token == mine, self.phase == .finishing else { return }
                switch result {
                case .failure(let error):
                    self.end(.failed(error.localizedDescription), cancelEngine: false)
                case .success(let clip):
                    if Voice.isEmpty(clip) { return self.end(.silent(device: self.engine.deviceName), cancelEngine: false) }
                    guard let target = self.target else { return self.end(.discarded, cancelEngine: false) }
                    let event = Voice.event(text: clip.text, kind: Event.voice, src: "voice", start: started,
                                            end: ended, app: target.app, bundle: target.bundle, title: target.title)
                    target.write(event)
                    self.end(.saved(chars: event.text.count, seconds: clip.seconds), cancelEngine: false)
                }
            }
        }
    }

    /// The one way out. Always turns the indicator off.
    private func end(_ outcome: VoiceOutcome, cancelEngine: Bool) {
        if cancelEngine { engine.cancel() }
        cancelTimeout?()
        cancelTimeout = nil
        phase = .idle
        target = nil
        startedAt = nil
        onIndicator(false)
        report(outcome)
    }

    private func report(_ outcome: VoiceOutcome) {
        log(outcome.logSummary)
        onOutcome(outcome)
    }

    /// Where log lines go (the app's AppLog; tests can swap it).
    var log: (String) -> Void = { AppLog.shared.write($0) }
}

// MARK: - Diagnostics log

/// `~/.context-capsule/app.log`: one line per notable thing (recording started/stopped and why, voice-note
/// numbers and error codes). Never screen text, titles or spoken words. Kept under `maxBytes`.
final class AppLog {
    static let shared = AppLog(url: Paths.standard.log)
    static let maxBytes = 256 * 1024

    let url: URL
    private let queue = DispatchQueue(label: "com.thekarya.contextcapsule.log")

    init(url: URL) { self.url = url }

    func write(_ message: String, at date: Date = Date()) {
        let line = "\(Clock.iso(date)) \(message)\n"
        queue.async { [url] in AppLog.append(line, to: url) }
        NSLog("Context Capsule: \(message)")
    }

    /// Waits for pending writes (tests, quitting).
    func flush() { queue.sync {} }

    static func append(_ line: String, to url: URL) {
        let fm = FileManager.default
        try? fm.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        if let size = (try? fm.attributesOfItem(atPath: url.path))?[.size] as? Int, size > maxBytes,
           let data = try? Data(contentsOf: url) {
            // Keep the newer half, starting at a line.
            var tail = data.suffix(maxBytes / 2)
            if let nl = tail.firstIndex(of: 0x0A) { tail = tail[(nl + 1)...] }
            try? Data(tail).write(to: url, options: .atomic)
        }
        if !fm.fileExists(atPath: url.path) { fm.createFile(atPath: url.path, contents: nil) }
        guard let h = try? FileHandle(forWritingTo: url) else { return }
        defer { try? h.close() }
        _ = try? h.seekToEnd()
        try? h.write(contentsOf: Data(line.utf8))
    }
}

// MARK: - Projects

/// A project the user works on. `match` are clues found in the window title, its page address / file
/// path, or the app name (never in screen text: other projects' names show up there).
struct Project: Equatable {
    var id: String
    var name: String
    var match: [String] = []
}

enum Projects {
    /// "Client Report (Q3)!" → "client-report-q3"
    static func slug(_ name: String) -> String {
        let folded = name.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: Locale(identifier: "en_US_POSIX"))
        var out = ""
        var dash = false
        for c in folded.unicodeScalars {
            if CharacterSet.alphanumerics.contains(c) && c.isASCII {
                out.unicodeScalars.append(c)
                dash = false
            } else if !dash && !out.isEmpty {
                out += "-"
                dash = true
            }
        }
        while out.hasSuffix("-") { out.removeLast() }
        var cut = String(out.prefix(60))
        while cut.hasSuffix("-") { cut.removeLast() } // same as the MCP: never end on a dash
        return cut
    }

    /// `{"projects":[{"id","name","match":[…]}]}`; a missing or broken file means no projects.
    static func load(from url: URL) -> [Project] {
        guard let data = try? Data(contentsOf: url),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let list = obj["projects"] as? [[String: Any]] else { return [] }
        return list.compactMap { p in
            guard let name = (p["name"] as? String)?.trimmingCharacters(in: .whitespaces), !name.isEmpty else { return nil }
            let id = (p["id"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? slug(name)
            let match = (p["match"] as? [String] ?? []).map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
            return Project(id: id, name: name, match: match)
        }
    }

    static func save(_ projects: [Project], to url: URL) throws {
        let list: [[String: Any]] = projects.map { ["id": $0.id, "name": $0.name, "match": $0.match] }
        let data = try JSONSerialization.data(withJSONObject: ["projects": list], options: [.prettyPrinted, .sortedKeys])
        try writeAtomically(String(decoding: data, as: UTF8.self) + "\n", to: url)
    }

    /// The project whose clue appears in the title, location or app name (case-insensitive). On a tie the
    /// longest clue wins, then list order. nil: no clue found.
    static func detect(_ projects: [Project], title: String, location: String, app: String) -> Project? {
        let hay = [title, location, app].map { $0.lowercased() }
        var best: (project: Project, length: Int)?
        for p in projects {
            for clue in p.match {
                let c = clue.lowercased()
                guard c.count >= 2, hay.contains(where: { $0.contains(c) }) else { continue }
                if best == nil || c.count > best!.length { best = (p, c.count) }
            }
        }
        return best?.project
    }

    /// A clue to remember a window by, when the user makes a project from it: the file or folder name
    /// from its location, else the page's host, else the most specific part of the title.
    static func clue(title: String, location: String) -> String? {
        if let url = URL(string: location), let scheme = url.scheme {
            if scheme == "file" {
                let name = url.deletingPathExtension().lastPathComponent
                if name.count >= 3 { return name }
            } else if let host = url.host, !host.isEmpty {
                let path = url.path.split(separator: "/").prefix(2).joined(separator: "/")
                return path.isEmpty ? host : "\(host)/\(path)"
            }
        }
        // "main.swift — karya-capsule" / "Pricing - Linear": the longest part that isn't the app's own name.
        let parts = title.components(separatedBy: CharacterSet(charactersIn: "—–|·")).map {
            $0.trimmingCharacters(in: .whitespaces)
        }.filter { $0.count >= 3 }
        return parts.max { $0.count < $1.count }
    }
}

/// What the recorder is doing about projects: detect automatically, or stick to the one the user picked.
struct ProjectChoice: Equatable {
    /// nil: auto-detect.
    var pinned: String?

    /// The project id and how it was chosen, for one sample.
    func resolve(_ projects: [Project], title: String, location: String, app: String) -> (id: String, by: String)? {
        if let pinned { return (pinned, "you") }
        return Projects.detect(projects, title: title, location: location, app: app).map { ($0.id, "auto") }
    }
}

/// A window's page address or file path, safe to store: query and fragment dropped (they often carry
/// tokens or private ids), then redacted like any text.
enum Location {
    static func clean(_ raw: String?) -> String {
        guard let raw = raw?.trimmingCharacters(in: .whitespacesAndNewlines), !raw.isEmpty else { return "" }
        var s = raw
        if let cut = s.firstIndex(where: { $0 == "?" || $0 == "#" }) { s = String(s[..<cut]) }
        return Redactor.clean(s, cap: 500)
    }
}

// MARK: - Capsule files (for the "My capsules" window)

/// What the app shows about a saved capsule: from its front matter and its "In short" line.
struct CapsuleSummary: Equatable {
    var url: URL
    var title: String
    var project: String = ""
    /// "daily", "project" (the growing one), or "" (a one-off capsule).
    var kind: String = ""
    var date: String = ""
    var created: String = ""
    var author: String = ""
    var inShort: String = ""
    var draft = false

    /// Reads `---` front matter (simple `key: value` lines) and the first lines under `## In short`.
    static func parse(_ text: String, url: URL) -> CapsuleSummary {
        // Saved on Windows (Notepad, the clipboard): "\r\n" line ends. In Swift "\r\n" is one Character, so
        // without this neither hasPrefix("---\n") nor split(separator: "\n") would see the lines.
        let text = text.replacingOccurrences(of: "\r\n", with: "\n")
        var front: [String: String] = [:]
        var body = text
        if text.hasPrefix("---\n"), let end = text.range(of: "\n---", range: text.index(text.startIndex, offsetBy: 4)..<text.endIndex) {
            for line in text[text.index(text.startIndex, offsetBy: 4)..<end.lowerBound].split(separator: "\n") {
                guard let colon = line.firstIndex(of: ":") else { continue }
                let key = line[..<colon].trimmingCharacters(in: .whitespaces)
                var value = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
                if let hash = value.range(of: " #") { value = String(value[..<hash.lowerBound]).trimmingCharacters(in: .whitespaces) }
                if value.count >= 2, let q = value.first, (q == "\"" || q == "'"), value.last == q {
                    value = String(value.dropFirst().dropLast())
                }
                front[key] = value
            }
            body = String(text[end.upperBound...])
        }
        var inShort = ""
        if let start = body.range(of: "## In short", options: .caseInsensitive) {
            let rest = body[start.upperBound...]
            let section = rest.range(of: "\n## ").map { rest[..<$0.lowerBound] } ?? rest
            inShort = section.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty }.joined(separator: " ")
        }
        let fallback = url.deletingPathExtension().deletingPathExtension().lastPathComponent
        return CapsuleSummary(url: url, title: front["title"].flatMap { $0.isEmpty ? nil : $0 } ?? fallback,
                              project: front["project"] ?? "", kind: front["kind"] ?? "", date: front["date"] ?? "",
                              created: front["created"] ?? "", author: front["author"] ?? "", inShort: inShort,
                              draft: front["draft"] == "true")
    }

    /// Every `*.capsule.md` in the folder (not its .history), newest first.
    static func all(in folder: URL) -> [CapsuleSummary] {
        let files = (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
        return files.filter { $0.lastPathComponent.hasSuffix(".capsule.md") }
            .compactMap { url in (try? String(contentsOf: url, encoding: .utf8)).map { parse($0, url: url) } }
            .sorted { ($0.created, $0.title) > ($1.created, $1.title) }
    }
}

/// Opening the Claude app on a new chat. A link with the message pre-filled (`newChat(prefill:)`) makes Claude
/// show a red "use caution" warning, so the app opens a clean chat and puts the request on the clipboard instead.
enum ClaudeLink {
    static let newChat = URL(string: "claude://claude.ai/new")!

    /// Names the app, so Claude uses Context Capsule's tools rather than guessing (Drive, Gmail…).
    static let dayPrompt = "Use Context Capsule to make my capsules for today."

    static func newChat(prefill text: String) -> URL? {
        var parts = URLComponents(string: "claude://claude.ai/new")
        parts?.queryItems = [URLQueryItem(name: "q", value: text)]
        return parts?.url
    }
}

// MARK: - Evening reminder and clean-up

enum Reminder {
    /// "18:00" → minutes after midnight.
    static func parse(_ hhmm: String) -> Int? {
        let p = hhmm.split(separator: ":")
        guard p.count == 2, let h = Int(p[0]), let m = Int(p[1]), (0..<24).contains(h), (0..<60).contains(m) else { return nil }
        return h * 60 + m
    }

    /// Time to ask "Make today's capsules?": past the reminder time, something recorded today, no capsule made
    /// for today yet, not asked today already, and not recording right now.
    static func due(now minutes: Int, at: String?, recordedToday: Bool, capsuleToday: Bool, askedToday: Bool,
                    recording: Bool) -> Bool {
        guard let at, let target = parse(at) else { return false }
        return minutes >= target && recordedToday && !capsuleToday && !askedToday && !recording
    }
}

enum Cleanup {
    /// Session folders (YYYYMMDD-HHMMSS) started before `keepDays` days ago. Capsules are never touched.
    static func oldSessions(_ ids: [String], keepDays: Int, now: Date, timeZone: TimeZone = .current) -> [String] {
        guard keepDays > 0 else { return [] }
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = timeZone
        guard let cutoff = cal.date(byAdding: .day, value: -keepDays, to: cal.startOfDay(for: now)) else { return [] }
        let cut = Clock.sessionID(cutoff, timeZone: timeZone)
        return ids.filter { $0.count == 15 && $0 < cut }
    }
}

// MARK: - Automatic capsules (SPEC "Automatic capsules"): when to start a run, and auto.json

enum AutoRun {
    /// The time when Make capsules is Automatically: remind_at, or 18:00 when it's missing, off or not a time.
    static let defaultAt = "18:00"
    /// A "running" entry older than this was cut off (the computer slept, the app quit…).
    static let staleSeconds: TimeInterval = 15 * 60

    static func atMinutes(_ at: String?) -> Int {
        at.flatMap(Reminder.parse) ?? Reminder.parse(defaultAt)!
    }

    /// Start an automatic run now? Make capsules is Automatically, a tool is installed, it's past the time today,
    /// no automatic run started today, no run for today started at or after the time (a click run counts),
    /// something was recorded today, not recording, no run going. Days are "yyyy-MM-dd" (local).
    static func due(nowMinutes: Int, today: String, mode: String, at: String?, hasAgent: Bool, autoDay: String?,
                    lastDay: String?, lastStartMinutes: Int?, recordedToday: Bool, recording: Bool, running: Bool) -> Bool {
        let target = atMinutes(at)
        guard mode == "auto", hasAgent, nowMinutes >= target, autoDay != today else { return false }
        if lastDay == today, let started = lastStartMinutes, started >= target { return false }
        return recordedToday && !recording && !running
    }

    /// auto.json, as `make` writes it.
    struct State {
        var autoDay: String?
        /// `last` as it is in the file, without `pid` (the page gets it as it is).
        var last: [String: Any]?
        var lastDay: String? { last?["day"] as? String }
        var lastStatus: String? { last?["status"] as? String }
        var lastBy: String? { last?["by"] as? String }
        var startedAt: Date? { (last?["started_at"] as? String).flatMap { ISO8601DateFormatter().date(from: $0) } }
        var pid: Int32?

        /// Local minutes after midnight when the last run started.
        func lastStartMinutes(timeZone: TimeZone = .current) -> Int? {
            guard let d = startedAt else { return nil }
            var cal = Calendar(identifier: .gregorian)
            cal.timeZone = timeZone
            return cal.component(.hour, from: d) * 60 + cal.component(.minute, from: d)
        }

        /// A run is going: status running, started under 15 minutes ago, and its process is still there.
        func running(now: Date = Date(), alive: (Int32) -> Bool) -> Bool {
            guard lastStatus == "running", let started = startedAt, now.timeIntervalSince(started) < AutoRun.staleSeconds,
                  let pid, pid > 0 else { return false }
            return alive(pid)
        }
    }

    /// Reads auto.json (`{"auto_day", "last": {day, by, status, ai, started_at, finished_at, capsules, reason, pid}}`);
    /// missing or broken = nothing yet.
    static func parse(_ data: Data?) -> State {
        guard let data, let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return State() }
        var s = State()
        s.autoDay = obj["auto_day"] as? String
        if var last = obj["last"] as? [String: Any] {
            s.pid = (last["pid"] as? NSNumber)?.int32Value
            last.removeValue(forKey: "pid")
            s.last = last
        }
        return s
    }

    /// Config make_capsules: "auto" or "click" (missing or anything else = "click").
    static func mode(config url: URL) -> String {
        guard let data = try? Data(contentsOf: url),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return "click" }
        return obj["make_capsules"] as? String == "auto" ? "auto" : "click"
    }

    /// The bridge's auto.status (ui/BRIDGE.md). `agent` is `{"id","name"}` or nil.
    static func status(mode: String, at: String?, agent: [String: String]?, running: Bool, state: State) -> [String: Any] {
        let time = at.flatMap { Reminder.parse($0) == nil ? nil : $0 } ?? defaultAt
        return ["mode": mode == "auto" ? "auto" : "click", "at": time, "agent": agent ?? NSNull(), "running": running,
                "autoDay": state.autoDay ?? NSNull(), "last": state.last ?? NSNull()]
    }

    /// `context-capsule-mcp agents` → the tool a run would use (`use`), nil when none.
    static func agentInUse(_ data: Data) -> [String: String]? {
        guard let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let use = obj["use"] as? [String: Any], let id = use["id"] as? String, let name = use["name"] as? String else { return nil }
        return ["id": id, "name": name]
    }

    /// "3 capsules ready" / "1 capsule ready".
    static func readyTitle(_ n: Int) -> String { n == 1 ? "1 capsule ready" : "\(n) capsules ready" }
}

// MARK: - Only what's new on screen

/// Remembers the lines already saved for each window in a recording, so each sample saves only what's new —
/// no size limit needed, and long documents are kept whole as you scroll through them.
final class NewLines {
    private var seen: [String: Set<String>] = [:]
    private var order: [String] = []
    /// Windows remembered at once; the least recently used is forgotten first.
    static let maxWindows = 64

    /// The lines of `text` not saved before for `window`, in their order (blank lines dropped, repeats once).
    func filter(_ text: String, window: String) -> String {
        var known = seen[window] ?? []
        var out: [String] = []
        for raw in text.split(whereSeparator: \.isNewline) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            guard !line.isEmpty, known.insert(line).inserted else { continue }
            out.append(line)
        }
        seen[window] = known
        order.removeAll { $0 == window }
        order.append(window)
        if order.count > Self.maxWindows { seen[order.removeFirst()] = nil }
        return out.joined(separator: "\n")
    }

    func reset() {
        seen = [:]
        order = []
    }

    static func key(app: String, bundle: String, title: String, location: String) -> String {
        [bundle.isEmpty ? app : bundle, title, location].joined(separator: "\u{1F}")
    }
}


// MARK: - Changing settings (config.json) from the app

/// Writes one setting into config.json, keeping every other key (and any the app doesn't know) as it was.
enum ConfigFile {
    static func set(_ key: String, _ value: Any?, in url: URL) throws {
        var obj: [String: Any] = [:]
        if let data = try? Data(contentsOf: url), let read = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            obj = read
        }
        obj[key] = value ?? NSNull()
        try write(obj, to: url)
    }

    /// The shared window's config.set (ui/BRIDGE.md): JSON null removes the key; anything else is stored as it is.
    static func merge(_ key: String, _ value: Any, in url: URL) throws {
        var obj: [String: Any] = [:]
        if let data = try? Data(contentsOf: url), let read = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            obj = read
        }
        if value is NSNull { obj.removeValue(forKey: key) } else { obj[key] = value }
        try write(obj, to: url)
    }

    private static func write(_ obj: [String: Any], to url: URL) throws {
        let data = try JSONSerialization.data(withJSONObject: obj, options: [.prettyPrinted, .sortedKeys])
        try writeAtomically(String(decoding: data, as: UTF8.self) + "\n", to: url)
    }
}

// MARK: - The shared window's bridge (ui/BRIDGE.md): what the page may read, and how its files are served

enum BridgeFiles {
    /// A path the page may read: relative to the data folder and in the allow-list (config/projects/state.json,
    /// sessions/<id>/events.jsonl|gaps.jsonl|assign.json|session.json, capsules/<name>.md). No absolute paths, "..",
    /// hidden names, empty parts or backslashes.
    static func allowed(_ p: String) -> Bool {
        guard !p.isEmpty, !p.hasPrefix("/"), !p.contains("\\"), !p.contains("\0") else { return false }
        let parts = p.split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        if parts.contains(where: { $0.isEmpty || $0.hasPrefix(".") }) { return false }
        switch parts.count {
        case 1: return ["config.json", "projects.json", "state.json"].contains(parts[0])
        case 2: return parts[0] == "capsules" && parts[1].hasSuffix(".md")
        case 3: return parts[0] == "sessions" && ["events.jsonl", "gaps.jsonl", "assign.json", "session.json"].contains(parts[2])
        default: return false
        }
    }

    /// The file for an allowed path, if it stays inside the data folder once symlinks are followed.
    static func url(_ p: String, root: URL) -> URL? {
        guard allowed(p) else { return nil }
        let url = root.appendingPathComponent(p)
        let base = root.resolvingSymlinksInPath().standardizedFileURL.path
        let real = url.resolvingSymlinksInPath().standardizedFileURL.path
        return real.hasPrefix(base.hasSuffix("/") ? base : base + "/") ? url : nil
    }

    /// data.list: the folders in sessions/ or the files in capsules/ (hidden ones and .history left out), sorted.
    static func list(_ dir: String, root: URL) -> [String]? {
        guard dir == "sessions" || dir == "capsules" else { return nil }
        let folder = root.appendingPathComponent(dir, isDirectory: true)
        let fm = FileManager.default
        return ((try? fm.contentsOfDirectory(atPath: folder.path)) ?? []).filter { n in
            var isDir: ObjCBool = false
            guard !n.hasPrefix("."), fm.fileExists(atPath: folder.appendingPathComponent(n).path, isDirectory: &isDir) else { return false }
            return dir == "sessions" ? isDir.boolValue : !isDir.boolValue
        }.sorted()
    }

    /// events.jsonl without, for each (t, line), the first line that is exactly `line` with time `t` (CRLF files too).
    /// All or none: nil when any of them isn't there.
    static func removingLines(_ content: Data, _ lines: [(t: String, line: String)]) -> Data? {
        var out = content
        for l in lines {
            let raw = Data(l.line.utf8)
            guard let next = Timeline.removingLine(out, t: l.t, line: raw)
                    ?? Timeline.removingLine(out, t: l.t, line: raw + Data("\r".utf8)) else { return nil }
            out = next
        }
        return out
    }

    /// timeline.delete: the whole recording (lines nil) or those lines of it, in one rewrite. Not while it records.
    static func delete(session: String, lines: [(t: String, line: String)]?, sessions: URL, state: URL) throws {
        guard Timeline.isSessionID(session) else { throw Timeline.DeleteError.badName }
        guard let lines else { try Timeline.deleteSession(session, sessions: sessions, state: state); return }
        if Timeline.recordingSession(state: state) == session { throw Timeline.DeleteError.recording }
        let url = sessions.appendingPathComponent(session).appendingPathComponent("events.jsonl")
        guard let data = try? Data(contentsOf: url), let out = removingLines(data, lines) else { throw Timeline.DeleteError.notFound }
        try out.write(to: url, options: .atomic)
    }

    /// The page's own file for a capsule://app/<path> address: inside `root`, no hidden parts, a file. "/" = index.html.
    static func uiFile(root: URL, path: String) -> URL? {
        let clean = (path.removingPercentEncoding ?? path).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let rel = clean.isEmpty ? "index.html" : clean
        let parts = rel.split(separator: "/", omittingEmptySubsequences: false)
        guard !rel.contains("\\"), !parts.contains(where: { $0.isEmpty || $0.hasPrefix(".") }) else { return nil }
        let url = root.appendingPathComponent(rel)
        let base = root.resolvingSymlinksInPath().standardizedFileURL.path
        let real = url.resolvingSymlinksInPath().standardizedFileURL.path
        var isDir: ObjCBool = false
        guard real.hasPrefix(base + "/"), FileManager.default.fileExists(atPath: real, isDirectory: &isDir), !isDir.boolValue else { return nil }
        return url
    }

    static func mime(_ ext: String) -> String {
        switch ext.lowercased() {
        case "html": return "text/html; charset=utf-8"
        case "css": return "text/css; charset=utf-8"
        case "js", "mjs": return "text/javascript; charset=utf-8"
        case "json": return "application/json; charset=utf-8"
        case "svg": return "image/svg+xml"
        case "png": return "image/png"
        case "woff2": return "font/woff2"
        case "md": return "text/markdown; charset=utf-8"
        default: return "application/octet-stream"
        }
    }
}

// MARK: - Reading where you look (the mouse) as well as where you type

enum PointerRule {
    /// How long after the mouse last moved or scrolled its screen still counts as "being looked at".
    static let window: TimeInterval = 30

    /// The top normal window under a point (global, top-left origin), private ones included (they're skipped later).
    static func window(at point: CGPoint, in list: [ListedWindow]) -> ListedWindow? {
        list.first { WindowList.isNormal($0) && $0.frame.contains(point) }
    }

    /// Read the window under the mouse too? Only when it's on a shared screen other than the one in use (or the
    /// window in use isn't on a shared screen), it isn't the window in use, and the mouse moved or scrolled
    /// there recently — a mouse just parked on another screen doesn't keep it being read.
    static func shouldRead(pointer: ListedWindow?, pointerPick: PickedContent?, front: ListedWindow?,
                           frontPick: PickedContent?, lastActivity: Date?, now: Date) -> Bool {
        guard let pointer, let pointerPick, let lastActivity, now.timeIntervalSince(lastActivity) <= window else { return false }
        if let front, front.id == pointer.id { return false }
        guard let frontPick else { return true }
        return pointerPick.source != frontPick.source
    }
}

// MARK: - Time per project (for Home: Today and This week)

enum WorkStats {
    struct Totals: Equatable {
        var seconds: Double = 0
        /// Project id ("" = not sorted yet) → seconds, most first.
        var byProject: [(id: String, seconds: Double)] = []
        static func == (a: Totals, b: Totals) -> Bool {
            a.seconds == b.seconds && a.byProject.map(\.id) == b.byProject.map(\.id) && a.byProject.map(\.seconds) == b.byProject.map(\.seconds)
        }
    }

    /// Time spent in windows in use between two moments, from the recordings (notes and windows only looked at
    /// on another screen don't count; each moment counts at least one 3-second sample, like the Claude side).
    static func totals(sessions: URL, from: Date, to: Date, projects: [Project] = []) -> Totals {
        let iso = ISO8601DateFormatter()
        let earliest = Clock.sessionID(from.addingTimeInterval(-86_400)) // a recording can start the day before
        let names = (try? FileManager.default.contentsOfDirectory(atPath: sessions.path)) ?? []
        var by: [String: Double] = [:]
        for id in names where id.count == 15 && id >= earliest {
            guard let text = try? String(contentsOf: sessions.appendingPathComponent(id).appendingPathComponent("events.jsonl"),
                                         encoding: .utf8) else { continue }
            let fixes = Assignments.load(sessions.appendingPathComponent(id).appendingPathComponent("assign.json"))
            for line in text.split(separator: "\n") {
                guard let obj = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any],
                      let t = (obj["t"] as? String).flatMap(iso.date(from:)), t >= from, t < to,
                      obj["kind"] == nil, (obj["focus"] as? String) != "pointer" else { continue }
                let end = (obj["end"] as? String).flatMap(iso.date(from:)) ?? t
                // The same order as the Claude side: a correction, what the recorder chose, then today's clues.
                var project = obj["project"] as? String ?? ""
                if let fixed = Assignments.at(fixes, t) {
                    project = fixed
                } else if project.isEmpty, let p = Projects.detect(projects, title: obj["title"] as? String ?? "",
                                                                   location: obj["where"] as? String ?? "",
                                                                   app: obj["app"] as? String ?? "") {
                    project = p.id
                }
                by[project, default: 0] += max(3, end.timeIntervalSince(t))
            }
        }
        let sorted = by.map { (id: $0.key, seconds: $0.value) }.sorted { $0.seconds > $1.seconds }
        return Totals(seconds: sorted.reduce(0) { $0 + $1.seconds }, byProject: sorted)
    }

    /// Days in a row with a recording, ending today (or yesterday, when nothing is recorded yet today).
    /// Session ids are "yyyyMMdd-HHmmss" in local time.
    static func streak(sessionIDs: [String], today: Date, calendar: Calendar = .current) -> Int {
        let days = Set(sessionIDs.filter { $0.count == 15 }.map { String($0.prefix(8)) })
        func key(_ d: Date) -> String { String(Clock.sessionID(d, timeZone: calendar.timeZone).prefix(8)) }
        var day = calendar.startOfDay(for: today)
        if !days.contains(key(day)) { day = calendar.date(byAdding: .day, value: -1, to: day) ?? day }
        var count = 0
        while days.contains(key(day)) {
            count += 1
            guard let before = calendar.date(byAdding: .day, value: -1, to: day) else { break }
            day = before
        }
        return count
    }

    /// The same words as the Timeline: "1 h 5 min", "12 min", "under 1 min"; nothing at all is "0 min".
    static func duration(_ seconds: Double) -> String {
        seconds < 1 ? "0 min" : Timeline.length(seconds)
    }

    /// "Good morning" / "Good afternoon" / "Good evening".
    static func greeting(hour: Int) -> String {
        hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening"
    }
}

/// Dates and times for people, in the system's language and style (12/24-hour as set in System Settings).
/// The same day style on Home and the Timeline: "Friday 23 October" / "Friday, October 23", with the year
/// only when it isn't this year. File formats (session ids, ISO times, "yyyy-MM-dd" keys) never use these.
enum DayWords {
    static func long(_ date: Date, now: Date = Date(), locale: Locale = .current, calendar: Calendar = .current) -> String {
        let f = DateFormatter()
        f.locale = locale
        f.calendar = calendar
        f.timeZone = calendar.timeZone
        let thisYear = calendar.component(.year, from: date) == calendar.component(.year, from: now)
        f.setLocalizedDateFormatFromTemplate(thisYear ? "EEEEdMMMM" : "EEEEdMMMMy")
        return f.string(from: date)
    }

    /// A clock time: "14:05" or "2:05 PM", as the system says.
    static func time(_ date: Date, locale: Locale = .current, timeZone: TimeZone = .current) -> String {
        let f = DateFormatter()
        f.locale = locale
        f.timeZone = timeZone
        f.dateStyle = .none
        f.timeStyle = .short
        return f.string(from: date)
    }

    /// "yyyy-MM-dd" (a file/data key) read as a local day, whatever the system calendar.
    static func parseKey(_ ymd: String, timeZone: TimeZone = .current) -> Date? {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.calendar = Calendar(identifier: .gregorian)
        f.timeZone = timeZone
        f.dateFormat = "yyyy-MM-dd"
        return f.date(from: ymd)
    }

    /// A date as a "yyyy-MM-dd" key (local day), whatever the system calendar.
    static func key(_ date: Date, timeZone: TimeZone = .current) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.calendar = Calendar(identifier: .gregorian)
        f.timeZone = timeZone
        f.dateFormat = "yyyy-MM-dd"
        return f.string(from: date)
    }
}

/// Corrections Claude made to which project a stretch belongs to (`assign.json` in a recording's folder).
enum Assignments {
    struct Fix { var from: Date?; var to: Date?; var project: String }

    static func load(_ url: URL) -> [Fix] {
        guard let data = try? Data(contentsOf: url), let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let list = obj["assign"] as? [[String: Any]] else { return [] }
        let iso = ISO8601DateFormatter()
        return list.compactMap { a in
            guard let p = a["project"] as? String else { return nil }
            return Fix(from: (a["from"] as? String).flatMap(iso.date(from:)), to: (a["to"] as? String).flatMap(iso.date(from:)),
                       project: p)
        }
    }

    /// The correction covering `t` (later ones win); "" means "no project"; nil: none.
    static func at(_ fixes: [Fix], _ t: Date) -> String? {
        var hit: String?
        for f in fixes where (f.from.map { t >= $0 } ?? true) && (f.to.map { t < $0 } ?? true) { hit = f.project }
        return hit
    }
}

// MARK: - Your own shortcuts

/// A keyboard shortcut as written in config.json: modifiers then one key, joined by "+" ("Alt+C", "Ctrl+Shift+F5").
/// Alt is Option on a Mac. See SPEC.md, "Your own shortcuts".
struct Shortcut: Equatable {
    var ctrl = false
    var alt = false
    var shift = false
    var cmd = false
    /// "A"–"Z", "0"–"9" or "F1"–"F12".
    var key = ""

    static let defaultRecord = Shortcut(alt: true, key: "C")
    static let defaultVoice = Shortcut(alt: true, key: "V")
    static let keys: [String] = (65...90).map { String(UnicodeScalar($0)!) } + (0...9).map(String.init) + (1...12).map { "F\($0)" }

    var isFunctionKey: Bool { key.count > 1 && key.hasPrefix("F") }

    static func parse(_ text: String) -> Shortcut? {
        var s = Shortcut()
        let parts = text.split(separator: "+").map { $0.trimmingCharacters(in: .whitespaces) }
        guard let last = parts.last?.uppercased(), keys.contains(last) else { return nil }
        s.key = last
        for p in parts.dropLast() {
            switch p.lowercased() {
            case "ctrl", "control": s.ctrl = true
            case "alt", "option", "opt": s.alt = true
            case "shift": s.shift = true
            case "cmd", "command": s.cmd = true
            default: return nil
            }
        }
        return s
    }

    /// For config.json and Windows / Linux: "Ctrl+Alt+Shift+Cmd+K" in that order.
    var text: String {
        ([ctrl ? "Ctrl" : nil, alt ? "Alt" : nil, shift ? "Shift" : nil, cmd ? "Cmd" : nil].compactMap { $0 } + [key])
            .joined(separator: "+")
    }

    /// As the Mac writes it: "⌃⌥⇧⌘K".
    var symbols: String { (ctrl ? "⌃" : "") + (alt ? "⌥" : "") + (shift ? "⇧" : "") + (cmd ? "⌘" : "") + key }

    enum Problem: Equatable {
        case needsModifier      // "Add Ctrl, ⌥ or ⌘"
        case sameAsOther        // "Already used for …"
    }

    /// Can this be used, next to the other shortcut? (Whether another app has it is found when registering.)
    func problem(other: Shortcut) -> Problem? {
        if !(ctrl || alt || cmd) && !isFunctionKey { return .needsModifier }
        if self == other { return .sameAsOther }
        return nil
    }

    /// config.json "hotkeys": {"record": "Alt+C", "voice": "Alt+V"}; anything missing or broken = the default.
    static func load(from url: URL) -> (record: Shortcut, voice: Shortcut) {
        guard let data = try? Data(contentsOf: url),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let keys = obj["hotkeys"] as? [String: Any] else { return (defaultRecord, defaultVoice) }
        var record = (keys["record"] as? String).flatMap(parse) ?? defaultRecord
        var voice = (keys["voice"] as? String).flatMap(parse) ?? defaultVoice
        if record.problem(other: voice) != nil { record = defaultRecord }
        if voice.problem(other: record) != nil { voice = record == defaultVoice ? defaultRecord : defaultVoice }
        return (record, voice)
    }

    /// The shortcuts in use now, for every text that names them.
    static var current = (record: defaultRecord, voice: defaultVoice)
}

// MARK: - Any AI (SPEC "Any AI"): the context-capsule-mcp program's answers, and which AI is yours

/// One AI app from `context-capsule-mcp clients`.
struct AIClient: Equatable {
    var id: String
    var name: String
    var installed: Bool
    var connected: Bool
    /// "extension" (Claude: the .mcpb) or "config" (Context Capsule is written into the app's MCP settings).
    var how: String
    /// Optional extras, if the program gives them: the Mac app's bundle id or path, to open it.
    var bundle: String? = nil
    var app: String? = nil
}

/// Reading what `context-capsule-mcp` prints (JSON on stdout). Unknown keys are ignored; anything broken is an error.
enum AIAnswer {
    struct Connect: Equatable { var ok: Bool; var message: String; var how: String? }
    struct Day: Equatable { var text: String; var chars: Int }
    struct Saved: Equatable { var title: String; var path: String }
    struct Refused: Equatable { var title: String; var why: String }
    struct Save: Equatable { var saved: [Saved]; var refused: [Refused] }

    enum Problem: Error, Equatable { case notJSON, missing(String) }

    /// The JSON value in the output. Tolerates lines before it: takes from the first line starting with { or [.
    static func json(_ data: Data) throws -> Any {
        if let v = try? JSONSerialization.jsonObject(with: data) { return v }
        let text = String(decoding: data, as: UTF8.self)
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        if let i = lines.firstIndex(where: { $0.hasPrefix("{") || $0.hasPrefix("[") }),
           let v = try? JSONSerialization.jsonObject(with: Data(lines[i...].joined(separator: "\n").utf8)) { return v }
        throw Problem.notJSON
    }

    static func clients(_ data: Data) throws -> [AIClient] {
        let v = try json(data)
        // `[...]`, or `{"clients": [...]}` just in case.
        guard let list = (v as? [Any]) ?? ((v as? [String: Any])?["clients"] as? [Any]) else { throw Problem.missing("clients") }
        return list.compactMap { item in
            guard let o = item as? [String: Any], let id = o["id"] as? String, !id.isEmpty else { return nil }
            return AIClient(id: id, name: (o["name"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? id,
                            installed: o["installed"] as? Bool ?? false, connected: o["connected"] as? Bool ?? false,
                            how: o["how"] as? String ?? "config",
                            bundle: o["bundle"] as? String ?? o["bundle_id"] as? String, app: o["app"] as? String)
        }
    }

    static func connect(_ data: Data) throws -> Connect {
        guard let o = try json(data) as? [String: Any], let ok = o["ok"] as? Bool else { throw Problem.missing("ok") }
        return Connect(ok: ok, message: o["message"] as? String ?? "", how: o["how"] as? String)
    }

    static func day(_ data: Data) throws -> Day {
        guard let o = try json(data) as? [String: Any], let text = o["text"] as? String else { throw Problem.missing("text") }
        return Day(text: text, chars: (o["chars"] as? NSNumber)?.intValue ?? text.count)
    }

    static func save(_ data: Data) throws -> Save {
        guard let o = try json(data) as? [String: Any], o["saved"] != nil || o["refused"] != nil else { throw Problem.missing("saved") }
        let saved = (o["saved"] as? [[String: Any]] ?? []).map {
            Saved(title: ($0["title"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "Untitled", path: $0["path"] as? String ?? "")
        }
        let refused = (o["refused"] as? [[String: Any]] ?? []).map {
            Refused(title: ($0["title"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "Untitled",
                    why: $0["why"] as? String ?? "")
        }
        return Save(saved: saved, refused: refused)
    }

    /// Settings → Connect your AI: installed apps first (Claude first among them), then by name.
    static func sorted(_ list: [AIClient]) -> [AIClient] {
        list.sorted { a, b in
            if a.installed != b.installed { return a.installed }
            if (a.id == "claude") != (b.id == "claude") { return a.id == "claude" }
            return a.name.localizedCaseInsensitiveCompare(b.name) == .orderedAscending
        }
    }
}

/// "Your AI" (config.json "ai"): an id from `clients`, a known website, or "any" for any other AI.
enum YourAI: Equatable {
    case claude
    case app(String)          // another app connected through MCP (Cursor, VS Code…)
    case website(String, URL) // ChatGPT or Gemini on the web: Copy my day, then their site opens
    case other                // any other AI: Copy my day

    static let anyID = "any"
    static let websites: [(id: String, name: String, url: URL)] = [
        ("chatgpt-web", "ChatGPT (website)", URL(string: "https://chatgpt.com")!),
        ("gemini-web", "Gemini (website)", URL(string: "https://gemini.google.com")!),
    ]

    /// The id in use: the saved one, else Claude if it's connected, else the first connected app, else "any".
    /// A saved app id the program doesn't list (any more) falls back the same way; with no list yet it is kept.
    static func resolve(saved: String?, clients: [AIClient]) -> String {
        if let s = saved?.trimmingCharacters(in: .whitespaces), !s.isEmpty {
            if s == anyID || websites.contains(where: { $0.id == s }) || clients.isEmpty || clients.contains(where: { $0.id == s }) {
                return s
            }
        }
        if clients.contains(where: { $0.id == "claude" && $0.connected }) { return "claude" }
        return clients.first(where: \.connected)?.id ?? anyID
    }

    static func of(_ id: String) -> YourAI {
        if id == "claude" { return .claude }
        if id == anyID || id.isEmpty { return .other }
        if let w = websites.first(where: { $0.id == id }) { return .website(id, w.url) }
        return .app(id)
    }

    /// Copy my day / Save from clipboard is the way (no MCP).
    var usesCopy: Bool {
        switch self { case .website, .other: return true; case .claude, .app: return false }
    }

    /// config.json "ai" (nil when not set).
    static func saved(in url: URL) -> String? {
        guard let data = try? Data(contentsOf: url), let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let s = obj["ai"] as? String, !s.trimmingCharacters(in: .whitespaces).isEmpty else { return nil }
        return s
    }

    /// The line shown after Copy my day.
    static let pasteLine = "Paste it into your AI. When it answers, copy its whole reply, then click Save from clipboard."

    /// What Save from clipboard says: a title, then what was saved and what wasn't (and why).
    static func summary(_ r: AIAnswer.Save) -> (title: String, detail: String) {
        let n = r.saved.count
        if n == 0 && r.refused.isEmpty {
            return ("No capsules found",
                    "The clipboard has no capsule in it. Copy your AI's whole reply, then click Save from clipboard again.")
        }
        let title = n == 0 ? "Nothing was saved" : "Saved \(n) capsule\(n == 1 ? "" : "s")"
        var lines: [String] = r.saved.map { "✓ \($0.title)" }
        if !r.refused.isEmpty {
            if !lines.isEmpty { lines.append("") }
            lines.append("Not saved:")
            lines += r.refused.map { $0.why.isEmpty ? "• \($0.title)" : "• \($0.title): \($0.why)" }
        }
        return (title, lines.joined(separator: "\n"))
    }
}

/// Where the context-capsule-mcp program is: inside the app, else (developing) in the repo's mcp/ folder.
enum AIProgram {
    enum Launch: Equatable { case binary(String), node(script: String) }

    /// `resources`: the app's Contents/Resources; `repos`: folders that may be the repo root (nearest first).
    static func find(resources: String?, repos: [String], exists: (String) -> Bool) -> Launch? {
        if let r = resources {
            let p = (r as NSString).appendingPathComponent("context-capsule-mcp")
            if exists(p) { return .binary(p) }
        }
        for root in repos {
            let bin = (root as NSString).appendingPathComponent("mcp/dist-bin/context-capsule-mcp-macos")
            if exists(bin) { return .binary(bin) }
        }
        for root in repos {
            let js = (root as NSString).appendingPathComponent("mcp/dist/index.js")
            if exists(js) { return .node(script: js) }
        }
        return nil
    }

    /// The repo roots to try for an app at `appPath` (…/capture/build/Context Capsule.app → …), then `cwd` and its parent.
    static func repoCandidates(appPath: String, cwd: String) -> [String] {
        var out: [String] = []
        var dir = (appPath as NSString).deletingLastPathComponent
        for _ in 0..<4 {
            out.append(dir)
            dir = (dir as NSString).deletingLastPathComponent
        }
        out += [cwd, (cwd as NSString).deletingLastPathComponent]
        var seen = Set<String>()
        return out.filter { seen.insert($0).inserted }
    }
}

// MARK: - Gaps: stretches that weren't read (SPEC "Timeline", gaps.jsonl)

enum Gaps {
    /// Shorter stretches aren't written.
    static let minimum: TimeInterval = 5
    static let codes = ["private_app", "not_shared", "no_text", "system", "locked"]

    /// The gaps.jsonl code for why a sample wrote nothing. The lock screen (or the screen saver) in front, or a
    /// locked screen, is "locked" whatever the sample said.
    static func why(_ skip: Skip, frontBundle: String? = nil, locked: Bool = false) -> String {
        let b = (frontBundle ?? "").lowercased()
        if locked || b.contains("com.apple.loginwindow") || b.contains("com.apple.screensaver") { return "locked" }
        switch skip {
        case .privateApp, .privateWindow: return "private_app"
        case .notPicked, .unknownDisplay: return "not_shared"
        case .overlay: return "system"
        case .noWindow, .waitingFrame: return "no_text"
        }
    }

    /// In plain words, after "Not read 10:40–10:55: ".
    static func message(_ why: String) -> String {
        switch why {
        case "private_app": return "a private app was in front"
        case "not_shared": return "this window wasn't shared"
        case "no_text": return "nothing readable"
        case "system": return "system screen"
        case "locked": return "the screen was locked"
        default: return "not read"
        }
    }

    /// One gaps.jsonl line (no newline): {"t","end","why"}.
    static func line(t: String, end: String, why: String) -> String {
        jsonObject([("t", .string(t)), ("end", .string(end)), ("why", .string(why))])
    }
}

/// Append-only gaps.jsonl, written like events: samples skipped for the same reason, one after another, make one line
/// whose `end` moves; a stretch under 5 s is never written. The file is made only when the first line is written.
final class GapLog {
    let url: URL
    private var run: (why: String, t: Date, end: Date)?
    /// Where the open stretch's line starts in the file, once it is written.
    private var lineStart: UInt64?
    /// Lines written by this log.
    private(set) var written = 0

    init(url: URL) { self.url = url }

    /// One sample: `why` is a gap code, or nil when the sample was read (that ends the stretch).
    func note(_ why: String?, at: Date) throws {
        guard let why else { close(); return }
        if var r = run, r.why == why, at >= r.end {
            r.end = at
            run = r
        } else {
            run = (why, at, at)
            lineStart = nil
        }
        guard let r = run, r.end.timeIntervalSince(r.t) >= Gaps.minimum else { return }
        let t = Clock.iso(r.t), end = Clock.iso(r.end)
        if !FileManager.default.fileExists(atPath: url.path) {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data().write(to: url)
        }
        let h = try FileHandle(forWritingTo: url)
        defer { try? h.close() }
        if let start = lineStart {
            // Same length (ISO times): overwrite just the end value in place.
            try h.seek(toOffset: start + UInt64(("{\"t\":" + jsonString(t) + ",\"end\":\"").utf8.count))
            try h.write(contentsOf: Data(end.utf8))
        } else {
            lineStart = try h.seekToEnd()
            try h.write(contentsOf: Data((Gaps.line(t: t, end: end, why: r.why) + "\n").utf8))
            written += 1
        }
    }

    /// The stretch is over (a sample was read, or recording stopped): the next skip starts a new one.
    func close() {
        run = nil
        lineStart = nil
    }
}

// MARK: - Timeline (SPEC "Timeline"): one day, everything recorded, grouped into stretches

extension ClaudeLink {
    /// Timeline → Make a capsule from this.
    static func sessionPrompt(_ id: String) -> String { "Use Context Capsule to make a capsule from my session \(id)." }
}

enum Timeline {
    /// One events.jsonl line. Screen text keeps only its first lines here (`preview`); the whole text is read from the
    /// file on Show all (`fullText`). Notes keep all their words.
    struct Moment: Identifiable, Equatable {
        let session: String
        /// The line's `t`, as written (to find the line again).
        let stamp: String
        let t: Date
        let end: Date
        let app: String
        let bundle: String
        let title: String
        let location: String
        /// Project id ("" = not sorted yet): a correction, the recorder's, else today's clues.
        let project: String
        /// "" for a screen moment, "voice" or "note".
        let kind: String
        /// Read because the mouse was over it on another shared screen.
        let lookedAt: Bool
        let preview: String
        /// More text than the preview shows.
        let more: Bool
        /// The first line of 20 or more characters, cut at 120 ("" if none).
        let peek: String
        /// How much text it has (bytes): the stretch's peek comes from the moment with the most.
        let size: Int
        /// Where the line is in events.jsonl, and its length in bytes (without the newline).
        let offset: Int
        let length: Int

        var id: String { "\(session)#\(offset)" }
        var isNote: Bool { kind == Event.voice || kind == Event.note }
    }

    struct Gap: Identifiable, Equatable {
        let session: String
        let t: Date
        let end: Date
        let why: String
        var id: String { "\(session)~\(Int(t.timeIntervalSince1970))~\(why)" }
        var message: String { Gaps.message(why) }
    }

    enum Item: Identifiable, Equatable {
        case moment(Moment)
        case gap(Gap)
        var id: String {
            switch self { case .moment(let m): return m.id; case .gap(let g): return g.id }
        }
        var time: Date {
            switch self { case .moment(let m): return m.t; case .gap(let g): return g.t }
        }
    }

    /// Screen moments in a row with the same app and title (SPEC "Stretches"). Never empty.
    struct Stretch: Identifiable, Equatable {
        let session: String
        /// Its moments, the reads, in time order.
        let reads: [Moment]
        /// While searching: the reads that match (nil when not searching).
        var found: [Moment]? = nil
        /// `t` of the first read, `end` of the last.
        let start: Date
        let end: Date
        let app: String
        let bundle: String
        let title: String
        /// The first `where` that isn't empty.
        let location: String
        /// From the read with the most text (the first, if several have as much).
        let peek: String
        /// The most-used project among its reads ("" = none).
        let project: String

        init(session: String, reads: [Moment]) {
            self.session = session
            self.reads = reads
            start = reads[0].t
            end = max(reads[reads.count - 1].end, start)
            app = reads[0].app
            bundle = reads[0].bundle
            title = reads[0].title
            location = reads.first { !$0.location.isEmpty }?.location ?? ""
            peek = reads.reduce(reads[0]) { $1.size > $0.size ? $1 : $0 }.peek
            project = Timeline.mostUsed(reads.map(\.project))
        }

        var length: TimeInterval { end.timeIntervalSince(start) }
        /// "App · title", or the app alone.
        var name: String {
            if title.isEmpty { return app.isEmpty ? "A window" : app }
            return app.isEmpty ? title : "\(app) · \(title)"
        }
        /// Stays the same while the recording goes on (the first read's place in the file).
        var id: String { reads[0].id }
    }

    /// What an opened recording lists, in time order.
    enum Entry: Identifiable, Equatable {
        case stretch(Stretch)
        case note(Moment)
        case gap(Gap)
        var id: String {
            switch self { case .stretch(let s): return "s-" + s.id; case .note(let m): return m.id; case .gap(let g): return g.id }
        }
    }

    /// One recording (session) on the day.
    struct Block: Identifiable, Equatable {
        let session: String
        var start: Date
        var end: Date
        /// The most-used project ("" = none).
        var project: String
        /// Screen moments (notes not counted).
        var moments: Int
        var notes: Int
        var items: [Item]
        /// The items grouped: stretches, notes and gaps.
        var entries: [Entry]
        /// This is the recording going on now.
        var recording: Bool
        var id: String { session }
        var stretches: [Stretch] {
            entries.compactMap { if case .stretch(let s) = $0 { return s } else { return nil } }
        }
    }

    struct Day: Equatable {
        var blocks: [Block] = []
        /// Moment id → its title, where, app and text, folded for search.
        var haystacks: [String: String] = [:]
    }

    static let previewLines = 3
    static let previewWidth = 300
    /// A stretch ends when the next read starts more than this after the last one's end.
    static let pause: TimeInterval = 60
    static let peekShortest = 20
    static let peekWidth = 120

    // MARK: reading

    static func isSessionID(_ id: String) -> Bool {
        id.count == 15 && id.enumerated().allSatisfy { i, c in i == 8 ? c == "-" : c.isASCII && c.isNumber }
    }

    /// "yyyyMMdd-HHmmss" (local time) → the moment it started.
    static func sessionStart(_ id: String, timeZone: TimeZone = .current) -> Date? {
        guard isSessionID(id) else { return nil }
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.calendar = Calendar(identifier: .gregorian)
        f.timeZone = timeZone
        f.dateFormat = "yyyyMMdd-HHmmss"
        return f.date(from: id)
    }

    /// Sessions that can overlap the day: started that day or the day before (a recording can run past midnight).
    static func candidates(_ names: [String], dayStart: Date, calendar: Calendar = .current) -> [String] {
        let today = String(Clock.sessionID(dayStart, timeZone: calendar.timeZone).prefix(8))
        let before = calendar.date(byAdding: .day, value: -1, to: dayStart)
            .map { String(Clock.sessionID($0, timeZone: calendar.timeZone).prefix(8)) }
        return names.filter { isSessionID($0) && ($0.hasPrefix(today) || before.map($0.hasPrefix) == true) }.sorted()
    }

    /// Days with a recording, for the calendar's dots: the local day each session started ("yyyyMMdd", from its id).
    static func recordedDays(_ names: [String]) -> Set<String> {
        Set(names.filter(isSessionID).map { String($0.prefix(8)) })
    }

    /// The day's key for `recordedDays`.
    static func dayKey(_ day: Date, timeZone: TimeZone = .current) -> String {
        String(Clock.sessionID(day, timeZone: timeZone).prefix(8))
    }

    /// The session being recorded now, from state.json (nil when not recording).
    static func recordingSession(state: URL) -> String? {
        guard let data = try? Data(contentsOf: state),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              obj["recording"] as? Bool == true, let id = obj["session_id"] as? String, !id.isEmpty else { return nil }
        return id
    }

    /// Every recording overlapping one local day, in time order, with its moments and gaps on that day.
    static func load(day: Date, sessions: URL, projects: [Project] = [], recording: String? = nil,
                     calendar: Calendar = .current) -> Day {
        let from = calendar.startOfDay(for: day)
        let to = calendar.date(byAdding: .day, value: 1, to: from) ?? from.addingTimeInterval(86_400)
        let names = (try? FileManager.default.contentsOfDirectory(atPath: sessions.path)) ?? []
        var out = Day()
        for id in candidates(names, dayStart: from, calendar: calendar) {
            let dir = sessions.appendingPathComponent(id, isDirectory: true)
            if let (block, hay) = block(id, dir: dir, from: from, to: to, projects: projects, recording: recording == id,
                                        timeZone: calendar.timeZone) {
                out.blocks.append(block)
                out.haystacks.merge(hay) { a, _ in a }
            }
        }
        out.blocks.sort { $0.start < $1.start }
        return out
    }

    /// One session's part of [from, to): nil when nothing of it is on that day.
    static func block(_ id: String, dir: URL, from: Date, to: Date, projects: [Project] = [], recording: Bool = false,
                      timeZone: TimeZone = .current) -> (Block, [String: String])? {
        let fixes = Assignments.load(dir.appendingPathComponent("assign.json"))
        let events = (try? Data(contentsOf: dir.appendingPathComponent("events.jsonl"))) ?? Data()
        let parsed = moments(events, session: id, from: from, to: to, fixes: fixes, projects: projects)
        let gapData = (try? Data(contentsOf: dir.appendingPathComponent("gaps.jsonl"))) ?? Data()
        let gapList = gaps(gapData, session: id, from: from, to: to)
        let started = sessionStart(id, timeZone: timeZone)
        let startedToday = started.map { $0 >= from && $0 < to } ?? false
        if parsed.isEmpty && gapList.isEmpty && !startedToday { return nil }

        var items = parsed.map { Item.moment($0.0) } + gapList.map { Item.gap($0) }
        items.sort { $0.time < $1.time }
        var byProject: [String: Double] = [:]
        for (m, _) in parsed where !m.isNote && !m.lookedAt {
            byProject[m.project, default: 0] += max(3, m.end.timeIntervalSince(m.t))
        }
        let project = byProject.filter { !$0.key.isEmpty }
            .max { $0.value < $1.value || ($0.value == $1.value && $0.key > $1.key) }?.key ?? ""
        let times = parsed.flatMap { [$0.0.t, $0.0.end] } + gapList.flatMap { [$0.t, $0.end] }
        let start = times.min() ?? started ?? from
        var end = times.max() ?? start
        if times.isEmpty, let ended = endedAt(dir) { end = min(max(ended, start), to) }
        let notes = parsed.filter { $0.0.isNote }.count
        let block = Block(session: id, start: start, end: end, project: project, moments: parsed.count - notes, notes: notes,
                          items: items, entries: entries(items), recording: recording)
        var hay: [String: String] = [:]
        for (m, h) in parsed { hay[m.id] = h }
        return (block, hay)
    }

    private static func endedAt(_ dir: URL) -> Date? {
        guard let data = try? Data(contentsOf: dir.appendingPathComponent("session.json")),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let s = obj["ended_at"] as? String else { return nil }
        return ISO8601DateFormatter().date(from: s)
    }

    /// The byte ranges of the lines in a file (without their newlines; empty lines left out).
    static func lineRanges(_ data: Data) -> [Range<Int>] {
        var out: [Range<Int>] = []
        data.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
            guard let base = raw.baseAddress else { return }
            var start = 0
            let n = raw.count
            while start < n {
                let hit = memchr(base + start, 0x0A, n - start)
                let stop = hit.map { UnsafeRawPointer($0) - base } ?? n
                if stop > start { out.append(start..<stop) }
                start = stop + 1
            }
        }
        return out
    }

    /// An ISO-8601 UTC time as the recorders write it ("2026-10-21T04:15:00Z"), read quickly; anything else through
    /// the system's parser.
    static func date(_ s: String, _ iso: ISO8601DateFormatter) -> Date? {
        let u = Array(s.utf8)
        func n(_ a: Int, _ b: Int) -> Int? {
            var v = 0
            for i in a..<b { let c = u[i]; guard c >= 48 && c <= 57 else { return nil }; v = v * 10 + Int(c - 48) }
            return v
        }
        guard u.count == 20, u[4] == 45, u[7] == 45, u[10] == 84, u[13] == 58, u[16] == 58, u[19] == 90,
              let y = n(0, 4), let mo = n(5, 7), let d = n(8, 10), let h = n(11, 13), let mi = n(14, 16), let sec = n(17, 19),
              (1...12).contains(mo), (1...31).contains(d), h < 24, mi < 60, sec < 61 else { return iso.date(from: s) }
        // Days from 1970-01-01 (civil calendar, proleptic Gregorian).
        let yy = mo <= 2 ? y - 1 : y
        let era = (yy >= 0 ? yy : yy - 399) / 400
        let yoe = yy - era * 400
        let doy = (153 * (mo + (mo > 2 ? -3 : 9)) + 2) / 5 + d - 1
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        let days = era * 146_097 + doe - 719_468
        return Date(timeIntervalSince1970: TimeInterval(days * 86_400 + h * 3600 + mi * 60 + sec))
    }

    /// A string from JSONSerialization, made a native Swift string (fast to split and search).
    private static func native(_ v: Any?) -> String {
        guard var s = v as? String else { return "" }
        s.makeContiguousUTF8()
        return s
    }

    /// The moments of events.jsonl that start in [from, to), each with its search text. Broken lines are skipped.
    static func moments(_ data: Data, session: String, from: Date, to: Date, fixes: [Assignments.Fix] = [],
                        projects: [Project] = []) -> [(Moment, String)] {
        let iso = ISO8601DateFormatter()
        var out: [(Moment, String)] = []
        for r in lineRanges(data) {
            guard let obj = try? JSONSerialization.jsonObject(with: data.subdata(in: r)) as? [String: Any],
                  let stamp = obj["t"] as? String, let t = date(stamp, iso), t >= from, t < to else { continue }
            let end = (obj["end"] as? String).flatMap { date($0, iso) }.map { max($0, t) } ?? t
            let kind = obj["kind"] as? String ?? ""
            let isNote = kind == Event.voice || kind == Event.note
            let app = native(obj["app"]), title = native(obj["title"])
            let location = native(obj["where"]), text = native(obj["text"])
            var project = obj["project"] as? String ?? ""
            if let fixed = Assignments.at(fixes, t) {
                project = fixed
            } else if project.isEmpty, !(isNote && app.isEmpty),
                      let p = Projects.detect(projects, title: title, location: location, app: app) {
                project = p.id
            }
            let folded = isNote ? (text.trimmingCharacters(in: .whitespacesAndNewlines), false) : fold(text)
            let m = Moment(session: session, stamp: stamp, t: t, end: end, app: app, bundle: obj["bundle"] as? String ?? "",
                           title: title, location: location, project: project, kind: isNote ? kind : "",
                           lookedAt: (obj["focus"] as? String) == "pointer", preview: folded.0, more: folded.1,
                           peek: isNote ? "" : peek(text), size: text.utf8.count, offset: r.lowerBound, length: r.count)
            out.append((m, searchable([title, location, app, text].joined(separator: "\n"))))
        }
        return out
    }

    /// The gaps of gaps.jsonl that overlap [from, to), cut to it.
    static func gaps(_ data: Data, session: String, from: Date, to: Date) -> [Gap] {
        let iso = ISO8601DateFormatter()
        var out: [Gap] = []
        for r in lineRanges(data) {
            guard let obj = try? JSONSerialization.jsonObject(with: data.subdata(in: r)) as? [String: Any],
                  let t = (obj["t"] as? String).flatMap({ date($0, iso) }), let why = obj["why"] as? String else { continue }
            let end = (obj["end"] as? String).flatMap { date($0, iso) }.map { max($0, t) } ?? t
            guard end >= from, t < to else { continue }
            out.append(Gap(session: session, t: max(t, from), end: min(end, to), why: why))
        }
        return out
    }

    /// The first lines of a moment's text (blank lines left out, long lines cut with "…"), and whether there's more.
    static func fold(_ text: String, lines: Int = previewLines, width: Int = previewWidth) -> (String, Bool) {
        var kept: [String] = []
        var more = false
        for raw in text.split(whereSeparator: \.isNewline) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.isEmpty { continue }
            if kept.count == lines { more = true; break }
            if line.count > width {
                kept.append(String(line.prefix(width - 1)) + "…")
                more = true
            } else {
                kept.append(line)
            }
        }
        return (kept.joined(separator: "\n"), more)
    }

    /// The first line of 20 or more characters (trimmed), cut at 120 with "…"; "" if no line is that long.
    static func peek(_ text: String) -> String {
        for raw in text.split(whereSeparator: \.isNewline) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.count < peekShortest { continue }
            return line.count > peekWidth ? String(line.prefix(peekWidth)) + "…" : line
        }
        return ""
    }

    // MARK: stretches

    /// The most common value that isn't empty (the first seen wins a tie); "" if none.
    static func mostUsed(_ values: [String]) -> String {
        var count: [String: Int] = [:]
        var best = ""
        for v in values where !v.isEmpty {
            count[v, default: 0] += 1
            if v != best && count[v]! > count[best, default: 0] { best = v }
        }
        return best
    }

    /// A recording's items (in time order) grouped: screen moments in a row with the same app and the same title make
    /// one stretch; a note, a gap, or more than a minute between two reads ends it.
    static func entries(_ items: [Item]) -> [Entry] {
        var out: [Entry] = []
        var run: [Moment] = []
        func close() {
            if let first = run.first { out.append(.stretch(Stretch(session: first.session, reads: run))) }
            run = []
        }
        for item in items {
            switch item {
            case .gap(let g):
                close()
                out.append(.gap(g))
            case .moment(let m) where m.isNote:
                close()
                out.append(.note(m))
            case .moment(let m):
                if let last = run.last,
                   last.app != m.app || last.title != m.title || m.t.timeIntervalSince(last.end) > pause { close() }
                run.append(m)
            }
        }
        close()
        return out
    }

    /// The day's recorded time: the stretches' lengths added up.
    static func recorded(_ blocks: [Block]) -> TimeInterval {
        blocks.reduce(0) { $0 + $1.stretches.reduce(0) { $0 + $1.length } }
    }

    /// One row of Where your time went: a project id ("" = not sorted), or an app when nothing has a project.
    struct Share: Equatable {
        let key: String
        let seconds: TimeInterval
    }

    /// Time per project, longest first; per app instead when no stretch has a project.
    static func shares(_ stretches: [Stretch]) -> (byProject: Bool, rows: [Share]) {
        let byProject = stretches.contains { !$0.project.isEmpty }
        var time: [String: TimeInterval] = [:]
        var order: [String] = []
        for s in stretches {
            let key = byProject ? s.project : s.app
            if time[key] == nil { order.append(key) }
            time[key, default: 0] += s.length
        }
        // Longest first; a tie keeps the order they came in.
        let rows = order.enumerated().sorted { a, b in
            time[a.element]! != time[b.element]! ? time[a.element]! > time[b.element]! : a.offset < b.offset
        }.map { Share(key: $0.element, seconds: time[$0.element]!) }
        return (byProject, rows)
    }

    /// What a folded recording names: its projects by time, or its top 2 apps when none has a project.
    static func names(_ block: Block) -> (byProject: Bool, keys: [String]) {
        let s = shares(block.stretches)
        let keys = s.rows.map(\.key).filter { !$0.isEmpty }
        return (s.byProject, s.byProject ? keys : Array(keys.prefix(2)))
    }

    /// The day bar: from the start of the hour of the first stretch to the end of the hour of the last.
    static func barRange(_ stretches: [Stretch], calendar: Calendar = .current) -> (from: Date, to: Date)? {
        guard let first = stretches.map(\.start).min(), let last = stretches.map(\.end).max(),
              let a = calendar.dateInterval(of: .hour, for: first), let b = calendar.dateInterval(of: .hour, for: last)
        else { return nil }
        return (a.start, b.end)
    }

    /// "under 1 min", "38 min", "1 h", "1 h 50 min" (to the nearest minute).
    static func length(_ seconds: TimeInterval) -> String {
        if seconds < 60 { return "under 1 min" }
        let m = Int((seconds / 60).rounded())
        if m < 60 { return "\(m) min" }
        return m % 60 == 0 ? "\(m / 60) h" : "\(m / 60) h \(m % 60) min"
    }

    /// A moment's whole text, read again from its line in events.jsonl (Show all).
    static func fullText(_ m: Moment, sessions: URL) -> String? {
        let url = sessions.appendingPathComponent(m.session).appendingPathComponent("events.jsonl")
        guard let h = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? h.close() }
        guard (try? h.seek(toOffset: UInt64(m.offset))) != nil, let data = try? h.read(upToCount: m.length),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              obj["t"] as? String == m.stamp else { return nil }
        return obj["text"] as? String
    }

    // MARK: search

    /// Lower case, without accents: what search compares.
    static func searchable(_ s: String) -> String {
        s.folding(options: [.caseInsensitive, .diacriticInsensitive, .widthInsensitive], locale: nil)
    }

    static func words(_ query: String) -> [String] {
        searchable(query).split(whereSeparator: { $0.isWhitespace }).map(String.init)
    }

    /// Every word is somewhere in the moment's title, where, app or text.
    static func matches(_ haystack: String, words: [String]) -> Bool {
        words.allSatisfy { haystack.contains($0) }
    }

    /// The ids of the moments (reads and notes) that match; nil for an empty query.
    static func hits(_ day: Day, query: String) -> Set<String>? {
        let w = words(query)
        guard !w.isEmpty else { return nil }
        return Set(day.haystacks.filter { matches($0.value, words: w) }.keys)
    }

    /// An opened recording while searching: the stretches with a read that matches (`found` set to those reads) and
    /// the notes that match. No gaps.
    static func filter(_ entries: [Entry], hits: Set<String>) -> [Entry] {
        entries.compactMap { e in
            switch e {
            case .stretch(var s):
                let found = s.reads.filter { hits.contains($0.id) }
                if found.isEmpty { return nil }
                s.found = found
                return .stretch(s)
            case .note(let m): return hits.contains(m.id) ? e : nil
            case .gap: return nil
            }
        }
    }

    /// Every recording of the day, each with only what matches the search. An empty query: all, as they are.
    static func filter(_ day: Day, query: String) -> [Block] {
        guard let hits = hits(day, query: query) else { return day.blocks }
        return day.blocks.map { b in
            var b = b
            b.entries = filter(b.entries, hits: hits)
            return b
        }
    }

    // MARK: delete

    enum DeleteError: Error, Equatable {
        /// That session is still recording: stop first.
        case recording
        /// The line isn't there anymore (or the file changed): reload.
        case notFound
        case badName
    }

    /// The file without the first line that is exactly `line` and has `t` as its time; nil if there's no such line.
    static func removingLine(_ content: Data, t: String, line: Data) -> Data? {
        guard let obj = try? JSONSerialization.jsonObject(with: line) as? [String: Any], obj["t"] as? String == t else { return nil }
        for r in lineRanges(content) where r.count == line.count && content.subdata(in: r) == line {
            var out = content
            let stop = r.upperBound < content.count ? r.upperBound + 1 : r.upperBound // and its newline
            out.removeSubrange(r.lowerBound..<stop)
            return out
        }
        return nil
    }

    /// Removes one moment from its events.jsonl (the file is rewritten at once). Not while that session records.
    static func deleteMoment(_ m: Moment, sessions: URL, state: URL) throws {
        try deleteMoments([m], sessions: sessions, state: state)
    }

    /// Removes moments of one recording (a whole stretch) from its events.jsonl, all or none, in one rewrite.
    static func deleteMoments(_ ms: [Moment], sessions: URL, state: URL) throws {
        guard let session = ms.first?.session else { return }
        guard isSessionID(session), ms.allSatisfy({ $0.session == session }) else { throw DeleteError.badName }
        if recordingSession(state: state) == session { throw DeleteError.recording }
        let url = sessions.appendingPathComponent(session).appendingPathComponent("events.jsonl")
        let data = try Data(contentsOf: url)
        var out = data
        for m in ms {
            guard m.offset >= 0, m.offset + m.length <= data.count,
                  let next = removingLine(out, t: m.stamp, line: data.subdata(in: m.offset..<(m.offset + m.length))) else {
                throw DeleteError.notFound
            }
            out = next
        }
        try out.write(to: url, options: .atomic)
    }

    /// Removes a whole recording (its folder). Not while it records.
    static func deleteSession(_ id: String, sessions: URL, state: URL) throws {
        guard isSessionID(id) else { throw DeleteError.badName }
        if recordingSession(state: state) == id { throw DeleteError.recording }
        let dir = sessions.appendingPathComponent(id, isDirectory: true)
        guard FileManager.default.fileExists(atPath: dir.path) else { throw DeleteError.notFound }
        try FileManager.default.removeItem(at: dir)
    }
}
