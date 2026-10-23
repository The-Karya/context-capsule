// context-capsule — capture: the shared window, checked — `--render-web-page` (the real web window drawn offscreen into a
// PNG, like --render-page) and `--test-web-window` (drives the page through the real bridge with evaluateJavaScript:
// every page, search, a recording opened, a read deleted, settings changed, events sent). Both use the data in
// CONTEXT_CAPSULE_HOME; the test refuses to run on ~/.context-capsule. Nothing is recorded; no app, link or panel opens.

import AppKit
import WebKit

extension WebWindow {
    static let renderPages = MainWindow.renderPages
    /// The page's clock in renders and tests, so they match ui/dev/render-all.sh.
    static let renderNow = "2026-10-23T19:00:00"

    static func spin(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }

    static func spin(until ok: () -> Bool, timeout: Double) -> Bool {
        let end = Date().addingTimeInterval(timeout)
        while !ok() {
            if Date() > end { return false }
            RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        }
        return true
    }

    /// Runs a function body in the page (it may await) and gives what it returns, as a string ("error: …" on failure).
    static func js(_ web: WKWebView, _ body: String, timeout: Double = 15) -> String {
        var out: String?
        web.callAsyncJavaScript(body, arguments: [:], in: nil, in: .page) { r in
            switch r {
            case .success(let v): out = v is NSNull ? "null" : "\(v)"
            case .failure(let e): out = "error: \(e.localizedDescription) \((e as NSError).userInfo["WKJavaScriptExceptionMessage"] ?? "")"
            }
        }
        _ = spin(until: { out != nil }, timeout: timeout)
        return out ?? "error: timeout"
    }

    /// A desktop picture behind the window, for glass renders (the blur needs something behind it).
    private static func backdrop(_ frame: NSRect) -> NSWindow? {
        let env = ProcessInfo.processInfo.environment["CONTEXT_CAPSULE_BACKDROP"]
        let candidates = [env, "/System/Library/Desktop Pictures/.thumbnails/Big Sur Graphic.heic",
                          "/System/Library/Desktop Pictures/.thumbnails/Big Sur Graphic Light.heic"].compactMap { $0 }
        guard let image = candidates.lazy.compactMap({ NSImage(contentsOfFile: $0) }).first else { return nil }
        let w = MainWindow.OffscreenWindow(contentRect: frame.insetBy(dx: -60, dy: -60), styleMask: [.borderless],
                                           backing: .buffered, defer: false)
        let view = NSImageView(frame: NSRect(origin: .zero, size: w.frame.size))
        view.image = image
        view.imageScaling = .scaleAxesIndependently
        w.contentView = view
        return w
    }

