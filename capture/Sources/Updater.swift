// context-capsule — capture: updates and "Move to Applications". A small built-in updater, no framework:
// fetch latest.json + its Ed25519 signature, check the signature, then the version, download the Mac zip, check its
// size and SHA-256, unzip it next to the app, check the new app is signed by the same signer as this one, and on
// "Restart now" swap the app in place and open it again. Never while contextualizing or during a voice note.
// The only network use in the app. Nothing about the person is sent.

import AppKit
import CryptoKit
import Security
import SwiftUI

final class Updater {
    private let paths: Paths
    /// Contextualizing or a voice note: no automatic check, no prompt, no restart.
    private let isBusy: () -> Bool
    /// The status line changed (Settings, the menu).
    var onChange: () -> Void = {}
    /// A new version is unpacked and checked: show "Restart to update".
    var onReady: (String) -> Void = { _ in }

    private(set) var line = ""
    private(set) var readyVersion: String?
    private(set) var working = false
    private var staged: (app: URL, folder: URL)?
    private var prompted: String?
    private var lastCheck: Date?
    private let launchedAt = Date()
    private var timer: Timer?
    private let session: URLSession = {
        let c = URLSessionConfiguration.ephemeral // no cookies, no cache
        c.httpAdditionalHeaders = ["User-Agent": "ContextCapsule"]
        c.timeoutIntervalForRequest = 30
        c.timeoutIntervalForResource = 15 * 60
        return URLSession(configuration: c)
    }()

