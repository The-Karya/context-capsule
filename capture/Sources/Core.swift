// karya-capsule — capture: pure logic (paths, JSON, times, redaction, exclusion, dedupe, event log).
// No UI or screen access here, so the tests can compile this file on its own.

import Foundation

// MARK: - Paths

struct Paths {
    let root: URL
    var sessions: URL { root.appendingPathComponent("sessions", isDirectory: true) }
    var config: URL { root.appendingPathComponent("config.json") }
    var control: URL { root.appendingPathComponent("control") }
    var state: URL { root.appendingPathComponent("state.json") }

    static let standard = Paths(root: FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent(".karya-capsule", isDirectory: true))
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
    var text: String

    func sameContent(as o: Event) -> Bool {
        app == o.app && bundle == o.bundle && title == o.title && text == o.text
    }

    /// One events.jsonl line (no newline), keys in SPEC order.
    var line: String {
        jsonObject([("t", .string(t)), ("end", .string(end)), ("app", .string(app)),
                    ("bundle", .string(bundle)), ("title", .string(title)), ("text", .string(text))])
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

func stateJSON(recording: Bool, sessionID: String?, startedAt: Date?, pid: Int32) -> String {
    jsonObject([("recording", .bool(recording)),
                ("session_id", sessionID.map { .string($0) } ?? .null),
                ("started_at", startedAt.map { .string(Clock.iso($0)) } ?? .null),
                ("pid", .int(Int(pid)))])
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
