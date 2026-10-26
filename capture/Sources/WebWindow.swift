// context-capsule — capture: the shared window — the same page as Windows and Linux (ui/, copied into
// Contents/Resources/ui) in a WKWebView, served from capsule://app/ (never the network), and the bridge it talks to
// (ui/BRIDGE.md), done with the app's own Swift code. The app's only window. The screen picker, file panels, the share
// menu, the app picker and permission prompts stay native.

import AppKit
import AVFoundation
import Combine
import ServiceManagement
import Speech
import UniformTypeIdentifiers
import WebKit

/// Renders and tests: fixed answers, and nothing done outside the window (no apps, links, panels or prompts).
struct WebHostOptions {
    /// Fixed Set up steps (id → done) and Screen Recording.
    var setup: [String: Bool]?
    var screenRecording: Bool?
    /// A fixed `mcp.run clients` answer.
    var clients: String?
    /// Never open apps, links, panels or system prompts; the clipboard is kept in memory.
    var quiet = false
    /// "glass" / "none" instead of what the Mac says (renders).
    var material: String?
    /// What the host did or would have done (tests).
    var log: (String) -> Void = { _ in }
}

final class WebWindow: NSObject, NSWindowDelegate, WKNavigationDelegate, WKUIDelegate, WKScriptMessageHandlerWithReply {
    static let scheme = "capsule"
    static let pages = ["home", "timeline", "capsules", "projects", "settings", "help"]
    /// Set once the window has opened on first launch (the old welcome screen used the same key).
    static let seenKey = "welcomeSeen"
    static var seen: Bool { UserDefaults.standard.bool(forKey: seenKey) }

    /// Contents/Resources/ui, or CONTEXT_CAPSULE_UI (a folder) when developing.
    static var uiRoot: URL? {
        if let dev = ProcessInfo.processInfo.environment["CONTEXT_CAPSULE_UI"], !dev.isEmpty {
            return URL(fileURLWithPath: dev, isDirectory: true)
        }
        return Bundle.main.resourceURL?.appendingPathComponent("ui", isDirectory: true)
    }

    let app: AppModel
    let paths: Paths
    var options = WebHostOptions()
    /// projects.json was saved: the recorder detects with it.
    var onProjectsChanged: () -> Void = {}
    /// New keys for both shortcuts: nil = done, "taken" = another app has them, else plain words.
    var setShortcuts: (Shortcut, Shortcut) -> String? = { _, _ in nil }
    /// Open on Settings with nothing set up (the first launch).
    var firstRun = false
    /// Automatic capsules (auto.status, auto.run). The app sets its own; renders and tests get a quiet one.
    lazy var auto: AutoRunner = {
        let a = AutoRunner(paths: paths) { false }
        a.quiet = true
        a.fixedAgent = .some(nil)
        return a
    }()

    private(set) var window: NSWindow?
    private(set) var web: WKWebView?
    private var effect: NSVisualEffectView?
    private var initialPage = "home"
    private var helloDone = false
    private var lastState = ""
    private var lastDark: Bool?
    private var lastMaterial = ""
    private var stamps: [String: String] = [:]
    private var lastSessionsEmit = Date.distantPast
    private var watchTimer: Timer?
    private var stateSink: AnyCancellable?
    private var appearanceWatch: NSKeyValueObservation?
    private var transparencyWatch: NSObjectProtocol?
    private let setup = SetupStatus()
    private var clipboard = "" // quiet mode only
    private static var voiceLanguages: [[String: String]]?

    init(app: AppModel, paths: Paths) {
        self.app = app
        self.paths = paths
        super.init()
    }

    // MARK: the window