    /// The window with what is under it (the backdrop), as the window server draws them.
    private static func image(of windows: [NSWindow]) -> CGImage? {
        typealias Fn = @convention(c) (CGRect, CFArray, UInt32) -> Unmanaged<CGImage>?
        guard let sym = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "CGWindowListCreateImageFromArray"),
              let front = windows.last, let main = NSScreen.screens.first else { return nil }
        let fn = unsafeBitCast(sym, to: Fn.self)
        // The front window's rect, in CG coordinates (origin at the top left of the main screen).
        let f = front.frame
        let rect = CGRect(x: f.minX, y: main.frame.maxY - f.maxY, width: f.width, height: f.height)
        // The screen as shown there (the window server's blur is only in what the display shows); both windows float
        // above everything else meanwhile.
        typealias ScreenFn = @convention(c) (CGRect, UInt32, UInt32, UInt32) -> Unmanaged<CGImage>?
        if let s = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "CGWindowListCreateImage"),
           let shot = unsafeBitCast(s, to: ScreenFn.self)(rect, CGWindowListOption.optionOnScreenOnly.rawValue, 0,
                                                          CGWindowImageOption([.bestResolution]).rawValue)?.takeRetainedValue() {
            return shot
        }
        // Front to back, as CGWindowList wants.
        // The array holds the window numbers themselves (not CFNumbers), as CGWindowListCreateImageFromArray wants.
        var raw: [UnsafeRawPointer?] = windows.reversed().map { UnsafeRawPointer(bitPattern: UInt($0.windowNumber)) }
        guard let ids = CFArrayCreate(nil, &raw, raw.count, nil) else { return nil }
        return fn(rect, ids, CGWindowImageOption([.bestResolution]).rawValue)?.takeRetainedValue()
    }

    /// --render-web-page: one page of the shared window, drawn offscreen at 2× with the data in CONTEXT_CAPSULE_HOME.
    /// Same pages and set-up as --render-page ("welcome" = first launch, nothing set up; no AI apps installed).
    /// material "glass" puts a desktop picture behind the window.
    static func renderPage(_ page: String, to out: URL, dark: Bool, size: NSSize, material: String) -> Bool {
        let paths = Paths.standard
        let model = AppModel()
        model.version = Updater.currentVersion
        NSApp.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        let w = WebWindow(app: model, paths: paths)
        let done = page != "welcome"
        w.options = WebHostOptions(setup: ["read": done, "voice": done, "dictation": done, "ai": false, "login": done],
                                   screenRecording: true,
                                   clients: #"[{"id":"claude","name":"Claude","installed":false,"connected":false,"how":"extension"},{"id":"cursor","name":"Cursor","installed":false,"connected":false,"how":"config"}]"#,
                                   quiet: true, material: material)
        w.firstRun = page == "welcome"
        let hash: String
        switch page {
        case "timeline-open": hash = "timeline"
        case "welcome": hash = "settings"
        default: hash = page
        }
        let window = w.makeWindow(MainWindow.OffscreenWindow.self)
        window.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        window.colorSpace = .sRGB // the page's colours as they are, like the other renders
        window.setContentSize(size)
        window.setFrameOrigin(NSPoint(x: -30_000, y: -30_000))
        var below: NSWindow?
        if material == "glass", let b = backdrop(window.frame) {
            // On screen: the window server only blurs what is really behind a window that is shown.
            let screen = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
            window.setFrameOrigin(NSPoint(x: screen.minX + 80, y: screen.maxY - window.frame.height - 80))
            b.setFrame(window.frame.insetBy(dx: -60, dy: -60), display: true)
            b.level = .floating
            window.level = .floating
            b.orderFront(nil)
            below = b
        }
        w.attach(window, query: "now=\(renderNow)" + (page == "timeline-open" ? "&open=first" : ""), page: hash)
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        guard let web = w.web else { return false }
        let ready = spin(until: { js(web, "return document.documentElement.dataset.ready === '1'") == "1" }, timeout: 15)
        if !ready { FileHandle.standardError.write("render: the page never said it was ready\n".data(using: .utf8)!) }
        spin(0.6)
        let shot = below.map { image(of: [$0, window]) } ?? MainWindow.windowImage(window)
        window.orderOut(nil)
        below?.orderOut(nil)
        guard let shot, let png = NSBitmapImageRep(cgImage: shot).representation(using: .png, properties: [:]) else { return false }
        return (try? png.write(to: out)) != nil && ready
    }
}

/// --test-web-window: the shared window driven through its real bridge, on a copy of the data (CONTEXT_CAPSULE_HOME).
/// The recorder, shortcuts and updates are stand-ins that note what was asked; apps, links and panels are never opened.
enum WebCheck {
    static func run() -> Bool {
        let paths = Paths.standard
        let real = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".context-capsule").standardizedFileURL.path
        guard ProcessInfo.processInfo.environment["CONTEXT_CAPSULE_HOME"] != nil, paths.root.standardizedFileURL.path != real else {
            print("refusing: set CONTEXT_CAPSULE_HOME to a copy of some data (never ~/.context-capsule)")
            return false
        }
        var passes = 0, failures = 0
        func check(_ ok: Bool, _ name: String, _ detail: @autoclosure () -> String = "") {
            if ok { passes += 1; print("ok    \(name)") } else { failures += 1; print("FAIL  \(name) \(detail())") }
        }
        let fm = FileManager.default
        func text(_ url: URL) -> String { (try? String(contentsOf: url, encoding: .utf8)) ?? "" }
        func config() -> [String: Any] {
            (try? JSONSerialization.jsonObject(with: Data(contentsOf: paths.config))) as? [String: Any] ?? [:]
        }

        var asked: [String] = []
        let model = AppModel()
        model.version = Updater.currentVersion
        model.toggleRecord = { asked.append("toggle") }
        model.toggleVoice = { asked.append("voice") }
        model.addScreen = { asked.append("add-screen") }
        model.newProject = { asked.append("new-project") }
        model.pickProject = { asked.append("project:\($0 ?? "auto")") }
        model.pauseHotKeys = { asked.append("pause:\($0)") }
        model.checkForUpdates = { asked.append("check-updates") }
        model.restartToUpdate = { asked.append("restart") }
        model.askAccessibility = { asked.append("accessibility") }
        NSApp.appearance = NSAppearance(named: .aqua)

