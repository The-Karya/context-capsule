// context-capsule — capture: any AI (SPEC "Any AI"). Runs the context-capsule-mcp program (clients, connect,
// disconnect, day, save) off the main thread with a time limit, and does what "Your AI" asks for: Claude with the
// request typed in, another connected app with the request on the clipboard, or Copy my day / Save from clipboard.

import AppKit

/// Runs `context-capsule-mcp <command>` and reads its JSON. Every call returns on the main thread.
enum AIBridge {
    enum Failure: Error, Equatable {
        case missing
        case timeout
        case failed(String)
        case badAnswer

        /// Said plainly, for people.
        var message: String {
            switch self {
            case .missing: return "Part of Context Capsule is missing. Please download Context Capsule again."
            case .timeout: return "That took too long. Please try again."
            case .failed(let why): return why.isEmpty ? "Something went wrong. Please try again." : why
            case .badAnswer: return "Something went wrong. Please try again."
            }
        }
    }

    /// Where the program is (looked up once): inside the app, else the repo's mcp/ folder when developing.
    static let launch: AIProgram.Launch? = {
        let fm = FileManager.default
        let found = AIProgram.find(resources: Bundle.main.resourcePath,
                                   repos: AIProgram.repoCandidates(appPath: Bundle.main.bundlePath, cwd: fm.currentDirectoryPath),
                                   exists: { fm.isExecutableFile(atPath: $0) || ($0.hasSuffix(".js") && fm.fileExists(atPath: $0)) })
        AppLog.shared.write("any AI: program \(found.map { "\($0)" } ?? "not found")")
        return found
    }()

    /// node for the developer fallback (`node mcp/dist/index.js`).
    private static var node: String? {
        ["/opt/homebrew/bin/node", "/usr/local/bin/node", "/usr/bin/node"].first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    /// Runs the program; `input` goes to its stdin. Gives its stdout, or a plain failure.
    /// `lowPriority`: in the background, under `nice -n 10` (Automatic capsules' `make`).
    static func run(_ args: [String], input: Data? = nil, timeout: TimeInterval = 20, lowPriority: Bool = false,
                    done: @escaping (Result<Data, Failure>) -> Void) {
        let finish: (Result<Data, Failure>) -> Void = { r in DispatchQueue.main.async { done(r) } }
        guard let launch else { finish(.failure(.missing)); return }
        DispatchQueue.global(qos: lowPriority ? .utility : .userInitiated).async {
            let p = Process()
            var command: [String]
            switch launch {
            case .binary(let path):
                command = [path] + args
            case .node(let script):
                guard let node else { finish(.failure(.missing)); return }
                command = [node, script] + args
            }
            if lowPriority && FileManager.default.isExecutableFile(atPath: "/usr/bin/nice") {
                command = ["/usr/bin/nice", "-n", "10"] + command
                p.qualityOfService = .utility
            }
            p.executableURL = URL(fileURLWithPath: command[0])
            p.arguments = Array(command.dropFirst())
            let out = Pipe(), err = Pipe(), inp = Pipe()
            p.standardOutput = out
            p.standardError = err
            p.standardInput = inp
            let ended = DispatchSemaphore(value: 0)
            p.terminationHandler = { _ in ended.signal() }
            do { try p.run() } catch {
                AppLog.shared.write("any AI: cannot run \(args.first ?? ""): \(error.localizedDescription)")
                finish(.failure(.missing))
                return
            }
            // Read both pipes and write stdin at the same time, so a full pipe never blocks the program.
            let stdoutBox = Box(), stderrBox = Box()
            let group = DispatchGroup()
            DispatchQueue.global().async(group: group) { stdoutBox.data = out.fileHandleForReading.readDataToEndOfFile() }
            DispatchQueue.global().async(group: group) { stderrBox.data = err.fileHandleForReading.readDataToEndOfFile() }
            DispatchQueue.global().async {
                if let input { try? inp.fileHandleForWriting.write(contentsOf: input) }
                try? inp.fileHandleForWriting.close()
            }
            if ended.wait(timeout: .now() + timeout) == .timedOut {
                p.terminate()
                AppLog.shared.write("any AI: \(args.first ?? "") timed out after \(Int(timeout)) s")
                finish(.failure(.timeout))
                return
            }
            group.wait()
            let stdout = stdoutBox.data, stderr = stderrBox.data
            if p.terminationStatus != 0 && stdout.isEmpty {
                let why = String(decoding: stderr, as: UTF8.self).split(separator: "\n").last.map(String.init) ?? ""
                AppLog.shared.write("any AI: \(args.first ?? "") failed (exit \(p.terminationStatus))")
                finish(.failure(.failed(why.count > 200 ? "" : why)))
                return
            }
            finish(.success(stdout))
        }
    }

    private final class Box: @unchecked Sendable { var data = Data() }

    /// Runs a command and reads its answer with `parse`.
    private static func call<T>(_ args: [String], input: Data? = nil, timeout: TimeInterval = 20,
                                parse: @escaping (Data) throws -> T, done: @escaping (Result<T, Failure>) -> Void) {
        run(args, input: input, timeout: timeout) { r in
            switch r {
            case .failure(let f): done(.failure(f))
            case .success(let data):
                if let v = try? parse(data) { done(.success(v)) } else {
                    AppLog.shared.write("any AI: \(args.first ?? "") gave an answer that can't be read")
                    done(.failure(.badAnswer))
                }
            }
        }
    }

    static func clients(_ done: @escaping (Result<[AIClient], Failure>) -> Void) {
        call(["clients"], timeout: 10, parse: AIAnswer.clients, done: done)
    }
    static func connect(_ id: String, _ done: @escaping (Result<AIAnswer.Connect, Failure>) -> Void) {
        call(["connect", id], parse: AIAnswer.connect, done: done)
    }
    static func disconnect(_ id: String, _ done: @escaping (Result<AIAnswer.Connect, Failure>) -> Void) {
        call(["disconnect", id], parse: AIAnswer.connect, done: done)
    }
    static func day(date: String? = nil, _ done: @escaping (Result<AIAnswer.Day, Failure>) -> Void) {
        call(["day"] + (date.map { ["--date", $0] } ?? []), timeout: 60, parse: AIAnswer.day, done: done)
    }
    static func save(_ reply: String, _ done: @escaping (Result<AIAnswer.Save, Failure>) -> Void) {
        call(["save"], input: Data(reply.utf8), timeout: 30, parse: AIAnswer.save, done: done)
    }
    /// Automatic capsules: the AI command-line tools here (`agents`), and one run (`make`, up to 10 minutes).
    static func agents(_ done: @escaping (Result<Data, Failure>) -> Void) {
        run(["agents"], timeout: 15, done: done)
    }
    static func make(date: String, by: String, _ done: @escaping (Result<Data, Failure>) -> Void) {
        run(["make", "--date", date, "--by", by], timeout: 12 * 60, lowPriority: true, done: done)
    }
}

/// The AI apps on this Mac, which one is "Your AI", and the actions that use it. One for the whole app.
final class AIState: ObservableObject {
    static let shared = AIState()