    /// Show the window (on `page`, or where it is). Open already: the page goes there.
    func show(_ page: String? = nil) {
        NSApp.setActivationPolicy(.regular)
        // While the window is open the app is a normal app, so it needs the usual menus (MainMenu): without them, a
        // full-screen window never shows its close / minimise buttons, and ⌘C / ⌘V don't work in text fields.
        if NSApp.mainMenu?.items.isEmpty ?? true { NSApp.mainMenu = MainMenu.make { [weak self] in self?.show("settings") } }
        if let window {
            if let page { navigate(page) }
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            return
        }
        initialPage = page ?? "home"
        let window = makeWindow(NSWindow.self)
        window.setFrameAutosaveName("ContextCapsuleMain")
        if !window.setFrameUsingName("ContextCapsuleMain") { window.setContentSize(NSSize(width: 860, height: 620)); window.center() }
        window.delegate = self
        self.window = window
        load(URL(string: "capsule://app/index.html")!)
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    /// The tray or the menu asked for a page.
    func navigate(_ page: String) {
        guard Self.pages.contains(page) else { return }
        if helloDone { emit("navigate", ["page": page]) } else { initialPage = page }
    }

    /// A capsule opened with the app (Finder → Open With): the page checks it and shows it to read (Continue in your AI there).
    func openCapsule(_ url: URL) {
        show(nil)
        let text = (try? Data(contentsOf: url)).map { String(decoding: $0, as: UTF8.self) } ?? ""
        emit("open-capsule", ["path": url.path, "text": text])
    }

    /// Files changed that the page shows ("capsules", "projects", "sessions", "config", "clients").
    func dataChanged(_ what: [String]) {
        guard helloDone, !what.isEmpty else { return }
        emit("data-changed", ["what": what])
    }

    /// Automatic capsules changed (a run started or ended, the tools or settings changed): the `auto` event.
    func autoChanged() {
        guard helloDone else { return }
        emit("auto", auto.status())
    }

    /// The window and its web view, not shown yet (shared by show() and the renders).
    func makeWindow<W: NSWindow>(_ type: W.Type) -> W {
        let config = WKWebViewConfiguration()
        config.setURLSchemeHandler(SchemeHandler(), forURLScheme: Self.scheme)
        let content = config.userContentController
        content.addUserScript(WKUserScript(source: Self.shim, injectionTime: .atDocumentStart, forMainFrameOnly: true))
        content.addScriptMessageHandler(WeakHandler(self), contentWorld: .page, name: "capsule")
        config.preferences.isElementFullscreenEnabled = false

        let size = NSRect(x: 0, y: 0, width: 860, height: 620)
        let web = WKWebView(frame: size, configuration: config)
        web.setValue(false, forKey: "drawsBackground") // transparent: the material (or the window colour) shows through
        web.underPageBackgroundColor = .clear
        web.navigationDelegate = self
        web.uiDelegate = self
        web.allowsMagnification = false
        web.allowsBackForwardNavigationGestures = false
        web.autoresizingMask = [.width, .height]
        if #available(macOS 13.3, *) { web.isInspectable = ProcessInfo.processInfo.environment["CONTEXT_CAPSULE_INSPECT"] == "1" }

        // Behind the page: the window's material, as System Settings. The page paints tints over it ("glass"), or
        // solid colours when transparency is reduced ("none").
        let effect = NSVisualEffectView(frame: size)
        effect.material = .sidebar
        effect.blendingMode = .behindWindow
        // Renders draw as the window in front (they can't really be); the app follows the window.
        effect.state = options.material == nil ? .followsWindowActiveState : .active
        effect.autoresizingMask = [.width, .height]
        effect.addSubview(web)

        let window = W(contentRect: size, styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
                       backing: .buffered, defer: false)
        window.title = AppName.full // not drawn; the Window menu and Mission Control use it
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.titlebarSeparatorStyle = .none
        // An empty unified toolbar: the traffic lights sit in the page's 52 pt toolbar band, as in the SwiftUI window.
        // Not in full screen, where macOS draws a toolbar as an opaque band over the page (windowWillEnterFullScreen).
        window.toolbar = Self.emptyToolbar()
        window.toolbarStyle = .unified
        window.contentMinSize = NSSize(width: 760, height: 540)
        window.isReleasedWhenClosed = false
        window.contentView = effect
        self.web = web
        self.effect = effect
        startWatching()
        return window
    }

    /// Renders and tests: use this window (made with makeWindow) and load the page with a query and a page.
    func attach(_ window: NSWindow, query: String = "", page: String? = nil) {
        self.window = window
        if let page { initialPage = page }
        load(URL(string: "capsule://app/index.html" + (query.isEmpty ? "" : "?" + query) + (page.map { "#" + $0 } ?? ""))!)
    }

    private func load(_ url: URL) {
        helloDone = false
        lastState = ""
        web?.load(URLRequest(url: url))
    }

    static func emptyToolbar() -> NSToolbar {
        let toolbar = NSToolbar(identifier: "ContextCapsuleWeb")
        toolbar.displayMode = .iconOnly
        return toolbar
    }

    /// Full screen: no toolbar (macOS would draw it as an opaque band hiding the page's top, the Today pill too);
    /// the page's own band stays. Back in a window, the toolbar returns so the traffic lights sit in that band.
    func windowWillEnterFullScreen(_ notification: Notification) {
        (notification.object as? NSWindow)?.toolbar = nil
    }

    func windowDidExitFullScreen(_ notification: Notification) {
        guard let window = notification.object as? NSWindow, window.toolbar == nil else { return }
        window.toolbar = Self.emptyToolbar()
        window.toolbarStyle = .unified
    }

    func windowWillClose(_ notification: Notification) {
        stopWatching()
        web?.configuration.userContentController.removeAllScriptMessageHandlers()
        web = nil
        effect = nil
        window = nil
        helloDone = false
        NSApp.setActivationPolicy(.accessory) // back to menu bar only
    }

    // MARK: events (host → page)

    func emit(_ event: String, _ payload: Any) {
        guard let web, let json = Self.json(payload) else { return }
        web.callAsyncJavaScript("window.capsuleHost && window.capsuleHost._emit(event, JSON.parse(payload))",
                                arguments: ["event": event, "payload": json], in: nil, in: .page, completionHandler: nil)
    }

    var state: [String: Any] {
        let iso = ISO8601DateFormatter()
        return [
            "recording": app.recording, "picking": app.picking, "listening": app.listening,
            "started_at": app.startedAt.map { iso.string(from: $0) } ?? NSNull(),
            "session_id": app.sessionID ?? NSNull(), "events": app.moments,
            "project": app.projectID ?? NSNull(), "project_by": app.projectBy ?? NSNull(),
            "screens": app.screens, "can_add_screen": app.canAddScreen, "several_screens": app.hasSeveralScreens,
            "update": ["line": app.updateLine, "ready": app.updateReady, "working": app.updateWorking],
        ]
    }

    /// Sends `state` when it changed (the app model changes often; the page gets each new state once).
    func pushState(force: Bool = false) {
        guard helloDone, let json = Self.json(state) else { return }
        if !force && json == lastState { return }
        lastState = json
        emit("state", state)
    }

    var systemDark: Bool { NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua }

    var material: String {
        if let m = options.material { return m }
        return NSWorkspace.shared.accessibilityDisplayShouldReduceTransparency ? "none" : "glass"
    }

    private func startWatching() {
        stopWatching()
        stateSink = app.objectWillChange.sink { [weak self] _ in DispatchQueue.main.async { self?.pushState() } }
        appearanceWatch = NSApp.observe(\.effectiveAppearance, options: [.new]) { [weak self] _, _ in
            DispatchQueue.main.async {
                guard let self, self.helloDone, self.systemDark != self.lastDark else { return }
                self.lastDark = self.systemDark
                self.emit("appearance", ["dark": self.systemDark])
            }
        }
        transparencyWatch = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification, object: nil, queue: .main) { [weak self] _ in
            guard let self, self.helloDone, self.material != self.lastMaterial else { return }
            self.lastMaterial = self.material
            self.emit("material", ["material": self.material])
        }
        _ = changedData() // today's stamps, so only later changes are sent
        watchTimer = repeating(2) { [weak self] in
            guard let self else { return }
            self.dataChanged(self.changedData())
        }
    }

