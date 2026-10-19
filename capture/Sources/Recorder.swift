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

    func toggle() {
        if isRecording { stop() } else if isPicking { cancelPicking() } else { start() }
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
            lastError = "cancelled"
        case .failed(let message):
            NSLog("Context Capsule: \(message)")
            if isRecording { endSession(wait: false) }
            lastError = message
        case .stopped(let message):
            if let message { NSLog("Context Capsule: capture stopped: \(message)") }
            endSession(wait: false)
            lastError = message.map { "capture: \($0)" }
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
        sessionID = id
        startedAt = now
        sessionDir = dir
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
                                       events: eventCount, skip: lastSkip, listening: listening, noteAt: noteAt),
                             to: paths.state)
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
        return VoiceTarget(app: app, bundle: bundle, title: title) { [weak self] event in self?.write(event, to: log) }
    }

    /// A note typed in Claude (the MCP's `add_note`). Ignored when not recording.
    func addTypedNote(_ text: String, at: Int64) {
        guard isRecording, let log else { return }
        let now = Date()
        let event = Voice.event(text: text, kind: Event.note, src: "typed", start: now, end: now,
                                app: "", bundle: "", title: "")
        noteAt = at
        write(event, to: log)
        NSLog("Context Capsule: typed note saved: \(event.text.count) chars")
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
        guard let pick = capture.pickedContent() else { return skip(.notPicked, log) }
        if pick.style == .display && pick.rect.isEmpty { return skip(.unknownDisplay, log) }
        guard Membership.contains(pick, window.info) else { return skip(.notPicked, log) }

        sampling = true
        let capture = self.capture
        let changedAt = self.changedAt
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
                guard let frame = capture.latestFrame(), Freshness.ok(frameAt: frame.at, changedAt: changedAt) else {
                    log.breakRun(); outcome = .waitingFrame; return
                }
                switch Membership.ocrDecision(capture.pickedContent() ?? pick, front: window, list: front.list) {
                case .skip(let why):
                    log.breakRun(); outcome = why; return
                case .read(let region, let masks):
                    text = Screen.recognizeText(in: frame.buffer, region: region, masks: masks) // frame dropped after
                }
            }
            let title = AXReader.title(pid: front.pid) ?? capture.pickedWindowTitle(id: window.id) ?? window.name ?? ""
            let stamp = Clock.iso(now)
            let event = Event(t: stamp, end: stamp, app: front.appName, bundle: front.bundle ?? "",
                              title: Redactor.clean(title), src: source.rawValue, text: Redactor.clean(text))
            do { try log.record(event) } catch { NSLog("Context Capsule: cannot write event: \(error)") }
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