    static var currentVersion: String { Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.0.0" }

    init(paths: Paths, isBusy: @escaping () -> Bool) {
        self.paths = paths
        self.isBusy = isBusy
    }

    func start() {
        timer = repeating(30) { [weak self] in self?.tick() }
    }

    private func tick() {
        let config = Config.load(from: paths.config)
        if readyVersion == nil, !working,
           Update.shouldCheck(now: Date(), launchedAt: launchedAt, lastCheck: lastCheck, auto: config.autoUpdate, busy: isBusy()) {
            check(manual: false)
        }
        if let v = readyVersion, prompted != v, !isBusy() {
            prompted = v
            onReady(v)
        }
    }

    // MARK: checking

    /// `manual`: the person clicked "Check for Updates": they're told the result. Automatic checks are silent
    /// (app.log only). `done` gets the line to show.
    func check(manual: Bool, done: ((String) -> Void)? = nil) {
        if let v = readyVersion { done?(Self.readyLine(v)); return }
        guard !working else { done?(line); return }
        working = true
        lastCheck = Date()
        if manual { set("Checking…") } else { onChange() }

        let finish: (String, String) -> Void = { [weak self] log, text in
            DispatchQueue.main.async {
                guard let self else { return }
                self.working = false
                AppLog.shared.write("update: \(log)")
                if manual || self.readyVersion != nil { self.set(text) } else { self.onChange() }
                done?(text)
                if self.readyVersion != nil { self.tick() }
            }
        }
        let config = Config.load(from: paths.config)
        guard let urls = Update.feedURLs(base: config.updateFeed) else {
            return finish("bad update_feed in config.json", "Couldn't check for updates.")
        }
        fetch(urls.feed) { [weak self] feed, status in
            guard let self else { return }
            guard status != 404 else { return finish("no feed (404)", "You're up to date.") }
            guard let feed, status == 200 else { return finish("feed: \(status.map(String.init) ?? "no network")", Self.offline) }
            self.fetch(urls.signature) { sig, status in
                guard let sig, status == 200 else {
                    return finish("signature: \(status.map(String.init) ?? "no network")", Self.offline)
                }
                switch Update.decide(feed: feed, signature: sig, current: Self.currentVersion) {
                case .badSignature: finish("the feed's signature is wrong; ignored", "Couldn't check for updates.")
                case .badFeed: finish("the feed is broken; ignored", "Couldn't check for updates.")
                case .upToDate: finish("up to date (\(Self.currentVersion))", "You're up to date.")
                case .noBuildForThisPlatform(let v): finish("\(v) has no Mac build", "You're up to date.")
                case .update(let version, let asset):
                    if manual { DispatchQueue.main.async { self.set("Downloading \(version)…") } }
                    self.download(version: version, asset: asset, finish: finish)
                }
            }
        }
    }

    private static let offline = "Couldn't check. Are you online?"
    static func readyLine(_ v: String) -> String { "\(v) is ready. Restart to update." }

    private func set(_ text: String) {
        line = text
        onChange()
    }

    private func fetch(_ url: URL, _ done: @escaping (Data?, Int?) -> Void) {
        session.dataTask(with: url) { data, response, _ in
            done(data, (response as? HTTPURLResponse)?.statusCode)
        }.resume()
    }

    private func download(version: String, asset: UpdateAsset, finish: @escaping (String, String) -> Void) {
        // The new app is unpacked on the same disk as this one, so the swap is a rename.
        guard let folder = Self.stagingFolder() else {
            return finish("the app's folder can't be changed", "To update, move Context Capsule to Applications.")
        }
        session.downloadTask(with: asset.url) { [weak self] file, response, _ in
            let status = (response as? HTTPURLResponse)?.statusCode
            guard let self, let file, status == 200 else {
                try? FileManager.default.removeItem(at: folder)
                return finish("download: \(status.map(String.init) ?? "no network")", Self.offline)
            }
            let zip = folder.appendingPathComponent("update.zip")
            do { try FileManager.default.moveItem(at: file, to: zip) } catch {
                return finish("download: cannot keep the file", "Couldn't update. Try again later.")
            }
            switch Self.stage(zip: zip, asset: asset, version: version, in: folder) {
            case .failure(let why):
                try? FileManager.default.removeItem(at: folder)
                finish("\(version) refused: \(why.reason)", "Couldn't update: the download didn't check out.")
            case .success(let app):
                DispatchQueue.main.async {
                    self.staged = (app, folder)
                    self.readyVersion = version
                    finish("\(version) downloaded and checked; ready", Self.readyLine(version))
                }
            }
        }.resume()
    }

    struct Refused: Error { let reason: String }

    /// Size + SHA-256, unzip, then the new app must be ours (identifier, version) and signed by the same signer.
    static func stage(zip: URL, asset: UpdateAsset, version: String, in folder: URL) -> Result<URL, Refused> {
        if let bad = Update.checkDownload(zip, against: asset) { return .failure(Refused(reason: bad)) }
        let out = folder.appendingPathComponent("unzipped", isDirectory: true)
        let ditto = Process()
        ditto.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        ditto.arguments = ["-x", "-k", zip.path, out.path]
        do { try ditto.run() } catch { return .failure(Refused(reason: "cannot unzip")) }
        ditto.waitUntilExit()
        try? FileManager.default.removeItem(at: zip)
        guard ditto.terminationStatus == 0 else { return .failure(Refused(reason: "cannot unzip")) }
        let apps = ((try? FileManager.default.contentsOfDirectory(atPath: out.path)) ?? []).filter { $0.hasSuffix(".app") }
        guard apps.count == 1 else { return .failure(Refused(reason: "the zip should hold one app")) }
        let app = out.appendingPathComponent(apps[0], isDirectory: true)
        let info = Bundle(url: app)?.infoDictionary ?? [:]
        guard info["CFBundleIdentifier"] as? String == "com.thekarya.contextcapsule" else {
            return .failure(Refused(reason: "not Context Capsule"))
        }
        guard (info["CFBundleShortVersionString"] as? String).flatMap(SemVer.init) == SemVer(version) else {
            return .failure(Refused(reason: "its version is not \(version)"))
        }
        if let why = Signer.check(app) { return .failure(Refused(reason: why)) }
        return .success(app)
    }

    /// A temporary folder on the app's own disk; nil when the app's folder isn't writable (the disk image, a
    /// locked folder).
    static func stagingFolder() -> URL? {
        let bundle = Bundle.main.bundleURL
        guard FileManager.default.isWritableFile(atPath: bundle.deletingLastPathComponent().path) else { return nil }
        return try? FileManager.default.url(for: .itemReplacementDirectory, in: .userDomainMask,
                                            appropriateFor: bundle, create: true)
    }

    // MARK: installing

    /// Swap the app in place (old aside, new in, old removed after we quit) and open the new one.
    /// Returns a line to show when it can't.
    @discardableResult
    func restartNow() -> String? {
        guard let staged, let version = readyVersion else { return nil }
        if isBusy() { return "Stop contextualizing first, then restart." }
        let fm = FileManager.default
        let bundle = Bundle.main.bundleURL
        let old = bundle.deletingLastPathComponent().appendingPathComponent(".\(bundle.lastPathComponent).old-\(UUID().uuidString.prefix(8))")
        do {
            try fm.moveItem(at: bundle, to: old)
        } catch {
            AppLog.shared.write("update: cannot move the old app aside: \(error.localizedDescription)")
            return "Couldn't update. Try again later."
        }
        do {
            try fm.moveItem(at: staged.app, to: bundle)
        } catch {
            try? fm.moveItem(at: old, to: bundle) // put it back
            AppLog.shared.write("update: cannot move the new app in: \(error.localizedDescription)")
            return "Couldn't update. Try again later."
        }
        AppLog.shared.write("update: installed \(version); restarting")
        Relaunch.after(quitting: bundle, removing: [old, staged.folder])
        NSApp.terminate(nil)
        return nil
    }
}

/// Code signature checks: the new app must be valid and signed like the running one.
enum Signer {
    /// The requirement "same identifier, same team (Developer ID) or the same certificate (a local one)".
    static func requirementForSelf() -> String? {
        var me: SecCode?
        var code: SecStaticCode?
        guard SecCodeCopySelf([], &me) == errSecSuccess, let me,
              SecCodeCopyStaticCode(me, [], &code) == errSecSuccess, let code else { return nil }
        var info: CFDictionary?
        guard SecCodeCopySigningInformation(code, SecCSFlags(rawValue: kSecCSSigningInformation), &info) == errSecSuccess,
              let dict = info as? [String: Any], let id = dict[kSecCodeInfoIdentifier as String] as? String else { return nil }
        let team = dict[kSecCodeInfoTeamIdentifier as String] as? String
        let leaf = (dict[kSecCodeInfoCertificates as String] as? [SecCertificate])?.first.map { cert in
            Insecure.SHA1.hash(data: SecCertificateCopyData(cert) as Data).map { String(format: "%02x", $0) }.joined()
        }
        return Update.requirement(identifier: id, teamID: team, leafCertSHA1: leaf)
    }

