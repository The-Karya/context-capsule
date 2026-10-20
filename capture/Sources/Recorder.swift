// context-capsule — capture: recording sessions — start (via the system picker) / stop, a sample every 3 s,
// state.json, and the `control` file the MCP server writes to start or stop us from outside.

import AppKit

final class Recorder {
    static let interval: TimeInterval = 3

    let paths: Paths
    var onChange: (() -> Void)?
    /// A session ended with nothing written; the reason is the most common skip (main thread).
    var onEmptySession: ((Skip?) -> Void)?
    /// Recording is about to end (any way: hotkey, MCP, macOS sharing icon): finish a voice note first.
    var willStop: (() -> Void)?
    /// A voice note is listening (shown in state.json).
    var listening = false { didSet { if listening != oldValue { writeState() } } }
    /// `at` of the last typed note from the MCP that was written.
    private var noteAt: Int64?
    /// Projects from projects.json (reloaded when the file changes), and auto-detect vs. pinned.
    private(set) var projects: [Project] = []
    private var projectsModified: Date?
    private(set) var projectChoice = ProjectChoice()
    /// The project of the latest sample, and how it was chosen.
    private(set) var currentProject: (id: String, by: String)?
    /// The current project changed (main thread): show it.
    var onProjectChange: ((Project?) -> Void)?
    /// Lines already saved per window in this recording (only touched on the work queue).
    private let newLines = NewLines()
    /// The front window as of the latest sample: what "New project from this window" uses.
    private(set) var lastWindow: (title: String, location: String, app: String)?

    private(set) var sessionID: String?
    private(set) var startedAt: Date?
    /// Why the last start did not happen ("cancelled", "picker: …", "capture: …"); nil after a good start.
    private(set) var lastError: String?
    private var sessionDir: URL?
    private var log: EventLog?
    private var config = Config()
    private var timer: Timer?
    private var sampling = false
    /// Events written in this session, and why the latest sample wrote nothing (nil: it wrote).
    private(set) var eventCount = 0
    private(set) var lastSkip: Skip?
    private var skipCounts: [Skip: Int] = [:]
    /// Last app switch, space switch or Mission Control: frames from before it are stale.
    private var changedAt: Date?
    private let capture = PickerCapture()
    private let work = DispatchQueue(label: "com.thekarya.contextcapsule.sample") // all file writes happen here
    private let host = Host.current().localizedName ?? ProcessInfo.processInfo.hostName

    var isRecording: Bool { sessionID != nil }
    /// The system picker is open, waiting for the user to choose a window or display.
    var isPicking: Bool { capture.isPicking }

