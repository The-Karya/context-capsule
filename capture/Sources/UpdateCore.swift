// context-capsule — capture: pure update logic (the feed, its Ed25519 signature, versions, which file is ours,
// when to check). No network or UI here, so the tests compile it on their own. The contract is shared with
// Windows and Linux: one signed latest.json for all platforms (see release/README.md).

import CryptoKit
import Foundation

/// A version like "0.2.0" (a leading "v" and a "-beta.1" tail are allowed). A tail sorts before the plain one.
struct SemVer: Comparable, CustomStringConvertible {
    let numbers: [Int]
    let pre: [String]

    init?(_ text: String) {
        var s = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if s.hasPrefix("v") || s.hasPrefix("V") { s.removeFirst() }
        s = String(s.split(separator: "+", maxSplits: 1, omittingEmptySubsequences: false).first ?? "")
        let parts = s.split(separator: "-", maxSplits: 1, omittingEmptySubsequences: false)
        let nums = (parts.first ?? "").split(separator: ".", omittingEmptySubsequences: false).map { Int($0) }
        guard !nums.isEmpty, nums.count <= 3, !nums.contains(where: { $0 == nil || $0! < 0 }) else { return nil }
        numbers = nums.map { $0! } + Array(repeating: 0, count: 3 - nums.count)
        pre = parts.count > 1 ? parts[1].split(separator: ".").map(String.init) : []
        if parts.count > 1 && pre.isEmpty { return nil }
    }

    var description: String { numbers.map(String.init).joined(separator: ".") + (pre.isEmpty ? "" : "-" + pre.joined(separator: ".")) }

    static func == (a: SemVer, b: SemVer) -> Bool { a.numbers == b.numbers && a.pre == b.pre }

    static func < (a: SemVer, b: SemVer) -> Bool {
        if a.numbers != b.numbers { return a.numbers.lexicographicallyPrecedes(b.numbers) }
        if a.pre.isEmpty || b.pre.isEmpty { return !a.pre.isEmpty && b.pre.isEmpty }
        for (x, y) in zip(a.pre, b.pre) where x != y {
            switch (Int(x), Int(y)) {
            case let (i?, j?): return i < j
            case (_?, nil): return true
            case (nil, _?): return false
            default: return x < y
            }
        }
        return a.pre.count < b.pre.count
    }
}

/// One platform's download in latest.json.
struct UpdateAsset: Equatable {
    let url: URL
    let sha256: String
    let size: Int
}

/// latest.json: {"version","date","notes","mac":{url,sha256,size},"windows":…,"linux":…}. Any platform may be missing.
struct UpdateFeed: Equatable {
    let version: String
    let date: String
    let notes: String
    let assets: [String: UpdateAsset]
    /// Staged rollout: the share of people (0–100) who get this version by themselves. Missing = 100.
    var rollout = 100

    static let platforms = ["mac", "windows", "linux"]

    static func parse(_ data: Data) -> UpdateFeed? {
        guard let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let version = obj["version"] as? String, SemVer(version) != nil else { return nil }
        var assets: [String: UpdateAsset] = [:]
        for key in platforms {
            guard let a = obj[key] as? [String: Any] else { continue }
            guard let asset = Self.asset(a) else { return nil } // a broken entry: distrust the whole feed
            assets[key] = asset
        }
        var rollout = 100
        if let r = obj["rollout"] {
            guard let n = (r as? NSNumber)?.intValue, (0...100).contains(n) else { return nil }
            rollout = n
        }
        return UpdateFeed(version: version, date: obj["date"] as? String ?? "", notes: obj["notes"] as? String ?? "",
                          assets: assets, rollout: rollout)
    }

    /// https only, a 64-hex sha256, a size between 1 byte and 1 GB.
    private static func asset(_ a: [String: Any]) -> UpdateAsset? {
        guard let s = a["url"] as? String, let url = URL(string: s), url.scheme == "https", url.host?.isEmpty == false,
              let sha = (a["sha256"] as? String)?.lowercased(), sha.count == 64,
              sha.allSatisfy({ "0123456789abcdef".contains($0) }),
              let size = (a["size"] as? NSNumber)?.intValue, size > 0, size <= 1 << 30 else { return nil }
        return UpdateAsset(url: url, sha256: sha, size: size)
    }
}

enum Update {
    /// The release signing key (Ed25519, raw 32 bytes, base64). Only latest.json signed with its private half is trusted.
    static let publicKey = "WHTBrAjhTOQIG3S15PKYqHHdj5nbon4J/eEam7LrXd4="
    static let defaultFeedBase = "https://github.com/The-Karya/context-capsule/releases/latest/download/"
    static let platform = "mac"
    /// First check this long after launch, then once a day.
    static let launchDelay: TimeInterval = 60
    static let interval: TimeInterval = 24 * 60 * 60

    /// latest.json and latest.json.sig under the base URL (config.json "update_feed" can change the base).
    static func feedURLs(base: String?) -> (feed: URL, signature: URL)? {
        var b = (base ?? "").trimmingCharacters(in: .whitespaces)
        if b.isEmpty { b = defaultFeedBase }
        if !b.hasSuffix("/") { b += "/" }
        guard let feed = URL(string: b + "latest.json"), feed.scheme == "https",
              let sig = URL(string: b + "latest.json.sig") else { return nil }
        return (feed, sig)
    }

