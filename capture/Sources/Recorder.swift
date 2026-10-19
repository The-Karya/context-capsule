// karya-capsule — capture: recording sessions — start (via the system picker) / stop, a sample every 3 s,
// state.json, and the `control` file the MCP server writes to start or stop us from outside.

import AppKit

final class Recorder {
    static let interval: TimeInterval = 3

    let paths: Paths
    var onChange: (() -> Void)?

    private(set) var sessionID: String?
    private(set) var startedAt: Date?
    /// Why the last start did not happen ("cancelled", "picker: …", "capture: …"); nil after a good start.
    private(set) var lastError: String?
    private var sessionDir: URL?
    private var log: EventLog?
    private var config = Config()
    private var timer: Timer?
    private var sampling = false
    private let capture = PickerCapture()
    private let work = DispatchQueue(label: "com.karya.capsule.sample") // all file writes happen here
    private let host = Host.current().localizedName ?? ProcessInfo.processInfo.hostName

    var isRecording: Bool { sessionID != nil }
    /// The system picker is open, waiting for the user to choose a window or display.
    var isPicking: Bool { capture.isPicking }

    init(paths: Paths) {
        self.paths = paths
        capture.onOutcome = { [weak self] in self?.handle($0) }
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
            NSLog("Capsule: \(message)")
            if isRecording { endSession(wait: false) }
            lastError = message
        case .stopped(let message):
            if let message { NSLog("Capsule: capture stopped: \(message)") }
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
            NSLog("Capsule: cannot create session \(id): \(error)")
            capture.stop()
            lastError = "cannot create session: \(error.localizedDescription)"
            return
        }
        config = Config.load(from: paths.config)
        lastError = nil
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
        capture.stop()
        guard let id = sessionID, let started = startedAt, let dir = sessionDir else { return }
        timer?.invalidate()
        timer = nil
        sessionID = nil
        startedAt = nil
        sessionDir = nil
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
                                       picking: isPicking && !isRecording, lastError: lastError), to: paths.state)
    }

    // MARK: sampling

    /// One sample: only when the front window is part of what the user picked. Text comes from
    /// Accessibility when allowed and useful, else from OCR of the picked content's latest frame.
    private func tick() {
        guard let log, !sampling else { return } // skip a tick if the last one is still busy
        let now = Date()
        guard let front = Screen.frontWindow() else { return }

        let own = Bundle.main.bundleIdentifier ?? "com.karya.capsule"
        if front.bundle == own
            || Exclusion.isExcluded(bundle: front.bundle, appName: front.appName, userList: config.excludeApps) {
            work.async { log.breakRun() } // never record: nothing read, nothing written
            return
        }
        guard let window = front.window, let pick = capture.pickedContent(), Membership.contains(pick, window) else {
            work.async { log.breakRun() } // not what the user shared: write nothing
            return
        }

        sampling = true
        let capture = self.capture
        work.async { [weak self] in
            defer { DispatchQueue.main.async { self?.sampling = false } }
            let axText = AXReader.text(pid: front.pid)
            let source = AXText.choose(trusted: AXReader.trusted, axText: axText)
            var text = axText
            if source == .ocr {
                guard let frame = capture.latestFrame(),
                      let region = Membership.ocrRegion(capture.pickedContent() ?? pick, window: window.frame)
                else { log.breakRun(); return }
                text = Screen.recognizeText(in: frame, region: region) // the frame is dropped when this ends
            }
            let title = AXReader.title(pid: front.pid) ?? capture.pickedWindowTitle(id: window.id) ?? ""
            let stamp = Clock.iso(now)
            let event = Event(t: stamp, end: stamp, app: front.appName, bundle: front.bundle ?? "",
                              title: Redactor.clean(title), src: source.rawValue, text: Redactor.clean(text))
            do { try log.record(event) } catch { NSLog("Capsule: cannot write event: \(error)") }
        }
    }
}

/// A main-thread timer that keeps firing while the menu is open (common run-loop modes).
func repeating(_ interval: TimeInterval, _ block: @escaping () -> Void) -> Timer {
    let timer = Timer(timeInterval: interval, repeats: true) { _ in block() }
    RunLoop.main.add(timer, forMode: .common)
    return timer
}

/// Polls `~/.karya-capsule/control` twice a second. Each new `at` value is acted on once.
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