    /// From `clients`, installed first. Without the program: Claude only (connected the way the app always knew).
    @Published var clients: [AIClient] = []
    @Published var loaded = false
    /// Your AI: an id from clients, a website id, or "any".
    @Published var choice = YourAI.anyID
    /// The app being connected or disconnected now.
    @Published var busy: String?
    /// A short line under the list after connecting ("Connected. Restart Cursor.").
    @Published var note: String?
    /// Shown after Copy my day, until the reply is saved.
    @Published var dayLine: String?
    /// Copy my day or Save from clipboard is running.
    @Published var working = false

    var configURL = Paths.standard.config
    /// A small message under the menu-bar icon.
    var toast: (String) -> Void = { _ in }
    /// Capsules were saved: reload the lists.
    var onSaved: () -> Void = {}
    /// Something changed (the menu shows Save from clipboard for websites / any).
    var onChange: () -> Void = {}

    private var refreshing = false

    var anyConnected: Bool { clients.contains(where: \.connected) || ClaudeConnect.isConnected }
    var kind: YourAI { YourAI.of(choice) }

    func client(_ id: String) -> AIClient? { clients.first { $0.id == id } }

    /// The name for Your AI ("Claude", "Cursor", "ChatGPT", "your AI").
    var choiceName: String {
        switch kind {
        case .claude: return "Claude"
        case .app(let id): return client(id)?.name ?? id
        case .website(let id, _): return YourAI.websites.first { $0.id == id }?.name.replacingOccurrences(of: " (website)", with: "") ?? "your AI"
        case .other: return "your AI"
        }
    }

