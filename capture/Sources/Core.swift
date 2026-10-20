// context-capsule — capture: pure logic (paths, JSON, times, redaction, exclusion, dedupe, event log,
// picked-content membership, Accessibility text assembly, AX-vs-OCR choice, voice-note logic).
// No UI or screen access here, so the tests can compile this file on its own.

import CoreGraphics
import Foundation

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

    static let standard = Paths(root: FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".context-capsule", isDirectory: true))
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
    static let textCap = 4000

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

    /// Reads `{"exclude_apps": [...], "voice_locale": "en-GB"}`; a missing or broken file means defaults.
    static func load(from url: URL) -> Config {
        guard let data = try? Data(contentsOf: url),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return Config() }
        let locale = (obj["voice_locale"] as? String)?.trimmingCharacters(in: .whitespaces)
        return Config(excludeApps: (obj["exclude_apps"] as? [String]) ?? [],
                      voiceLocale: locale?.isEmpty == false ? locale : nil,
                      askProject: obj["ask_project"] as? Bool ?? false)
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

    static let voice = "voice"
    static let note = "note"

    /// Spoken or typed by the user (not a screen sample). Never merged with another event.
    var isNote: Bool { kind == Event.voice || kind == Event.note }

    func sameContent(as o: Event) -> Bool {
        !isNote && !o.isNote && kind == o.kind && src == o.src
            && app == o.app && bundle == o.bundle && title == o.title && text == o.text
            && location == o.location && project == o.project && projectBy == o.projectBy
    }

    /// One events.jsonl line (no newline), keys in SPEC order.
    /// Key order: t, end, [kind, src,] app, bundle, title, [where], [project, project_by], [src,] text.
    var line: String {
        var pairs: [(String, JSONValue)] = [("t", .string(t)), ("end", .string(end))]
        if !kind.isEmpty { pairs += [("kind", .string(kind)), ("src", .string(src))] }
        pairs += [("app", .string(app)), ("bundle", .string(bundle)), ("title", .string(title))]
        if !location.isEmpty { pairs.append(("where", .string(location))) }
        if !project.isEmpty { pairs += [("project", .string(project)), ("project_by", .string(projectBy))] }
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
    static let maxNodes = 2000
    static let budget: TimeInterval = 0.4
    static let maxDepth = 60
    /// Stop collecting past this (the event cap is 4000 after redaction).
    static let maxChars = 8000

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
        case .notRecording: return "Start recording first (⌥C)"
        case .denied(.microphone):
            return "Context Capsule can't use the microphone. Turn it on in System Settings → Privacy & Security → Microphone."
        case .denied(.speech):
            return "Context Capsule can't use speech recognition. Turn it on in System Settings → Privacy & Security → Speech Recognition."
        case .unavailable(let locale):
            return "On-device speech isn't available for \(locale), so voice notes are off (nothing is sent to Apple). "
                + "Set \"voice_locale\" in ~/.context-capsule/config.json, or add the language in System Settings → Keyboard → Dictation."
        case .failed(let why): return "Voice note failed: \(why)"
        case .discarded: return "Voice note not saved: recording had stopped"
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

/// Opening Claude with a message already typed (the user still presses Enter).
enum ClaudeLink {
    static let dayPrompt = "Make my capsules for today."

    static func newChat(prefill text: String) -> URL? {
        var parts = URLComponents(string: "claude://claude.ai/new")
        parts?.queryItems = [URLQueryItem(name: "q", value: text)]
        return parts?.url
    }
}