    /// nil when `app` is validly signed by the same signer as this app; else why not.
    static func check(_ app: URL) -> String? {
        guard let text = requirementForSelf() else { return "this copy is not signed, so it can't check updates" }
        var req: SecRequirement?
        var code: SecStaticCode?
        guard SecRequirementCreateWithString(text as CFString, [], &req) == errSecSuccess, let req,
              SecStaticCodeCreateWithPath(app as CFURL, [], &code) == errSecSuccess, let code else {
            return "cannot read its signature"
        }
        let flags = SecCSFlags(rawValue: kSecCSCheckAllArchitectures | kSecCSStrictValidate | kSecCSCheckNestedCode)
        let status = SecStaticCodeCheckValidity(code, flags, req)
        return status == errSecSuccess ? nil : "its signature doesn't match (\(status))"
    }
}

/// Opens an app once this one has quit, with a tiny shell script (waits for our pid, removes leftovers, opens).
enum Relaunch {
    static func after(quitting app: URL, removing: [URL]) {
        let script = """
        pid="$1"; app="$2"; shift 2
        i=0; while kill -0 "$pid" 2>/dev/null && [ $i -lt 600 ]; do sleep 0.1; i=$((i+1)); done
        for p in "$@"; do rm -rf "$p"; done
        open "$app"
        """
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/sh")
        p.arguments = ["-c", script, "sh", String(getpid()), app.path] + removing.map(\.path)
        do { try p.run() } catch { AppLog.shared.write("relaunch: \(error.localizedDescription)") }
    }
}

/// Run from the disk image or Downloads? Offer once to move to Applications (where it can update itself).
enum MoveToApplications {
    static let askedKey = "askedMoveToApplications"