    /// latest.json.sig is base64 of the raw 64-byte Ed25519 signature over the exact bytes of latest.json.
    static func verify(feed: Data, signature: Data, publicKey: String = Update.publicKey) -> Bool {
        let text = String(decoding: signature, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        guard let sig = Data(base64Encoded: text), sig.count == 64,
              let raw = Data(base64Encoded: publicKey), raw.count == 32,
              let key = try? Curve25519.Signing.PublicKey(rawRepresentation: raw) else { return false }
        return key.isValidSignature(sig, for: feed)
    }

    enum Decision: Equatable {
        case badSignature
        case badFeed
        case upToDate
        case noBuildForThisPlatform(version: String)
        case update(version: String, asset: UpdateAsset)
        /// A staged rollout that hasn't reached this computer yet (or is stopped, at 0).
        case notYet(version: String)
    }

    /// The order matters: the signature first, then the version (only newer), then our platform's file.
    /// Staged rollout: an automatic check only takes the version when this computer's bucket (0–99, kept for good)
    /// is under the rollout; a person who clicks Check for Updates gets it unless the rollout is stopped (0).
    static func decide(feed: Data, signature: Data, current: String, platform: String = Update.platform,
                       publicKey: String = Update.publicKey, bucket: Int = 0, manual: Bool = true) -> Decision {
        guard verify(feed: feed, signature: signature, publicKey: publicKey) else { return .badSignature }
        guard let parsed = UpdateFeed.parse(feed), let new = SemVer(parsed.version) else { return .badFeed }
        guard let now = SemVer(current), new > now else { return .upToDate }
        guard let asset = parsed.assets[platform] else { return .noBuildForThisPlatform(version: parsed.version) }
        guard parsed.rollout > 0, manual || bucket < parsed.rollout else { return .notYet(version: parsed.version) }
        return .update(version: parsed.version, asset: asset)
    }

    /// This computer's place in staged rollouts: a number 0–99 picked once and kept in `.update-bucket`
    /// (the same file on every platform). Nothing about the person; it never leaves the computer.
    static func bucket(in dir: URL) -> Int {
        let file = dir.appendingPathComponent(".update-bucket")
        if let text = try? String(contentsOf: file, encoding: .utf8), let n = Int(text.trimmingCharacters(in: .whitespacesAndNewlines)),
           (0..<100).contains(n) { return n }
        let n = Int.random(in: 0..<100)
        try? "\(n)\n".write(to: file, atomically: true, encoding: .utf8)
        return n
    }

    /// Automatic checks: on, not busy (contextualizing or a voice note), a minute after launch, then daily.
    static func shouldCheck(now: Date, launchedAt: Date, lastCheck: Date?, auto: Bool, busy: Bool) -> Bool {
        guard auto, !busy, now.timeIntervalSince(launchedAt) >= launchDelay else { return false }
        guard let last = lastCheck else { return true }
        return now.timeIntervalSince(last) >= interval
    }

    static func sha256(_ data: Data) -> String { SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined() }

    /// The SHA-256 of a file, read in pieces.
    static func sha256(file: URL) -> String? {
        guard let h = try? FileHandle(forReadingFrom: file) else { return nil }
        defer { try? h.close() }
        var hasher = SHA256()
        do {
            // read(upToCount:) gives nil or empty data at the end.
            while let chunk = try h.read(upToCount: 1 << 20), !chunk.isEmpty { hasher.update(data: chunk) }
        } catch { return nil }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    /// nil when the downloaded file is exactly the one in the feed; else why not.
    static func checkDownload(_ file: URL, against asset: UpdateAsset) -> String? {
        let size = (try? FileManager.default.attributesOfItem(atPath: file.path))?[.size] as? Int ?? -1
        guard size == asset.size else { return "size \(size), expected \(asset.size)" }
        guard let sum = sha256(file: file) else { return "cannot read the download" }
        return sum == asset.sha256 ? nil : "sha256 does not match"
    }

    /// The bundle's folder is /Applications or ~/Applications (else we offer to move it there).
    static func isInApplications(_ bundlePath: String, home: String) -> Bool {
        let parent = (bundlePath as NSString).deletingLastPathComponent
        return parent == "/Applications" || parent == (home as NSString).appendingPathComponent("Applications")
    }

    /// Code-signing requirement the new app must meet: same identifier and same signer as the running app.
    /// Developer ID: the same team. A local certificate: the same leaf certificate. Ad-hoc: nil (no updates).
    static func requirement(identifier: String, teamID: String?, leafCertSHA1: String?) -> String? {
        let id = "identifier \"\(identifier)\""
        if let team = teamID, !team.isEmpty, team.allSatisfy({ $0.isLetter || $0.isNumber }) {
            return "\(id) and anchor apple generic and certificate leaf[subject.OU] = \"\(team)\""
        }
        if let hash = leafCertSHA1, hash.count == 40, hash.allSatisfy({ $0.isHexDigit }) {
            return "\(id) and certificate leaf = H\"\(hash.lowercased())\""
        }
        return nil
    }
}