        let w = WebWindow(app: model, paths: paths)
        var log: [String] = []
        w.options = WebHostOptions(quiet: true, log: { log.append($0) })
        var projectsChanged = 0
        w.onProjectsChanged = { projectsChanged += 1 }
        w.setShortcuts = { r, v in
            asked.append("keys:\(r.text),\(v.text)")
            try? ConfigFile.set("hotkeys", ["record": r.text, "voice": v.text], in: paths.config)
            return nil
        }
        let window = w.makeWindow(MainWindow.OffscreenWindow.self)
        window.setContentSize(NSSize(width: 860, height: 620))
        window.setFrameOrigin(NSPoint(x: -30_000, y: -30_000))
        w.attach(window, query: "now=\(WebWindow.renderNow)", page: "home")
        window.makeKeyAndOrderFront(nil)
        guard let web = w.web else { return false }
        func js(_ body: String) -> String { WebWindow.js(web, body) }
        func wait(_ body: String, _ timeout: Double = 5) -> Bool {
            WebWindow.spin(until: { js("return !!(\(body))") == "1" }, timeout: timeout)
        }
        func call(_ method: String, _ args: String = "{}") -> String {
            js("try { return JSON.stringify(await capsuleHost.call('\(method)', \(args))) } catch (e) { return 'refused: ' + e.message }")
        }

        // Starting up
        check(wait("document.documentElement.dataset.ready === '1'", 15), "the page loads from capsule://app and says it's ready")
        check(js("return location.protocol") == "capsule:", "served from capsule://")
        check(js("return document.querySelector('.side-row[aria-current=page]')?.dataset.page") == "home", "opens on Home")
        check(js("return document.getElementById('page').innerText").contains("Saachi"), "Home greets by the name in config.json")
        check(js("return document.documentElement.dataset.material || 'none'") == w.material, "material from host.hello", w.material)
        let hello = call("host.hello")
        check(hello.contains("\"platform\":\"mac\"") && hello.contains("\"voiceLanguageDefault\":\"Same as my Mac\""), "host.hello", hello)

        // Pages, through the navigate event (as the tray and the menu do)
        for page in ["timeline", "capsules", "projects", "settings", "help", "home"] {
            w.navigate(page)
            check(wait("document.querySelector('.side-row[aria-current=page]')?.dataset.page === '\(page)' && document.getElementById('page').innerText.length > 20"),
                  "navigate: \(page)")
        }
        // The sidebar, clicked
        _ = js("document.querySelector('.side-row[data-page=projects]').click(); return 1")
        check(wait("location.hash === '#projects'"), "sidebar click: Projects")