    /// true when the app is moving (and quitting to reopen from Applications).
    static func offerIfNeeded() -> Bool {
        let bundle = Bundle.main.bundleURL
        guard bundle.pathExtension == "app",
              !Update.isInApplications(bundle.path, home: NSHomeDirectory()),
              !bundle.path.contains("/capture/build/"), // a build from source
              !UserDefaults.standard.bool(forKey: askedKey) else { return false }
        UserDefaults.standard.set(true, forKey: askedKey)
        let alert = NSAlert()
        alert.messageText = "Move Context Capsule to Applications?"
        alert.informativeText = "It works best from there, and can update itself."
        alert.addButton(withTitle: "Move")
        alert.addButton(withTitle: "Not now")
        NSApp.activate(ignoringOtherApps: true)
        guard alert.runModal() == .alertFirstButtonReturn else { return false }
        return move(bundle)
    }

    private static func move(_ bundle: URL) -> Bool {
        let fm = FileManager.default
        let dest = URL(fileURLWithPath: "/Applications").appendingPathComponent("Context Capsule.app")
        let temp = URL(fileURLWithPath: "/Applications").appendingPathComponent(".Context Capsule.app.new-\(UUID().uuidString.prefix(8))")
        do {
            try fm.copyItem(at: bundle, to: temp)
            if fm.fileExists(atPath: dest.path) { try fm.trashItem(at: dest, resultingItemURL: nil) } // an older copy
            try fm.moveItem(at: temp, to: dest)
        } catch {
            try? fm.removeItem(at: temp)
            AppLog.shared.write("move to Applications failed: \(error.localizedDescription)")
            let alert = NSAlert()
            alert.messageText = "Couldn't move it"
            alert.informativeText = "Drag Context Capsule into your Applications folder yourself."
            alert.runModal()
            return false
        }
        // The copy we ran from goes to the Trash, unless it's on the disk image or another read-only place.
        let path = bundle.path
        if !path.hasPrefix("/Volumes/"), !path.contains("/AppTranslocation/"),
           fm.isWritableFile(atPath: bundle.deletingLastPathComponent().path) {
            try? fm.trashItem(at: bundle, resultingItemURL: nil)
        }
        AppLog.shared.write("moved to Applications")
        Relaunch.after(quitting: dest, removing: [])
        NSApp.terminate(nil)
        return true
    }
}

/// "Context Capsule 0.2.0 is ready. Restart to update." — Later / Restart now. Top right, like the evening reminder.
final class UpdatePrompt {
    private var panel: NSPanel?

    func show(version: String, restart: @escaping () -> Void) {
        close()
        let view = UpdatePromptView(version: version,
                                    restart: { [weak self] in self?.close(); restart() },
                                    later: { [weak self] in self?.close() })
        let host = NSHostingView(rootView: view)
        host.frame = NSRect(origin: .zero, size: host.fittingSize)
        let panel = NSPanel(contentRect: host.frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .transient]
        panel.contentView = host
        if let screen = NSScreen.main {
            let f = screen.visibleFrame
            panel.setFrameOrigin(NSPoint(x: f.maxX - host.frame.width - 14, y: f.maxY - host.frame.height - 10))
        }
        panel.orderFrontRegardless()
        self.panel = panel
    }

    func close() {
        panel?.orderOut(nil)
        panel = nil
    }
}

private struct UpdatePromptView: View {
    let version: String
    let restart: () -> Void
    let later: () -> Void
    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 16, style: .continuous)
        let content = VStack(alignment: .leading, spacing: 12) {
            Text("Context Capsule \(version) is ready. Restart to update.")
                .font(.system(size: 14, weight: .semibold)).fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                Spacer()
                Button("Later", action: later)
                Button("Restart now", action: restart).keyboardShortcut(.defaultAction)
            }
        }
        .padding(16)
        .frame(width: 300)
        if #available(macOS 26, *) {
            content.glassEffect(.regular, in: shape)
        } else {
            content.background(shape.fill(.regularMaterial))
        }
    }
}