    /// Ask the program which AI apps there are (a moment, off the main thread).
    func refresh(then: (() -> Void)? = nil) {
        if refreshing { return }
        refreshing = true
        AIBridge.clients { [weak self] r in
            guard let self else { return }
            self.refreshing = false
            var list: [AIClient]
            switch r {
            case .success(let l): list = l
            case .failure:
                let installed = NSWorkspace.shared.urlForApplication(withBundleIdentifier: ClaudeConnect.desktopBundleID) != nil
                list = [AIClient(id: "claude", name: "Claude", installed: installed, connected: false, how: "extension")]
            }
            // Claude's extension is seen at once here, even before the program looks again.
            if let i = list.firstIndex(where: { $0.id == "claude" }), ClaudeConnect.isConnected { list[i].connected = true }
            let sorted = AIAnswer.sorted(list)
            if sorted != self.clients { self.clients = sorted }
            self.loaded = true
            self.updateChoice()
            then?()
        }
    }

    func updateChoice() {
        let c = YourAI.resolve(saved: YourAI.saved(in: configURL), clients: clients)
        if c != choice { choice = c }
        onChange()
    }

    func setChoice(_ id: String) {
        choice = id
        do { try ConfigFile.set("ai", id, in: configURL) } catch { AppLog.shared.write("cannot save ai: \(error.localizedDescription)") }
        AppLog.shared.write("your AI: \(id)")
        dayLine = nil
        onChange()
    }

    // MARK: connect

    func connect(_ c: AIClient) {
        if c.how == "extension" || c.id == "claude" {
            ClaudeConnect.connect()
            note = "Click Install in Claude. The ✓ shows when it's done."
            return
        }
        busy = c.id
        note = nil
        AppLog.shared.write("connect: \(c.id)")
        AIBridge.connect(c.id) { [weak self] r in
            guard let self else { return }
            self.busy = nil
            switch r {
            case .success(let a) where a.ok:
                self.note = a.message.isEmpty ? "Connected to \(c.name). Restart \(c.name) to use it." : a.message
                // The first app connected becomes Your AI, if nothing was chosen.
                if YourAI.saved(in: self.configURL) == nil && !self.clients.contains(where: \.connected) { self.setChoice(c.id) }
            case .success(let a):
                self.alert("Couldn't connect \(c.name)", a.message.isEmpty ? "Please try again." : a.message)
            case .failure(let f):
                self.alert("Couldn't connect \(c.name)", f.message)
            }
            self.refresh()
        }
    }

    func disconnect(_ c: AIClient) {
        busy = c.id
        note = nil
        AppLog.shared.write("disconnect: \(c.id)")
        AIBridge.disconnect(c.id) { [weak self] r in
            guard let self else { return }
            self.busy = nil
            switch r {
            case .success(let a) where a.ok:
                self.note = a.message.isEmpty ? "Disconnected from \(c.name)." : a.message
            case .success(let a) where a.how == "extension":
                self.alert("Remove it in Claude", "In Claude, open Settings → Extensions and remove Context Capsule.")
            case .success(let a):
                self.alert("Couldn't disconnect \(c.name)", a.message.isEmpty ? "Please try again." : a.message)
            case .failure(let f):
                self.alert("Couldn't disconnect \(c.name)", f.message)
            }
            self.refresh()
        }
    }

    // MARK: Make today's capsules, and other requests

    /// Make today's capsules, the way Your AI works.
    func makeTodaysCapsules() {
        switch kind {
        case .claude: Claude.makeTodaysCapsules()
        case .app: open(ClaudeLink.dayPrompt, why: "to read your day", again: "click Make today's capsules again")
        case .website, .other: copyMyDay()
        }
    }

    /// Timeline → Make a capsule from this: one recording. Claude or another connected app gets the request; a website or
    /// any other AI gets Copy my day for that day (it can't read one recording by itself).
    func makeCapsule(session id: String, day: String) {
        let prompt = ClaudeLink.sessionPrompt(id)
        switch kind {
        case .claude: Claude.open(prompt, why: "to read your recording", again: "click Make a capsule from this again")
        case .app: open(prompt, why: "to read your recording", again: "click Make a capsule from this again")
        case .website, .other: copyDay(date: day)
        }
    }