    private func stopWatching() {
        stateSink = nil
        appearanceWatch = nil
        if let t = transparencyWatch { NSWorkspace.shared.notificationCenter.removeObserver(t) }
        transparencyWatch = nil
        watchTimer?.invalidate()
        watchTimer = nil
    }

    /// What changed on disk since the last look (capsules saved by the MCP, projects made, sessions sorted or recorded).
    /// Sessions at most every 4 s, as BRIDGE.md says.
    private func changedData() -> [String] {
        let fm = FileManager.default
        func stamp(_ url: URL) -> String {
            guard let a = try? fm.attributesOfItem(atPath: url.path) else { return "-" }
            return "\((a[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0):\((a[.size] as? NSNumber)?.intValue ?? 0)"
        }
        func folder(_ url: URL, deep: Int = 0) -> String {
            let names = ((try? fm.contentsOfDirectory(atPath: url.path)) ?? []).filter { !$0.hasPrefix(".") }.sorted()
            var parts = names.map { "\($0)=\(stamp(url.appendingPathComponent($0)))" }
            for n in names.suffix(deep) { parts.append(stamp(url.appendingPathComponent(n).appendingPathComponent("events.jsonl"))) }
            return parts.joined(separator: "|")
        }
        let now: [String: String] = [
            "capsules": folder(paths.capsules),
            "projects": stamp(paths.projects),
            "config": stamp(paths.config),
            "sessions": folder(paths.sessions, deep: 3),
            "clients": AIState.shared.clients.map { "\($0.id):\($0.installed):\($0.connected)" }.joined(separator: ","),
        ]
        var changed: [String] = []
        for (k, v) in now where stamps[k] != v {
            if k == "sessions" && Date().timeIntervalSince(lastSessionsEmit) < 4 && stamps[k] != nil { continue }
            if k == "sessions" { lastSessionsEmit = Date() }
            if stamps[k] != nil { changed.append(k) }
            stamps[k] = v
        }
        return changed.sorted()
    }

    /// After the page changed a file itself: it knows, so the watcher doesn't send it back.
    private func settle() { _ = changedData() }