        // Reading files
        check(call("data.read", "{files: ['config.json']}") == WebWindow.json(["config.json": text(paths.config)]), "data.read: config.json as on disk")
        let sessions = BridgeFiles.list("sessions", root: paths.root) ?? []
        check(call("data.list", "{dir: 'sessions'}") == WebWindow.json(sessions), "data.list: sessions")
        check(call("data.read", "{files: ['capsules/nope.md']}") == #"{"capsules/nope.md":null}"#, "data.read: missing = null")
        for bad in ["../config.json", "/etc/passwd", "sessions/../config.json", "app.log", "capsules/../config.json", "control"] {
            check(call("data.read", "{files: ['\(bad)']}").hasPrefix("refused: not allowed"), "data.read refuses \(bad)")
        }
        check(call("data.list", "{dir: '..'}").hasPrefix("refused"), "data.list refuses other folders")

        // Capsules: search
        w.navigate("capsules")
        _ = wait("document.querySelectorAll('.cap-row').length > 0")
        let all = Int(js("return document.querySelectorAll('.cap-row').length")) ?? 0
        let firstTitle = js("return (document.querySelector('.cap-row .s') || document.querySelector('.cap-row b')).textContent.split(/\\s+/).filter((w) => w.length > 4)[0] || ''")
        check(all > 0, "capsules listed", "\(all)")
        let word = firstTitle.split(separator: " ").first.map(String.init) ?? "a"
        _ = js("const f = document.querySelector('#page input[type=search]'); f.value = \(WebWindow.json(word) ?? "''"); f.dispatchEvent(new Event('input')); return 1")
        _ = wait("document.querySelectorAll('.cap-row').length <= \(all)")
        let found = Int(js("return document.querySelectorAll('.cap-row').length")) ?? 0
        check(found >= 1 && found <= all, "capsules search: \"\(word)\" finds \(found) of \(all)")
        _ = js("const f = document.querySelector('#page input[type=search]'); f.value = 'zzqqxx'; f.dispatchEvent(new Event('input')); return 1")
        check(wait("document.getElementById('page').innerText.includes('No titles match')"), "capsules search: nothing found says so")

        // Timeline: open a recording, search, delete a read (on this copy of the data)
        w.navigate("timeline")
        check(wait("document.querySelectorAll('.rec .disc').length > 0", 8), "timeline: recordings shown")
        _ = js("document.querySelector('.rec .disc').click(); return 1")
        check(wait("document.querySelector('.rec .disc')?.getAttribute('aria-expanded') === 'true'"), "timeline: open a recording")
        _ = js("document.querySelector('.stretch .reads .link')?.click(); return 1")
        check(wait("document.querySelectorAll('.read-row').length > 0"), "timeline: show what was read")
        let session = js("return document.querySelector('.rec')?.dataset.key.slice(2)")
        let events = paths.sessions.appendingPathComponent(session).appendingPathComponent("events.jsonl")
        let before = text(events).split(separator: "\n").count
        _ = js("document.querySelector('.read-row button[aria-haspopup=menu]').click(); return 1")
        _ = wait("document.querySelector('[role=menuitem]')")
        _ = js("document.querySelector('[role=menuitem]').click(); return 1")
        check(wait("document.querySelector('[role=alertdialog] .danger')"), "timeline: delete asks first")
        _ = js("document.querySelector('[role=alertdialog] .danger').click(); return 1")
        check(WebWindow.spin(until: { text(events).split(separator: "\n").count == before - 1 }, timeout: 5),
              "timeline: a read deleted from \(session)/events.jsonl", "\(before) → \(text(events).split(separator: "\n").count)")
        check(js("return document.querySelector('[role=alertdialog]') ? 'dialog' : 'none'") == "none", "timeline: no error after deleting")
        _ = js("const f = document.querySelector('#page input[type=search]'); f.value = 'zzqqxx'; f.dispatchEvent(new Event('input')); return 1")
        check(wait("document.querySelectorAll('.read-row, .stretch').length === 0"), "timeline: search with no match hides everything")
        _ = js("const f = document.querySelector('#page input[type=search]'); f.value = ''; f.dispatchEvent(new Event('input')); return 1")

        // timeline.delete directly: refused while recording, a whole recording, gone
        if let other = sessions.last(where: { $0 != session }) {
            let saved = text(paths.state)
            try? #"{"recording":true,"session_id":"\#(other)"}"#.write(to: paths.state, atomically: true, encoding: .utf8)
            check(call("timeline.delete", "{session: '\(other)'}") == "refused: recording", "timeline.delete: refused while recording")
            try? saved.write(to: paths.state, atomically: true, encoding: .utf8)
            if saved.isEmpty { try? fm.removeItem(at: paths.state) }
            check(call("timeline.delete", "{session: '\(other)'}") == "{}", "timeline.delete: a whole recording")
            check(!fm.fileExists(atPath: paths.sessions.appendingPathComponent(other).path), "timeline.delete: its folder is gone")
            check(call("timeline.delete", "{session: '\(other)'}") == "refused: gone", "timeline.delete: twice says gone")
        }
        check(call("timeline.delete", "{session: '../x'}").hasPrefix("refused"), "timeline.delete: refuses a bad name")
        check(call("timeline.delete", "{session: '\(session)', lines: [{t: '2000-01-01T00:00:00Z', line: '{}'}]}") == "refused: gone",
              "timeline.delete: a line that isn't there")

        // Settings: a switch, the name, appearance
        w.navigate("settings")
        _ = wait("document.querySelector('button[role=switch][aria-label=Sounds]')")
        _ = js("document.querySelector('button[role=switch][aria-label=Sounds]').click(); return 1")
        check(WebWindow.spin(until: { config()["sounds"] as? Bool == false }, timeout: 3), "settings: Sounds off is saved", "\(config())")
        _ = js("const n = document.querySelector('#page input.text'); n.value = 'Ada Lovelace'; n.dispatchEvent(new Event('input')); n.dispatchEvent(new Event('change')); return 1")
        check(WebWindow.spin(until: { config()["author"] as? String == "Ada Lovelace" }, timeout: 3), "settings: your name is saved", "\(config())")
        func radio(_ label: String) -> String {
            js("const b = [...document.querySelectorAll('[role=radio]')].find((x) => x.textContent.trim() === '\(label)'); if (!b) return 'none'; b.click(); return 'ok'")
        }
        check(radio("Dark") == "ok", "settings: Appearance shows")
        check(WebWindow.spin(until: { config()["appearance"] as? String == "dark" }, timeout: 3), "settings: Dark is saved")
        check(NSApp.appearance?.name == .darkAqua, "settings: Dark applies to the app (title bar, traffic lights)")
        check(wait("document.documentElement.hasAttribute('data-dark')"), "settings: the page is dark")
        _ = radio("System")
        check(WebWindow.spin(until: { config()["appearance"] == nil }, timeout: 3), "settings: System removes the key", "\(config())")
        check(NSApp.appearance == nil, "settings: System follows the Mac again")
        check(call("config.set", "{key: 'remind_at', value: '19:30'}") == "{}" && config()["remind_at"] as? String == "19:30", "config.set: a value")
        check(call("config.set", "{key: 'remind_at', value: null}") == "{}" && config()["remind_at"] == nil, "config.set: null removes")

        // Events
        w.emit("appearance", ["dark": true])
        check(wait("document.documentElement.hasAttribute('data-dark')"), "event appearance: dark")
        w.emit("appearance", ["dark": false])
        check(wait("!document.documentElement.hasAttribute('data-dark')"), "event appearance: light")
        w.emit("material", ["material": "glass"])
        check(wait("document.documentElement.dataset.material === 'glass'"), "event material: glass")
        w.emit("material", ["material": "none"])
        check(wait("!document.documentElement.dataset.material"), "event material: none (reduce transparency)")
        model.startedAt = Date().addingTimeInterval(-65)
        model.recording = true
        check(wait("document.querySelector('#pill .dot')"), "event state: recording shows the timer")
        model.recording = false
        model.startedAt = nil
        check(wait("!document.querySelector('#pill .dot')"), "event state: stopped")
        w.navigate("capsules")
        let newCapsule = paths.capsules.appendingPathComponent("web-check.capsule.md")
        try? "---\ntitle: Web check capsule\nproject: website\ndate: 2026-10-23\ncreated: 2026-10-23T18:00:00Z\n---\n\n## In short\nMade by the web window check.\n"
            .write(to: newCapsule, atomically: true, encoding: .utf8)
        _ = js("const f = document.querySelector('#page input[type=search]'); if (f) { f.value = ''; f.dispatchEvent(new Event('input')) } return 1")
        check(wait("document.getElementById('page').innerText.includes('Web check capsule')", 8), "event data-changed: a new capsule shows by itself")
        let notCapsule = fm.temporaryDirectory.appendingPathComponent("not-a-capsule-\(getpid()).md")
        try? "hello".write(to: notCapsule, atomically: true, encoding: .utf8)
        w.openCapsule(notCapsule)
        check(wait("document.querySelector('[role=alertdialog]')?.innerText.includes(\"isn't a capsule\")"), "event open-capsule: checked by the page")
        _ = js("document.querySelector('[role=alertdialog] button').click(); return 1")
        try? fm.removeItem(at: notCapsule)

        // Projects
        let projectsText = text(paths.projects)
        check(call("projects.save", "{text: 'not json'}").hasPrefix("refused"), "projects.save: refuses what isn't projects.json")
        let newProjects = #"{"projects":[{"id":"web-check","name":"Web check","match":["webcheck"]}]}"#
        check(call("projects.save", "{text: \(WebWindow.json(newProjects)!)}") == "{}" && text(paths.projects) == newProjects && projectsChanged == 1,
              "projects.save: written, the recorder told")
        _ = call("projects.save", "{text: \(WebWindow.json(projectsText.isEmpty ? #"{"projects":[]}"# : projectsText)!)}")

        // Recording, shortcuts, set up, updates (stand-ins note what was asked)
        asked.removeAll()
        for a in ["{action: 'toggle'}", "{action: 'voice'}", "{action: 'add-screen'}", "{action: 'new-project'}", "{action: 'project', id: 'website'}", "{action: 'project', id: null}"] {
            _ = call("recorder", a)
        }
        WebWindow.spin(0.2)
        check(asked == ["toggle", "voice", "add-screen", "new-project", "project:website", "project:auto"], "recorder: every action", "\(asked)")
        check(call("recorder", "{action: 'dance'}").hasPrefix("refused"), "recorder: unknown action refused")
        asked.removeAll()
        _ = call("shortcuts.listen", "{on: true}")
        _ = call("shortcuts.listen", "{on: false}")
        check(call("shortcuts.set", "{record: 'Ctrl+Alt+K', voice: 'Alt+V'}") == "{}", "shortcuts.set")
        check(call("shortcuts.set", "{record: 'Nope', voice: 'Alt+V'}").hasPrefix("refused"), "shortcuts.set: bad keys refused")
        check(asked == ["pause:true", "pause:false", "keys:Ctrl+Alt+K,Alt+V"], "shortcuts: listen and set", "\(asked)")
        let status = call("setup.status")
        check(["read", "voice", "dictation", "ai", "login"].allSatisfy { status.contains("\"id\":\"\($0)\"") } && status.contains("screenRecording"),
              "setup.status: the Mac's steps", status)
        _ = call("setup.do", "{id: 'voice'}")
        check(log.contains("setup.do voice"), "setup.do (not done in tests: system prompts)")
        asked.removeAll()
        _ = call("updates", "{action: 'check'}")
        WebWindow.spin(0.1)
        check(asked == ["check-updates"], "updates: check")

        // Your AI, clipboard, links, capsules, apps
        let clients = call("mcp.run", "{command: 'clients'}")
        if clients == "refused: missing" { check(true, "mcp.run clients: the program isn't built here (says \"missing\")") }
        else { check(clients.hasPrefix("\"["), "mcp.run clients: the real program answers", String(clients.prefix(120))) }
        check(call("mcp.run", "{command: 'rm'}").hasPrefix("refused"), "mcp.run: unknown command refused")
        check(call("mcp.run", "{command: 'connect', id: '../x'}").hasPrefix("refused"), "mcp.run: bad app id refused")
        _ = call("clipboard.write", "{text: 'hello from the window'}")
        check(call("clipboard.read") == #"{"text":"hello from the window"}"#, "clipboard: write then read (kept in memory in tests)")
        check(call("open.url", "{url: 'file:///etc/passwd'}").hasPrefix("refused"), "open.url: only web links")
        check(call("open.url", "{url: 'https://chatgpt.com'}") == "{}" && log.contains("open https://chatgpt.com"), "open.url: https")
        check(call("ai.open", "{id: 'claude', prompt: 'Use Context Capsule to test.'}") == #"{"opened":false}"#, "ai.open (not opened in tests)")
        check(call("capsule.open", "{file: 'capsules/../config.json'}").hasPrefix("refused"), "capsule.open: refuses other files")
        check(call("capsule.open", "{file: 'config.json'}").hasPrefix("refused"), "capsule.open: only capsules")
        check(call("capsule.open", "{file: 'capsules/web-check.capsule.md'}") == "{}", "capsule.open")
        check(call("capsule.share", "{file: 'capsules/web-check.capsule.md', how: 'copy-text', text: 'cleaned'}") == #"{"done":true}"#
              && call("clipboard.read") == #"{"text":"cleaned"}"#, "capsule.share: the checked copy's text")
        check(call("capsule.share", "{file: 'capsules/web-check.capsule.md', how: 'reveal'}") == #"{"done":true}"#, "capsule.share: reveal")
        check(call("capsule.share", "{file: 'capsules/nope.md', how: 'copy-file'}") == "refused: gone", "capsule.share: gone")
        check(call("capsule.pick") == "null" && call("apps.pick") == "null", "capsule.pick / apps.pick (panels not opened in tests)")
        check(call("apps.names", "{ids: ['com.apple.Safari']}") == #"{"com.apple.Safari":"Safari"}"#, "apps.names")
        check(call("nope").hasPrefix("refused: unknown method"), "unknown methods refused")

        // Pages that load nothing from outside: only capsule:// was asked for
        check(js("return performance.getEntriesByType('resource').every((r) => r.name.startsWith('capsule://'))") == "1", "no network: every file from capsule://")

        try? fm.removeItem(at: newCapsule)
        window.orderOut(nil)
        print("\(passes) passed, \(failures) failed")
        return failures == 0
    }
}