    init(paths: Paths) {
        self.paths = paths
        capture.onOutcome = { [weak self] in self?.handle($0) }
        let center = NSWorkspace.shared.notificationCenter
        for name in [NSWorkspace.didActivateApplicationNotification, NSWorkspace.activeSpaceDidChangeNotification] {
            center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in self?.changedAt = Date() }
        }
    }

    // MARK: start / stop (main thread)

    /// Another screen was added (true) or one of several stopped (false), with how many are shared now.
    var onScreens: ((Int, Bool) -> Void)?
    var sharedScreens: Int { capture.sourceCount }
    var canAddScreen: Bool { isRecording && capture.sourceCount < PickerCapture.maxSources }

    /// While recording: share one more screen or window (the picker again).
    func addScreen() {
        guard canAddScreen else { return }
        capture.presentAdd()
    }

    /// A voice note is open: ⌥C waits until it's finished (⌥V), so one key never ends both.
    var onBusyListening: (() -> Void)?

    func toggle() {
        if isRecording && listening { onBusyListening?(); return }
        if isRecording { AppLog.shared.write("stop: hotkey or menu"); stop() } else if isPicking { cancelPicking() } else { start() }
    }

    /// Opens the system picker. Recording begins only once the user picks something.
    func start() {
        guard !isRecording else { return }
        lastError = nil
        capture.present() // also brings an open picker back to the front
        writeState()
        onChange?()
    }

    private func cancelPicking() {
        capture.stop()
        lastError = "cancelled"
        writeState()
        onChange?()
    }

    private func handle(_ outcome: PickerCapture.Outcome) {
        switch outcome {
        case .picked:
            beginSession()
        case .cancelled:
            AppLog.shared.write("picker: cancelled")
            lastError = "cancelled"
        case .failed(let message):
            AppLog.shared.write("start failed: \(message)")
            if isRecording { endSession(wait: false) }
            lastError = message
        case .stopped(let message):
            AppLog.shared.write(message.map { "stop: capture error: \($0)" } ?? "stop: sharing was stopped from macOS")
            endSession(wait: false)
            lastError = message.map { "capture: \($0)" }
        case .added(let count):
            AppLog.shared.write("screen added: \(count) shared")
            onScreens?(count, true)
        case .addCancelled:
            break
        case .removed(let count):
            AppLog.shared.write("a shared screen stopped: \(count) left")
            onScreens?(count, false)
        }
        writeState()
        onChange?()
    }

    private func beginSession() {
        guard !isRecording else { return }
        let now = Date()
        let id = Clock.sessionID(now)
        let dir = paths.sessions.appendingPathComponent(id, isDirectory: true)
        do {
            try writeAtomically(sessionJSON(id: id, startedAt: now, endedAt: nil, host: host),
                                to: dir.appendingPathComponent("session.json"))
            log = try EventLog(url: dir.appendingPathComponent("events.jsonl"))
        } catch {
            NSLog("Context Capsule: cannot create session \(id): \(error)")
            capture.stop()
            lastError = "cannot create session: \(error.localizedDescription)"
            return
        }
        config = Config.load(from: paths.config)
        lastError = nil
        eventCount = 0
        lastSkip = nil
        skipCounts = [:]
        changedAt = Date()
        reloadProjects(force: true)
        currentProject = projectChoice.pinned.map { ($0, "you") }
        askedProject = false
        let newLines = self.newLines
        work.async { newLines.reset() }
        sessionID = id
        startedAt = now
        sessionDir = dir
        let style = capture.pickedContent().map { "\($0.style)" } ?? "unknown"
        AppLog.shared.write("recording started: \(id), picked \(style)")
        timer = repeating(Self.interval) { [weak self] in self?.tick() }
    }

    /// Stops recording (or closes an open picker). `wait` blocks until session.json is on disk (quitting).
    func stop(wait: Bool = false) {
        if isPicking && !isRecording { cancelPicking(); return }
        guard isRecording else { return }
        endSession(wait: wait)
        writeState()
        onChange?()
    }

    private func endSession(wait: Bool) {
        if isRecording { willStop?() }
        capture.stop()
        guard let id = sessionID, let started = startedAt, let dir = sessionDir else { return }
        timer?.invalidate()
        timer = nil
        sessionID = nil
        startedAt = nil
        sessionDir = nil
        let top = skipCounts.max { $0.value < $1.value }?.key
        let skips = skipCounts.map { "\($0.key.rawValue) \($0.value)" }.sorted().joined(separator: ", ")
        AppLog.shared.write("recording ended: \(id), \(eventCount) events" + (skips.isEmpty ? "" : ", skipped: \(skips)"))
        if let log { // checked after any sample still running
            work.async { if log.appended == 0 { DispatchQueue.main.async { self.onEmptySession?(top) } } }
        }
        log = nil
        let ended = Date()
        let host = self.host
        let finish: () -> Void = {
            try? writeAtomically(sessionJSON(id: id, startedAt: started, endedAt: ended, host: host),
                                 to: dir.appendingPathComponent("session.json"))
        }
        if wait { work.sync(execute: finish) } else { work.async(execute: finish) }
    }

    func writeState() {
        try? writeAtomically(stateJSON(recording: isRecording, sessionID: sessionID, startedAt: startedAt,
                                       pid: ProcessInfo.processInfo.processIdentifier,
                                       picking: isPicking && !isRecording, lastError: lastError,
                                       events: eventCount, skip: lastSkip, listening: listening, noteAt: noteAt,
                                       project: currentProject?.id, projectBy: currentProject?.by),
                             to: paths.state)
    }

    // MARK: projects (main thread)

    func reloadProjects(force: Bool = false) {
        let modified = (try? FileManager.default.attributesOfItem(atPath: paths.projects.path))?[.modificationDate] as? Date
        guard force || modified != projectsModified else { return }
        projectsModified = modified
        projects = Projects.load(from: paths.projects)
    }

    /// Pin to a project (menu, or the MCP's `set_project`), or nil to auto-detect again.
    func pinProject(_ id: String?) {
        reloadProjects()
        projectChoice.pinned = id.flatMap { id in projects.contains { $0.id == id } ? id : nil }
        AppLog.shared.write(projectChoice.pinned.map { "project pinned: \($0)" } ?? "project: auto-detect")
        if let pinned = projectChoice.pinned { setCurrent((pinned, "you")) } else if currentProject?.by == "you" { setCurrent(nil) }
    }

    /// The window in front right now (title, page/file, app), for "New project from this window" when no
    /// sample has been taken yet. Private apps and our own app give nil.
    func currentWindow() -> (title: String, location: String, app: String)? {
        let own = Bundle.main.bundleIdentifier ?? "com.thekarya.contextcapsule"
        let userList = Config.load(from: paths.config).excludeApps
        let isExcluded: (String?, String?) -> Bool = { bundle, name in
            bundle == own || Exclusion.isExcluded(bundle: bundle, appName: name, userList: userList)
        }
        guard let front = Screen.frontWindow(isExcluded: isExcluded), !isExcluded(front.bundle, front.appName) else {
            return lastWindow
        }
        let title = AXReader.title(pid: front.pid) ?? front.window?.name ?? ""
        return (Redactor.clean(title), Location.clean(AXReader.location(pid: front.pid)), front.appName)
    }

    /// Asked "What are you working on?" in this session already.
    private var askedProject = false
    /// No project found for the first window of a recording: ask the user (main thread).
    var onAskProject: (((title: String, location: String, app: String)) -> Void)?

    /// The user answered "What are you working on?": an existing project gets this window's clue, a new
    /// name becomes a project with it. Auto-detect then finds it from now on.
    func answerProject(name: String, window: (title: String, location: String, app: String)) {
        let name = name.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        reloadProjects(force: true)
        let existing = projects.first { $0.name.caseInsensitiveCompare(name) == .orderedSame || $0.id == Projects.slug(name) }
        let clue = Projects.clue(title: window.title, location: window.location)
        guard let project = createProject(name: existing?.name ?? name, clue: clue) else { return }
        AppLog.shared.write("project answered: \(project.id)\(existing == nil ? " (new)" : ""), clue \(clue == nil ? "none" : "added")")
        if clue == nil { pinProject(project.id) } else { setCurrent((project.id, "auto")) }
    }

    /// Make a project from the front window: the name the user typed, the window's best clue as `match`.
    @discardableResult
    func createProject(name: String, clue: String?) -> Project? {
        reloadProjects(force: true)
        let id = Projects.slug(name)
        guard !id.isEmpty else { return nil }
        if let i = projects.firstIndex(where: { $0.id == id }) {
            if let clue, !projects[i].match.contains(where: { $0.caseInsensitiveCompare(clue) == .orderedSame }) {
                projects[i].match.append(clue)
            }
        } else {
            projects.append(Project(id: id, name: name, match: clue.map { [$0] } ?? []))
        }
        do { try Projects.save(projects, to: paths.projects) } catch {
            AppLog.shared.write("cannot save projects.json: \(error.localizedDescription)")
            return nil
        }
        reloadProjects(force: true)
        return projects.first { $0.id == id }
    }

    private func setCurrent(_ next: (id: String, by: String)?) {
        let changed = next?.id != currentProject?.id
        currentProject = next
        guard changed else { return }
        writeState()
        onChange?()
        if isRecording { onProjectChange?(next.flatMap { n in projects.first { $0.id == n.id } }) }
    }

    // MARK: notes (main thread)

    /// The voice locale from config.json, else the user's.
    var voiceLocale: Locale {
        Config.load(from: paths.config).voiceLocale.map { Locale(identifier: $0) } ?? Locale.current
    }

    /// Where a voice note goes: the current session, about the front window (nothing about a private
    /// app). nil when not recording.
    func noteTarget() -> VoiceTarget? {
        guard isRecording, let log else { return nil }
        let own = Bundle.main.bundleIdentifier ?? "com.thekarya.contextcapsule"
        let userList = config.excludeApps
        let isExcluded: (String?, String?) -> Bool = { bundle, name in
            bundle == own || Exclusion.isExcluded(bundle: bundle, appName: name, userList: userList)
        }
        var app = "", bundle = "", title = ""
        if let front = Screen.frontWindow(isExcluded: isExcluded), !isExcluded(front.bundle, front.appName) {
            app = front.appName
            bundle = front.bundle ?? ""
            title = AXReader.title(pid: front.pid) ?? front.window?.name ?? ""
        }
        let project = currentProject
        return VoiceTarget(app: app, bundle: bundle, title: title) { [weak self] event in
            var event = event
            if let project { event.project = project.id; event.projectBy = project.by }
            self?.write(event, to: log)
        }
    }

    /// A note typed in Claude (the MCP's `add_note`). Ignored when not recording.
    func addTypedNote(_ text: String, at: Int64) {
        guard isRecording, let log else { return }
        let now = Date()
        var event = Voice.event(text: text, kind: Event.note, src: "typed", start: now, end: now,
                                app: "", bundle: "", title: "")
        if let project = currentProject { event.project = project.id; event.projectBy = project.by }
        noteAt = at
        write(event, to: log)
        AppLog.shared.write("typed note saved: \(event.text.count) chars")
    }

    private func write(_ event: Event, to log: EventLog) {
        work.async { [weak self] in
            do { try log.record(event) } catch { NSLog("Context Capsule: cannot write note: \(error)") }
            let count = log.appended
            DispatchQueue.main.async {
                guard let self, self.log === log else { return }
                self.eventCount = count
                self.writeState()
                self.onChange?()
            }
        }
    }

    // MARK: sampling

    /// One sample: only when the front window is part of what the user picked. Text comes from
    /// Accessibility when allowed and useful, else from OCR of the picked content's latest frame —
    /// cropped to the front window, never-record windows painted out, never a frame from before the
    /// last app switch, never during Mission Control.
    private func tick() {
        guard let log, !sampling else { return } // skip a tick if the last one is still busy
        let now = Date()
        let own = Bundle.main.bundleIdentifier ?? "com.thekarya.contextcapsule"
        let userList = config.excludeApps
        let isExcluded: (String?, String?) -> Bool = { bundle, name in
            bundle == own || Exclusion.isExcluded(bundle: bundle, appName: name, userList: userList)
        }
        guard let front = Screen.frontWindow(isExcluded: isExcluded) else { return skip(.noWindow, log) }

        if isExcluded(front.bundle, front.appName) { return skip(.privateApp, log) } // nothing read
        if WindowList.overlayActive(front.list) {
            changedAt = now // the frame after it closes is the first good one
            return skip(.overlay, log)
        }
        guard let window = front.window else { return skip(.noWindow, log) }
        // Several screens can be shared: use the one holding the window in front (keyboard focus, not the mouse).
        let picks = capture.pickedContents()
        guard !picks.isEmpty else { return skip(.notPicked, log) }
        guard let pick = Membership.pick(for: window.info, in: picks) else {
            let unknown = picks.contains { $0.style == .display && $0.rect.isEmpty }
            return skip(unknown ? .unknownDisplay : .notPicked, log)
        }

        sampling = true
        reloadProjects()
        let capture = self.capture
        let changedAt = self.changedAt
        let projects = self.projects
        let choice = self.projectChoice
        let newLines = self.newLines
        work.async { [weak self] in
            var outcome: Skip?
            defer {
                let count = log.appended
                DispatchQueue.main.async { self?.sampled(outcome, count: count) }
            }
            let axText = AXReader.text(pid: front.pid)
            let source = AXText.choose(trusted: AXReader.trusted, axText: axText)
            var text = axText
            if source == .ocr {
                guard let frame = capture.latestFrame(source: pick.source),
                      Freshness.ok(frameAt: frame.at, changedAt: changedAt) else {
                    log.breakRun(); outcome = .waitingFrame; return
                }
                let current = capture.pickedContents().first { $0.source == pick.source } ?? pick
                switch Membership.ocrDecision(current, front: window, list: front.list) {
                case .skip(let why):
                    log.breakRun(); outcome = why; return
                case .read(let region, let masks):
                    text = Screen.recognizeText(in: frame.buffer, region: region, masks: masks) // frame dropped after
                }
            }
            let title = AXReader.title(pid: front.pid) ?? capture.pickedWindowTitle(id: window.id) ?? window.name ?? ""
            let location = Location.clean(AXReader.location(pid: front.pid))
            let stamp = Clock.iso(now)
            var event = Event(t: stamp, end: stamp, app: front.appName, bundle: front.bundle ?? "",
                              title: Redactor.clean(title), src: source.rawValue, text: Redactor.clean(text))
            event.location = location
            if let project = choice.resolve(projects, title: event.title, location: location, app: front.appName) {
                event.project = project.id
                event.projectBy = project.by
            }
            let found = event.project.isEmpty ? nil : (event.project, event.projectBy)
            let window = (event.title, location, front.appName)
            DispatchQueue.main.async { self?.sampledProject(found, window: window) }
            // Save only what's new in this window since it was last seen; nothing new: just extend the time.
            event.text = newLines.filter(event.text, window: NewLines.key(app: event.app, bundle: event.bundle,
                                                                          title: event.title, location: location))
            if event.text.isEmpty, (try? log.extendIfSameWindow(event)) == true { return }
            do { try log.record(event) } catch { NSLog("Context Capsule: cannot write event: \(error)") }
        }
    }

    /// Main thread: the project the latest written sample got. Auto mode keeps the last project across
    /// windows with no clue (Slack, Mail…) for display only; the events themselves stay unassigned.
    private func sampledProject(_ found: (id: String, by: String)?, window: (String, String, String)) {
        guard isRecording else { return }
        lastWindow = (window.0, window.1, window.2)
        if let found { setCurrent(found) }
        if config.askProject && found == nil && currentProject == nil && !askedProject {
            askedProject = true
            onAskProject?((window.0, window.1, window.2))
        }
    }

    private func skip(_ why: Skip, _ log: EventLog) {
        work.async { log.breakRun() }
        sampled(why, count: eventCount)
    }

    /// Main thread: note how the sample went; state.json only changes when something did.
    private func sampled(_ why: Skip?, count: Int) {
        sampling = false
        guard isRecording else { return }
        if let why { skipCounts[why, default: 0] += 1 }
        guard why != lastSkip || count != eventCount else { return }
        lastSkip = why
        eventCount = count
        writeState()
        onChange?()
    }
}

