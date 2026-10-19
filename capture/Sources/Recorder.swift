// karya-capsule — capture: recording sessions — start/stop, a sample every 3 s, state.json,
// and the `control` file the MCP server writes to start or stop us from outside.

import AppKit

final class Recorder {
    static let interval: TimeInterval = 3

    let paths: Paths
    var onChange: (() -> Void)?
    var onPermissionNeeded: (() -> Void)?

    private(set) var sessionID: String?
    private(set) var startedAt: Date?
    private var sessionDir: URL?
    private var log: EventLog?
    private var config = Config()
    private var timer: Timer?
    private var sampling = false
    private let work = DispatchQueue(label: "com.karya.capsule.sample") // all file writes happen here
    private let host = Host.current().localizedName ?? ProcessInfo.processInfo.hostName

    var isRecording: Bool { sessionID != nil }

    init(paths: Paths) { self.paths = paths }

    // MARK: start / stop (main thread)

    func toggle() { isRecording ? stop() : start() }

    func start() {
        guard !isRecording else { return }
        guard CGPreflightScreenCaptureAccess() else { onPermissionNeeded?(); return }

        let now = Date()
        let id = Clock.sessionID(now)
        let dir = paths.sessions.appendingPathComponent(id, isDirectory: true)
        do {
            try writeAtomically(sessionJSON(id: id, startedAt: now, endedAt: nil, host: host),
                                to: dir.appendingPathComponent("session.json"))
            log = try EventLog(url: dir.appendingPathComponent("events.jsonl"))
        } catch {
            NSLog("Capsule: cannot create session \(id): \(error)")
            return
        }
        config = Config.load(from: paths.config)
        sessionID = id
        startedAt = now
        sessionDir = dir
        timer = repeating(Self.interval) { [weak self] in self?.tick() }
        tick()
        writeState()
        onChange?()
    }

    /// `wait` blocks until the last sample and session.json are on disk (used when quitting).
    func stop(wait: Bool = false) {
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
        writeState()
        onChange?()
    }

    func writeState() {
        try? writeAtomically(stateJSON(recording: isRecording, sessionID: sessionID, startedAt: startedAt,
                                       pid: ProcessInfo.processInfo.processIdentifier), to: paths.state)
    }

    // MARK: sampling

    private func tick() {
        guard let log, !sampling else { return } // skip a tick if OCR is still busy
        let now = Date()
        guard let front = Screen.frontWindow() else { return }

        let own = Bundle.main.bundleIdentifier ?? "com.karya.capsule"
        if front.bundle == own
            || Exclusion.isExcluded(bundle: front.bundle, appName: front.appName, userList: config.excludeApps) {
            work.async { log.breakRun() } // never record: no screenshot, nothing written
            return
        }

        sampling = true
        work.async { [weak self] in
            defer { DispatchQueue.main.async { self?.sampling = false } }
            guard let image = Screen.screenshot(display: front.displayID) else { log.breakRun(); return }
            let text = Screen.recognizeText(in: image) // the image is dropped when this closure ends
            let stamp = Clock.iso(now)
            let event = Event(t: stamp, end: stamp, app: front.appName, bundle: front.bundle ?? "",
                              title: Redactor.clean(front.title), text: Redactor.clean(text))
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