    // MARK: navigation: only the page's own files; links go to the browser

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
                 decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let url = action.request.url else { decisionHandler(.cancel); return }
        if url.scheme == Self.scheme || url.absoluteString == "about:blank" { decisionHandler(.allow); return }
        if action.navigationType == .linkActivated { openLink(url) }
        decisionHandler(.cancel)
    }

    func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration,
                 for action: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
        if let url = action.request.url { openLink(url) }
        return nil
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        AppLog.shared.write("window: the page stopped; loading it again")
        initialPage = "home"
        load(URL(string: "capsule://app/index.html")!)
    }

    private func openLink(_ url: URL) {
        guard ["http", "https"].contains(url.scheme?.lowercased() ?? "") else { return }
        if options.quiet { options.log("open \(url.absoluteString)"); return }
        NSWorkspace.shared.open(url)
    }

    // MARK: calls (page → host)

    struct Refusal: Error { let words: String }
    typealias Done = (Result<Any, Refusal>) -> Void

    func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage,
                               replyHandler: @escaping (Any?, String?) -> Void) {
        guard message.frameInfo.isMainFrame, message.frameInfo.securityOrigin.protocol == Self.scheme,
              let body = message.body as? String,
              let obj = try? JSONSerialization.jsonObject(with: Data(body.utf8)) as? [String: Any],
              let method = obj["method"] as? String else { replyHandler(nil, "That isn't allowed."); return }
        let args = obj["args"] as? [String: Any] ?? [:]
        call(method, args) { result in
            switch result {
            case .success(let value): replyHandler(Self.json(value) ?? "null", nil)
            case .failure(let r): replyHandler(nil, r.words)
            }
        }
    }

    /// One bridge method (ui/BRIDGE.md). `done` is called once, on the main thread.
    func call(_ method: String, _ args: [String: Any], _ done: @escaping Done) {
        let ok: (Any) -> Void = { done(.success($0)) }
        let no: (String) -> Void = { done(.failure(Refusal(words: $0))) }
        let later: (@escaping () -> Void) -> Void = { work in ok([String: Any]()); DispatchQueue.main.async(execute: work) }
        let str: (String) -> String? = { args[$0] as? String }

        switch method {
        case "host.hello":
            Self.languages { languages in
                self.helloDone = true
                self.lastDark = self.systemDark
                self.lastMaterial = self.material
                ok(self.hello(languages))
                DispatchQueue.main.async { self.pushState(force: true) }
            }

        case "data.list":
            guard let dir = str("dir"), let names = BridgeFiles.list(dir, root: paths.root) else { return no("Can't list that folder.") }
            ok(names)

        case "data.read":
            guard let files = args["files"] as? [String], files.count <= 5000 else { return no("files must be a list") }
            if let bad = files.first(where: { !BridgeFiles.allowed($0) }) {
                return no("not allowed: \(bad)")
            }
            let root = paths.root
            DispatchQueue.global(qos: .userInitiated).async {
                var out: [String: Any] = [:]
                for f in files {
                    out[f] = BridgeFiles.url(f, root: root).flatMap { try? Data(contentsOf: $0) }
                        .map { String(decoding: $0, as: UTF8.self) } ?? NSNull()
                }
                DispatchQueue.main.async { ok(out) }
            }

        case "config.set":
            guard let key = str("key"), !key.isEmpty, key.count < 64 else { return no("no key") }
            let value = args["value"] ?? NSNull()
            do { try ConfigFile.merge(key, value, in: paths.config) } catch { return no(error.localizedDescription) }
            AppLog.shared.write("setting changed: \(key)")
            applySetting(key, value)
            settle()
            ok([String: Any]())

        case "projects.save":
            guard let text = str("text"), let data = text.data(using: .utf8),
                  (try? JSONSerialization.jsonObject(with: data)) is [String: Any] else { return no("That isn't valid projects.json.") }
            do { try writeAtomically(text, to: paths.projects) } catch { return no(error.localizedDescription) }
            AppLog.shared.write("projects edited (window)")
            settle()
            onProjectsChanged()
            ok([String: Any]())

        case "timeline.delete":
            guard let session = str("session") else { return no("That isn't a recording.") }
            var lines: [(t: String, line: String)]?
            if let list = args["lines"] {
                guard let list = list as? [[String: Any]] else { return no("lines must be a list") }
                lines = []
                for l in list {
                    guard let t = l["t"] as? String, let line = l["line"] as? String else { return no("lines must be a list") }
                    lines?.append((t, line))
                }
            }
            do {
                try BridgeFiles.delete(session: session, lines: lines, sessions: paths.sessions, state: paths.state)
                AppLog.shared.write(lines == nil ? "timeline: a recording deleted" : "timeline: \(lines?.count ?? 0) moment(s) deleted")
                settle()
                ok([String: Any]())
            } catch Timeline.DeleteError.recording {
                no("recording")
            } catch Timeline.DeleteError.notFound {
                no("gone")
            } catch Timeline.DeleteError.badName {
                no("That isn't a recording.")
            } catch {
                AppLog.shared.write("timeline: cannot delete: \(error.localizedDescription)")
                no(error.localizedDescription)
            }

        case "recorder":
            switch str("action") {
            case "toggle": later { self.app.toggleRecord() }
            case "voice": later { self.app.toggleVoice() }
            case "add-screen": later { self.app.addScreen() }
            case "new-project": later { self.app.newProject() }
            case "project": let id = str("id"); later { self.app.pickProject(id) }
            default: no("unknown action")
            }

        case "shortcuts.set":
            guard let r = str("record").flatMap(Shortcut.parse), let v = str("voice").flatMap(Shortcut.parse) else {
                return no("Those keys can't be used.")
            }
            if let problem = setShortcuts(r, v) { no(problem) } else { ok([String: Any]()) }

        case "shortcuts.listen":
            app.pauseHotKeys(args["on"] as? Bool == true)
            ok([String: Any]())

        case "setup.status":
            ok(setupStatus())

        case "setup.do":
            guard let id = str("id") else { return no("unknown step") }
            if options.quiet { options.log("setup.do \(id)"); return ok([String: Any]()) }
            switch id {
            case "read": later { self.app.askAccessibility() }
            case "voice": later { Setup.askVoice() }
            case "dictation": later { Setup.openSettings(Setup.keyboardPane) }
            case "ai": later { ClaudeConnect.connect() }
            case "login": later { Setup.setLaunchAtLogin(true) }
            case "screen-recording":
                later {
                    _ = CGRequestScreenCaptureAccess()
                    Setup.openSettings(Setup.screenRecordingPane)
                }
            default: no("unknown step")
            }

        case "mcp.run":
            mcp(args, ok, no)

        case "ai.open":
            guard let id = str("id"), let prompt = str("prompt") else { return no("Nothing to open.") }
            setClipboard(prompt)
            if options.quiet { options.log("ai.open \(id)"); return ok(["opened": false]) }
            if id == "claude" {
                guard NSWorkspace.shared.urlForApplication(withBundleIdentifier: ClaudeConnect.desktopBundleID) != nil else {
                    return ok(["opened": false])
                }
                AppLog.shared.write("open Claude with a request")
                NSWorkspace.shared.open(ClaudeLink.newChat(prefill: prompt) ?? ClaudeLink.newChat)
                return ok(["opened": true])
            }
            let client = AIState.shared.client(id) ?? AIClient(id: id, name: id, installed: true, connected: true, how: "config")
            guard let url = AIState.appURL(client) else { return ok(["opened": false]) }
            AppLog.shared.write("open \(id) with a request on the clipboard")
            let config = NSWorkspace.OpenConfiguration()
            config.activates = true
            NSWorkspace.shared.openApplication(at: url, configuration: config) { _, _ in }
            ok(["opened": true])

        case "clipboard.read":
            ok(["text": options.quiet ? clipboard : NSPasteboard.general.string(forType: .string) ?? ""])

        case "clipboard.write":
            setClipboard(str("text") ?? "")
            ok([String: Any]())

        case "open.url":
            guard let s = str("url"), let url = URL(string: s), ["http", "https"].contains(url.scheme?.lowercased() ?? "") else {
                return no("Only web links can be opened.")
            }
            openLink(url)
            ok([String: Any]())

        case "capsule.open":
            guard let file = capsuleFile(str("file")) else { return no("not allowed: \(str("file") ?? "")") }
            guard FileManager.default.fileExists(atPath: file.path) else { return no("gone") }
            if options.quiet { options.log("capsule.open \(file.lastPathComponent)"); return ok([String: Any]()) }
            // In a text editor, never the default app for .capsule.md: that's this app, which would open your AI.
            let ws = NSWorkspace.shared
            let me = Bundle.main.bundleURL.standardizedFileURL
            let editor = ws.urlForApplication(toOpen: UTType.plainText).flatMap { $0.standardizedFileURL == me ? nil : $0 }
                ?? ws.urlForApplication(withBundleIdentifier: "com.apple.TextEdit")
            if let editor {
                ws.open([file], withApplicationAt: editor, configuration: NSWorkspace.OpenConfiguration())
            } else {
                ws.activateFileViewerSelecting([file])
            }
            ok([String: Any]())

        case "capsule.share":
            share(args, ok, no)

        case "capsule.pick":
            if options.quiet { options.log("capsule.pick"); return ok(NSNull()) }
            let panel = NSOpenPanel()
            panel.allowedContentTypes = [UTType(filenameExtension: "md") ?? .plainText]
            panel.allowsMultipleSelection = false
            panel.prompt = "Open"
            panel.message = "Pick a capsule someone sent you."
            panel.directoryURL = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
            runPanel(panel) { url in
                guard let url, let data = try? Data(contentsOf: url) else { return ok(NSNull()) }
                ok(["path": url.path, "text": String(decoding: data, as: UTF8.self)])
            }

        case "apps.pick":
            if options.quiet { options.log("apps.pick"); return ok(NSNull()) }
            let panel = NSOpenPanel()
            panel.directoryURL = URL(fileURLWithPath: "/Applications")
            panel.allowedContentTypes = [.application]
            panel.prompt = "Never record"
            runPanel(panel) { url in
                guard let url, let id = Bundle(url: url)?.bundleIdentifier else { return ok(NSNull()) }
                ok(["id": id, "name": FileManager.default.displayName(atPath: url.path).replacingOccurrences(of: ".app", with: "")])
            }

        case "apps.names":
            let ids = (args["ids"] as? [String] ?? []).prefix(500)
            var out: [String: String] = [:]
            for id in ids { out[id] = Preferences.appName(id) }
            ok(out)

        case "apps.icons":
            let apps = args["apps"] as? [[String: Any]] ?? []
            DispatchQueue.global(qos: .userInitiated).async {
                let out = AppIconPNG.answer(apps)
                DispatchQueue.main.async { ok(out) }
            }

        case "auto.status":
            ok(auto.status())

        case "auto.run":
            if let why = auto.runNow() { return no(why) }
            ok(["started": true])

        case "updates":
            switch str("action") {
            case "check": later { self.app.checkForUpdates() }
            case "restart": later { self.app.restartToUpdate() }
            default: no("unknown action")
            }

        default:
            no("unknown method \(method)")
        }
    }

    private func hello(_ languages: [[String: String]]) -> [String: Any] {
        var hour12: Any = NSNull()
        if #available(macOS 13, *) {
            switch Locale.current.hourCycle {
            case .zeroToEleven, .oneToTwelve: hour12 = true
            case .zeroToTwentyThree, .oneToTwentyFour: hour12 = false
            @unknown default: break
            }
        }
        let locale: String
        if #available(macOS 13, *) { locale = Locale.current.identifier(.bcp47) } else {
            locale = Locale.current.identifier.replacingOccurrences(of: "_", with: "-")
        }
        return [
            "platform": "mac", "version": Updater.currentVersion, "locale": locale, "hour12": hour12,
            "firstWeekday": Calendar.current.firstWeekday, "userName": NSFullUserName(), "dark": systemDark,
            "material": material, "firstRun": firstRun, "page": initialPage,
            "voiceLanguages": languages, "voiceLanguageDefault": "Same as my Mac",
        ]
    }

    /// Only languages this Mac can turn into text by itself (nothing is sent to Apple). Worked out once, off the main thread.
    private static func languages(_ done: @escaping ([[String: String]]) -> Void) {
        if let l = voiceLanguages { return done(l) }
        DispatchQueue.global(qos: .userInitiated).async {
            let list = SFSpeechRecognizer.supportedLocales()
                .filter { SFSpeechRecognizer(locale: $0)?.supportsOnDeviceRecognition == true }
                .map { ["id": $0.identifier, "name": Locale.current.localizedString(forIdentifier: $0.identifier) ?? $0.identifier] }
                .sorted { ($0["name"] ?? "") < ($1["name"] ?? "") }
            DispatchQueue.main.async {
                voiceLanguages = list
                done(list)
            }
        }
    }

    /// What must happen at once when a setting changes (the rest is read from config.json when it's used).
    private func applySetting(_ key: String, _ value: Any) {
        switch key {
        case "appearance":
            let a = value as? String ?? "system"
            Preferences.apply(["light", "dark"].contains(a) ? a : "system")
        case "share_screens":
            if value as? String == "all" && !CGPreflightScreenCaptureAccess() {
                if options.quiet { options.log("ask Screen Recording") } else { _ = CGRequestScreenCaptureAccess() }
            }
        case "ai":
            AIState.shared.dayLine = nil
            AIState.shared.updateChoice()
            auto.settingChanged() // Your AI picks which tool a run uses
        case "make_capsules", "remind_at":
            auto.settingChanged()
        case "author":
            app.name = Preferences.name(from: paths.config).split(separator: " ").first.map(String.init) ?? ""
        default: break
        }
    }

    private func setupStatus() -> [String: Any] {
        if let fixed = options.setup {
            let steps = ["read", "voice", "dictation", "ai", "login"].map { ["id": $0, "done": fixed[$0] ?? false] as [String: Any] }
            return ["steps": steps, "screenRecording": options.screenRecording ?? true]
        }
        setup.refresh() // asks the AI apps every 10th time, as the old window did
        let steps: [(String, Bool)] = [("read", setup.readsText), ("voice", setup.microphone && setup.speech),
                                       ("dictation", setup.dictation), ("ai", setup.ai), ("login", setup.atLogin)]
        return ["steps": steps.map { ["id": $0.0, "done": $0.1] as [String: Any] },
                "screenRecording": options.screenRecording ?? CGPreflightScreenCaptureAccess()]
    }

    private func mcp(_ args: [String: Any], _ ok: @escaping (Any) -> Void, _ no: @escaping (String) -> Void) {
        let safe = { (s: String) in s.range(of: #"^[A-Za-z0-9._-]{1,64}$"#, options: .regularExpression) != nil }
        guard let command = args["command"] as? String, ["clients", "connect", "disconnect", "day", "save"].contains(command) else {
            return no("unknown command")
        }
        if command == "clients", let fixed = options.clients { return ok(fixed) }
        var argv = [command]
        if let id = args["id"] as? String {
            guard safe(id) else { return no("unknown app") }
            argv.append(id)
        }
        if let date = args["date"] as? String {
            guard date.range(of: #"^\d{4}-\d{2}-\d{2}$"#, options: .regularExpression) != nil else { return no("bad date") }
            argv += ["--date", date]
        }
        let input = (args["input"] as? String).map { Data($0.utf8) }
        let timeout: TimeInterval = ["clients": 10, "day": 60, "save": 30][command] ?? 20
        AIBridge.run(argv, input: input, timeout: timeout) { r in
            switch r {
            case .success(let out):
                if command == "save" || command == "connect" || command == "disconnect" { AIState.shared.refresh() }
                ok(String(decoding: out, as: UTF8.self))
            case .failure(.missing): no("missing")
            case .failure(.timeout): no("timeout")
            case .failure(let f): no(f.message)
            }
        }
    }

    private func capsuleFile(_ file: String?) -> URL? {
        guard let file, file.hasPrefix("capsules/") else { return nil }
        return BridgeFiles.url(file, root: paths.root)
    }

    private func setClipboard(_ text: String) {
        if options.quiet { clipboard = text; return }
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(text, forType: .string)
    }

    /// capsule.share: the file itself, or (with `text`, Check before sharing already done in the page) a copy with that text
    /// and the same name; copied as a file or as text, saved with the save panel, shown in Finder, or the share menu.
    private func share(_ args: [String: Any], _ ok: @escaping (Any) -> Void, _ no: @escaping (String) -> Void) {
        guard let original = capsuleFile(args["file"] as? String) else { return no("not allowed: \(args["file"] as? String ?? "")") }
        guard FileManager.default.fileExists(atPath: original.path) else { return no("gone") }
        let how = args["how"] as? String ?? ""
        var file = original
        if let text = args["text"] as? String, how != "reveal" {
            let fm = FileManager.default
            let base = fm.temporaryDirectory.appendingPathComponent("context-capsule-share", isDirectory: true)
            for old in (try? fm.contentsOfDirectory(at: base, includingPropertiesForKeys: [.creationDateKey])) ?? [] {
                if let made = try? old.resourceValues(forKeys: [.creationDateKey]).creationDate, made < Date().addingTimeInterval(-86_400) {
                    try? fm.removeItem(at: old)
                }
            }
            let copy = base.appendingPathComponent(UUID().uuidString, isDirectory: true).appendingPathComponent(original.lastPathComponent)
            do { try writeAtomically(text, to: copy) } catch { return no(error.localizedDescription) }
            AppLog.shared.write("share: a checked copy")
            file = copy
        }
        if options.quiet && how != "copy-text" { options.log("share \(how)"); return ok(["done": true]) }
        switch how {
        case "copy-file":
            let pb = NSPasteboard.general
            pb.clearContents()
            pb.writeObjects([file as NSURL])
            ok(["done": true])
        case "copy-text":
            guard let data = try? Data(contentsOf: file) else { return no("gone") }
            setClipboard(String(decoding: data, as: UTF8.self))
            ok(["done": true])
        case "reveal":
            NSWorkspace.shared.activateFileViewerSelecting([original])
            ok(["done": true])
        case "save-copy":
            let panel = NSSavePanel()
            panel.nameFieldStringValue = original.lastPathComponent
            panel.directoryURL = FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask).first
            panel.canCreateDirectories = true
            runPanel(panel) { dest in
                guard let dest else { return ok(["done": false]) }
                try? FileManager.default.removeItem(at: dest)
                do {
                    try FileManager.default.copyItem(at: file, to: dest)
                    NSWorkspace.shared.activateFileViewerSelecting([dest])
                    ok(["done": true, "where": dest.deletingLastPathComponent().lastPathComponent])
                } catch {
                    no(error.localizedDescription)
                }
            }
        case "more":
            // AirDrop, Mail, Messages…: the system share menu, where the mouse is.
            guard let window, let view = window.contentView else { return no("The window isn't open.") }
            let point = view.convert(window.mouseLocationOutsideOfEventStream, from: nil)
            NSSharingServicePicker(items: [file]).show(relativeTo: NSRect(x: point.x, y: point.y, width: 1, height: 1),
                                                       of: view, preferredEdge: .minY)
            ok(["done": true])
        default:
            no("unknown way to share")
        }
    }

    /// A file panel as a sheet on the window (on its own when the window isn't open).
    private func runPanel(_ panel: NSSavePanel, _ done: @escaping (URL?) -> Void) {
        NSApp.activate(ignoringOtherApps: true)
        if let window, window.isVisible, window.attachedSheet == nil {
            panel.beginSheetModal(for: window) { r in done(r == .OK ? panel.url : nil) }
        } else {
            done(panel.runModal() == .OK ? panel.url : nil)
        }
    }

    static func json(_ value: Any) -> String? {
        guard let data = try? JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed]) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    // MARK: the bridge script and the page's files

    /// window.capsuleHost (ui/BRIDGE.md), before the page loads. Calls go to the "capsule" handler as JSON and come back
    /// as JSON; events come in through _emit. `state`, `appearance` and `material` are replayed to late listeners; other
    /// events sent before anyone listens wait for the first listener.
    static let shim = #"""
    (() => {
      if (window.capsuleHost) return;
      const listeners = new Map(), last = new Map(), pending = [];
      const sticky = (e) => e === "state" || e === "appearance" || e === "material";
      window.capsuleHost = Object.freeze({
        call(method, args) {
          return window.webkit.messageHandlers.capsule.postMessage(JSON.stringify({ method, args: args ?? {} })).then(
            (json) => (json == null ? null : JSON.parse(json)),
            (e) => { throw new Error(e && e.message ? e.message : String(e)); });
        },
        on(event, fn) {
          if (!listeners.has(event)) listeners.set(event, new Set());
          listeners.get(event).add(fn);
          if (last.has(event)) queueMicrotask(() => fn(last.get(event)));
          for (let i = 0; i < pending.length; ) {
            if (pending[i][0] === event) { const p = pending.splice(i, 1)[0][1]; queueMicrotask(() => fn(p)); } else i++;
          }
          return () => listeners.get(event)?.delete(fn);
        },
        _emit(event, payload) {
          if (sticky(event)) last.set(event, payload);
          const fns = listeners.get(event);
          if (!fns || !fns.size) { if (!sticky(event) && pending.length < 50) pending.push([event, payload]); return; }
          for (const fn of [...fns]) { try { fn(payload); } catch (e) { console.error("capsuleHost " + event, e); } }
        },
      });
      // No Reload / Inspect menu: right-click does what the page says, or the usual menu in text.
      window.addEventListener("contextmenu", (e) => {
        if (e.defaultPrevented) return;
        const t = e.target;
        if (t && t.closest && t.closest("input, textarea, [contenteditable], .selectable")) return;
        e.preventDefault();
      });
    })();
    """#

    /// capsule://app/<path>: the page's files from the app (never the network).
    final class SchemeHandler: NSObject, WKURLSchemeHandler {
        func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
            guard let url = task.request.url, url.host == "app", let root = WebWindow.uiRoot,
                  let file = BridgeFiles.uiFile(root: root, path: url.path), let data = try? Data(contentsOf: file) else {
                let r = HTTPURLResponse(url: task.request.url ?? URL(string: "capsule://app/")!, statusCode: 404, httpVersion: "HTTP/1.1",
                                        headerFields: ["Content-Type": "text/plain"])!
                task.didReceive(r)
                task.didReceive(Data("not found".utf8))
                task.didFinish()
                return
            }
            let r = HTTPURLResponse(url: url, statusCode: 200, httpVersion: "HTTP/1.1",
                                    headerFields: ["Content-Type": BridgeFiles.mime(file.pathExtension),
                                                   "Content-Length": "\(data.count)", "Cache-Control": "no-store"])!
            task.didReceive(r)
            task.didReceive(data)
            task.didFinish()
        }

        func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {}
    }

    /// The content controller keeps its handler; this keeps the window from keeping itself.
    private final class WeakHandler: NSObject, WKScriptMessageHandlerWithReply {
        weak var target: WKScriptMessageHandlerWithReply?
        init(_ target: WKScriptMessageHandlerWithReply) { self.target = target }
        func userContentController(_ c: WKUserContentController, didReceive m: WKScriptMessage,
                                   replyHandler: @escaping (Any?, String?) -> Void) {
            guard let target else { replyHandler(nil, "The window is closed."); return }
            target.userContentController(c, didReceive: m, replyHandler: replyHandler)
        }
    }
}

/// The menu bar while the window is open: Karya Context Capsule, Edit, View, Window.
enum MainMenu {
    private final class Target: NSObject {
        let settings: () -> Void
        init(_ settings: @escaping () -> Void) { self.settings = settings }
        @objc func openSettings() { settings() }
    }
    private static var target: Target?

    static func make(settings: @escaping () -> Void) -> NSMenu {
        let target = Target(settings)
        Self.target = target
        let main = NSMenu()
        func submenu(_ title: String, _ items: [NSMenuItem]) {
            let menu = NSMenu(title: title)
            items.forEach(menu.addItem)
            let holder = NSMenuItem(title: title, action: nil, keyEquivalent: "")
            holder.submenu = menu
            main.addItem(holder)
        }
        func item(_ title: String, _ action: Selector?, _ key: String, _ mods: NSEvent.ModifierFlags = .command) -> NSMenuItem {
            let i = NSMenuItem(title: title, action: action, keyEquivalent: key)
            i.keyEquivalentModifierMask = mods
            return i
        }
        let settingsItem = item("Settings…", #selector(Target.openSettings), ",")
        settingsItem.target = target
        submenu(AppName.full, [
            item("About \(AppName.full)", #selector(NSApplication.orderFrontStandardAboutPanel(_:)), ""),
            .separator(),
            settingsItem,
            .separator(),
            item("Hide \(AppName.full)", #selector(NSApplication.hide(_:)), "h"),
            .separator(),
            item("Quit \(AppName.full)", #selector(NSApplication.terminate(_:)), "q"),
        ])
        submenu("Edit", [
            item("Undo", Selector(("undo:")), "z"),
            item("Redo", Selector(("redo:")), "z", [.command, .shift]),
            .separator(),
            item("Cut", #selector(NSText.cut(_:)), "x"),
            item("Copy", #selector(NSText.copy(_:)), "c"),
            item("Paste", #selector(NSText.paste(_:)), "v"),
            item("Select All", #selector(NSText.selectAll(_:)), "a"),
        ])
        submenu("View", [
            item("Enter Full Screen", #selector(NSWindow.toggleFullScreen(_:)), "f", [.command, .control]),
        ])
        let windowMenu = NSMenu(title: "Window")
        [item("Minimize", #selector(NSWindow.performMiniaturize(_:)), "m"),
         item("Zoom", #selector(NSWindow.performZoom(_:)), ""),
         .separator(),
         item("Close", #selector(NSWindow.performClose(_:)), "w")].forEach(windowMenu.addItem)
        let windowHolder = NSMenuItem(title: "Window", action: nil, keyEquivalent: "")
        windowHolder.submenu = windowMenu
        main.addItem(windowHolder)
        NSApp.windowsMenu = windowMenu
        return main
    }
}
