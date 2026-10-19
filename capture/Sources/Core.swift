// context-capsule — capture: pure logic (paths, JSON, times, redaction, exclusion, dedupe, event log,
// picked-content membership, Accessibility text assembly, AX-vs-OCR choice).
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

    /// Reads `{"exclude_apps": [...]}`; a missing or broken file means no extra exclusions.
    static func load(from url: URL) -> Config {
        guard let data = try? Data(contentsOf: url),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return Config() }
        return Config(excludeApps: (obj["exclude_apps"] as? [String]) ?? [])
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

    func sameContent(as o: Event) -> Bool {
        app == o.app && bundle == o.bundle && title == o.title && text == o.text
    }

    /// One events.jsonl line (no newline), keys in SPEC order.
    var line: String {
        var pairs: [(String, JSONValue)] = [("t", .string(t)), ("end", .string(end)), ("app", .string(app)),
                                            ("bundle", .string(bundle)), ("title", .string(title))]
        if !src.isEmpty { pairs.append(("src", .string(src))) }
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
func stateJSON(recording: Bool, sessionID: String?, startedAt: Date?, pid: Int32,
               picking: Bool = false, lastError: String? = nil) -> String {
    jsonObject([("recording", .bool(recording)),
                ("session_id", sessionID.map { .string($0) } ?? .null),
                ("started_at", startedAt.map { .string(Clock.iso($0)) } ?? .null),
                ("pid", .int(Int(pid))),
                ("picking", .bool(picking)),
                ("last_error", lastError.map { .string($0) } ?? .null)])
}

struct ControlCommand: Equatable {
    enum Kind: String { case start, stop }
    var kind: Kind
    var at: Int64

    /// `{"command":"start"|"stop","at":<ms>}`; anything else is ignored.
    static func parse(_ data: Data) -> ControlCommand? {
        guard let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let name = obj["command"] as? String, let kind = Kind(rawValue: name.lowercased()) else { return nil }
        let at = (obj["at"] as? NSNumber)?.int64Value ?? 0
        return ControlCommand(kind: kind, at: at)
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
        let full = CGRect(x: 0, y: 0, width: 1, height: 1)
        if pick.style == .window { return full }
        let c = pick.rect
        guard !c.isEmpty else { return nil }
        let i = c.intersection(window)
        guard !i.isNull, i.width >= 1, i.height >= 1 else { return nil }
        let r = CGRect(x: (i.minX - c.minX) / c.width, y: (c.maxY - i.maxY) / c.height,
                       width: i.width / c.width, height: i.height / c.height)
        return r.intersection(full)
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