/// A main-thread timer that keeps firing while the menu is open (common run-loop modes).
func repeating(_ interval: TimeInterval, _ block: @escaping () -> Void) -> Timer {
    let timer = Timer(timeInterval: interval, repeats: true) { _ in block() }
    RunLoop.main.add(timer, forMode: .common)
    return timer
}

/// Polls `~/.context-capsule/control` twice a second. Each new `at` value is acted on once.
final class ControlWatcher {
    private let url: URL
    private let handler: (ControlCommand) -> Void
    private var timer: Timer?
    private var lastModified: Date?
    private var lastAt: Int64?

    init(url: URL, handler: @escaping (ControlCommand) -> Void) {
        self.url = url
        self.handler = handler
        // A command left over from before we launched is stale: remember it, don't run it.
        lastModified = modified()
        lastAt = (try? Data(contentsOf: url)).flatMap(ControlCommand.parse)?.at
        timer = repeating(0.5) { [weak self] in self?.poll() }
    }

    deinit { timer?.invalidate() }

    private func modified() -> Date? {
        (try? FileManager.default.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date
    }

    private func poll() {
        let m = modified()
        guard let m, m != lastModified else { return }
        lastModified = m
        guard let data = try? Data(contentsOf: url), let cmd = ControlCommand.parse(data), cmd.at != lastAt else { return }
        lastAt = cmd.at
        handler(cmd)
    }
}