    /// A request for Your AI (ask your capsules, continue from a capsule). Claude, websites and any other AI: Claude, as
    /// before (these need Context Capsule's tools). Another connected app: open it with the request on the clipboard.
    func open(_ prompt: String, why: String, again: String) {
        guard case .app(let id) = kind, let c = client(id) else {
            Claude.open(prompt, why: why, again: again)
            return
        }
        guard c.connected else {
            let alert = NSAlert()
            alert.messageText = "Connect \(c.name) first"
            alert.informativeText = "\(c.name) needs Context Capsule \(why). Click Connect. After that, \(again)."
            alert.addButton(withTitle: "Connect")
            alert.addButton(withTitle: "Cancel")
            NSApp.activate(ignoringOtherApps: true)
            if alert.runModal() == .alertFirstButtonReturn { connect(c) }
            return
        }
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(prompt, forType: .string)
        if let app = Self.appURL(c) {
            AppLog.shared.write("open \(c.id) with a request on the clipboard")
            let config = NSWorkspace.OpenConfiguration()
            config.activates = true
            NSWorkspace.shared.openApplication(at: app, configuration: config) { _, _ in }
            toast("Copied. Paste it in a new chat in \(c.name).")
        } else {
            toast("Copied. Open \(c.name) and paste it in a new chat.")
        }
    }

    /// The Mac app to open for a client: its bundle id or path if the program says, a few known ones, else by name.
    static func appURL(_ c: AIClient) -> URL? {
        let ws = NSWorkspace.shared
        if let b = c.bundle, let u = ws.urlForApplication(withBundleIdentifier: b) { return u }
        if let p = c.app, p.hasSuffix(".app"), FileManager.default.fileExists(atPath: p) { return URL(fileURLWithPath: p) }
        let known = ["claude": ClaudeConnect.desktopBundleID, "cursor": "com.todesktop.230313mzl4w4u92",
                     "vscode": "com.microsoft.VSCode", "windsurf": "com.exafunction.windsurf", "chatgpt": "com.openai.chat",
                     "zed": "dev.zed.Zed"]
        if let b = known[c.id], let u = ws.urlForApplication(withBundleIdentifier: b) { return u }
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        for dir in ["/Applications", home + "/Applications"] {
            let p = "\(dir)/\(c.name).app"
            if FileManager.default.fileExists(atPath: p) { return URL(fileURLWithPath: p) }
        }
        return nil
    }

    // MARK: Copy my day, Save from clipboard

    /// Runs `day`, copies the message, opens the chosen website, and says what to do next.
    func copyMyDay() { copyDay(date: nil) }

    /// Copy my day for a day (YYYY-MM-DD; nil = today).
    func copyDay(date: String?) {
        guard !working else { return }
        working = true
        AppLog.shared.write("copy my day")
        AIBridge.day(date: date) { [weak self] r in
            guard let self else { return }
            self.working = false
            switch r {
            case .failure(let f):
                self.alert("Couldn't copy your day", f.message)
            case .success(let day):
                guard !day.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                    self.toast(date == nil ? "Nothing recorded today yet." : "Nothing recorded that day.")
                    return
                }
                let pb = NSPasteboard.general
                pb.clearContents()
                pb.setString(day.text, forType: .string)
                AppLog.shared.write("copy my day: \(day.chars) characters copied")
                if case .website(_, let url) = self.kind { NSWorkspace.shared.open(url) }
                self.dayLine = YourAI.pasteLine
                self.toast(YourAI.pasteLine)
            }
        }
    }

    /// Runs `save` on the clipboard, then shows what was saved and what wasn't (and why).
    func saveFromClipboard() {
        guard !working else { return }
        let text = NSPasteboard.general.string(forType: .string) ?? ""
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            alert("The clipboard is empty", "Copy your AI's whole reply first, then click Save from clipboard.")
            return
        }
        working = true
        AIBridge.save(text) { [weak self] r in
            guard let self else { return }
            self.working = false
            switch r {
            case .failure(let f):
                self.alert("Couldn't save", f.message)
            case .success(let result):
                AppLog.shared.write("save from clipboard: \(result.saved.count) saved, \(result.refused.count) refused")
                if !result.saved.isEmpty {
                    self.dayLine = nil
                    self.onSaved()
                }
                let s = YourAI.summary(result)
                self.alert(s.title, s.detail)
            }
        }
    }

    /// A small native box: a sheet on the Context Capsule window when it's in front, else on its own.
    private func alert(_ title: String, _ text: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = text
        alert.addButton(withTitle: "OK")
        NSApp.activate(ignoringOtherApps: true)
        if let window = NSApp.windows.first(where: { $0.isVisible && $0.title == AppName.full && $0.attachedSheet == nil }) {
            alert.beginSheetModal(for: window)
        } else {
            alert.runModal()
        }
    }
}
