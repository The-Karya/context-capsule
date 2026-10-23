// context-capsule — capture: Automatic capsules (SPEC "Automatic capsules"). Decides when to start
// `context-capsule-mcp make --by auto` (every minute, at launch, when the Mac wakes), starts click runs, keeps the list
// of AI command-line tools, and says how a run went in a small box at the top right. The AI run itself is the
// program's (mcp/src/auto.ts); auto.json is the one place that says what ran. Nothing is retried by itself.

import AppKit
import SwiftUI

final class AutoRunner {
    let paths: Paths
    private let isRecording: () -> Bool
    /// Today as "yyyy-MM-dd" (tests use another day).
    var today: () -> String = { DayWords.key(Date()) }
    /// Tests: a fixed tool instead of asking the program (`.some(nil)` = none here).
    var fixedAgent: [String: String]?? = nil
    /// Tests and renders: never start a run or show a box.
    var quiet = false
    /// The status changed (a run started or ended, the tools or the settings changed): the bridge's `auto` event.
    var onChange: () -> Void = {}
    /// A run ended: capsules may have been saved.
    var onSaved: () -> Void = {}
    /// Open the window on the Capsules page.
    var openCapsules: () -> Void = {}

    /// The tool a run would use (`{"id","name"}`), nil = none here.
    private(set) var agent: [String: String]?
    private var agentsChecked: Date?
    private var checkingAgents = false
    /// This app's run is going.
    private(set) var going = false
    /// The day this app last started an automatic run: never twice a day, even if `make` stopped before auto.json.
    private var autoTried: String?
    private var timer: Timer?
    private var wakeObservers: [NSObjectProtocol] = []
    private var panel: NSPanel?
    private var dismiss: DispatchWorkItem?

    init(paths: Paths, isRecording: @escaping () -> Bool) {
        self.paths = paths
        self.isRecording = isRecording
    }

    deinit {
        timer?.invalidate()
        wakeObservers.forEach(NSWorkspace.shared.notificationCenter.removeObserver)
    }

    /// At launch: find the tools (then check), every minute after, and when the Mac or its screens wake.
    func start() {
        refreshAgents()
        timer = repeating(60) { [weak self] in self?.tick() }
        let nc = NSWorkspace.shared.notificationCenter
        for name in [NSWorkspace.didWakeNotification, NSWorkspace.screensDidWakeNotification] {
            wakeObservers.append(nc.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in self?.tick() })
        }
    }

    private func tick() {
        if let checked = agentsChecked, Date().timeIntervalSince(checked) < 5 * 60 { checkDue() } else { refreshAgents() }
    }

    /// A setting changed (ai, make_capsules, remind_at): look for the tools again, say so, and check at once.
    func settingChanged() {
        onChange()
        refreshAgents()
    }

    /// Asks `context-capsule-mcp agents` which tool a run would use; then checks whether a run is due.
    func refreshAgents() {
        if let fixed = fixedAgent {
            if fixed != agent { agent = fixed; onChange() }
            agentsChecked = Date()
            checkDue()
            return
        }
        guard !checkingAgents else { return }
        checkingAgents = true
        AIBridge.agents { [weak self] r in
            guard let self else { return }
            self.checkingAgents = false
            self.agentsChecked = Date()
            let found = (try? r.get()).flatMap(AutoRun.agentInUse)
            if found != self.agent {
                self.agent = found
                AppLog.shared.write("automatic capsules: tool \(found?["id"] ?? "none")")
                self.onChange()
            }
            self.checkDue()
        }
    }

    // MARK: what auto.json and config.json say

    var autoFile: URL { paths.root.appendingPathComponent("auto.json") }

    func state() -> AutoRun.State { AutoRun.parse(try? Data(contentsOf: autoFile)) }

    static func alive(_ pid: Int32) -> Bool { kill(pid, 0) == 0 || errno == EPERM }

    var running: Bool { going || state().running(alive: Self.alive) }

    /// Automatically, with a tool here: the evening reminder isn't shown.
    var makesByItself: Bool { agent != nil && Config.load(from: paths.config).makeCapsules == "auto" }

    /// The bridge's auto.status.
    func status() -> [String: Any] {
        let config = Config.load(from: paths.config)
        let st = state()
        return AutoRun.status(mode: config.makeCapsules, at: config.remindAt, agent: agent,
                              running: going || st.running(alive: Self.alive), state: st)
    }

    /// Something recorded that day ("yyyy-MM-dd").
    private func recorded(_ day: String) -> Bool {
        DailyJobs.recorded(on: day.replacingOccurrences(of: "-", with: ""), sessions: paths.sessions)
    }

    // MARK: runs

    private func checkDue() {
        guard !going, !quiet else { return }
        let config = Config.load(from: paths.config)
        let st = state()
        let day = today()
        guard autoTried != day else { return }
        let cal = Calendar.current
        let now = Date()
        let minutes = cal.component(.hour, from: now) * 60 + cal.component(.minute, from: now)
        guard AutoRun.due(nowMinutes: minutes, today: day, mode: config.makeCapsules, at: config.remindAt,
                          hasAgent: agent != nil, autoDay: st.autoDay, lastDay: st.lastDay,
                          lastStartMinutes: st.lastStartMinutes(), recordedToday: recorded(day),
                          recording: isRecording(), running: st.running(alive: Self.alive)) else { return }
        launch(by: "auto")
    }

    /// A click run (the page's Make now / Try again, the menu, the reminder's Make). nil = started, else why not:
    /// "none" (no tool here), "running", "nothing" (nothing recorded today).
    @discardableResult
    func runNow() -> String? {
        guard agent != nil else { return "none" }
        if running { return "running" }
        if !recorded(today()) { return "nothing" }
        launch(by: "click")
        return nil
    }

    private func launch(by: String) {
        if quiet { AppLog.shared.write("automatic capsules: would start (\(by))"); return }
        if by == "auto" { autoTried = today() }
        going = true
        let started = Date()
        AppLog.shared.write("automatic capsules: starting (\(by))")
        onChange()
        AIBridge.make(date: today(), by: by) { [weak self] r in
            guard let self else { return }
            self.going = false
            let mins = max(1, Int((Date().timeIntervalSince(started) / 60).rounded()))
            switch r {
            case .success(let data):
                let obj = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
                let status = obj["status"] as? String ?? "?"
                let n = (obj["capsules"] as? NSNumber)?.intValue ?? 0
                AppLog.shared.write("automatic capsules: \(status) (\(by)), \(n) capsule(s), \(mins) min")
                switch status {
                case "done": self.showReady(n)
                case "failed": self.showFailed(obj["reason"] as? String)
                default: break // none, nothing, running: the log says it
                }
            case .failure(let f):
                AppLog.shared.write("automatic capsules: couldn't run (\(by)): \(f == .timeout ? "timeout" : f == .missing ? "missing" : "error")")
                self.showFailed(f.message)
            }
            self.onChange()
            self.onSaved()
        }
    }

    // MARK: the box (top right, like the evening reminder)

    private func showReady(_ n: Int) {
        show(AutoBox(title: AutoRun.readyTitle(n), detail: nil, action: "Open",
                     close: { [weak self] in self?.close() },
                     act: { [weak self] in self?.close(); self?.openCapsules() }))
        let work = DispatchWorkItem { [weak self] in self?.close() }
        dismiss = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 30, execute: work)
    }

    private func showFailed(_ reason: String?) {
        show(AutoBox(title: "Couldn't make today's capsules", detail: reason?.isEmpty == false ? reason : nil,
                     action: "Try again",
                     close: { [weak self] in self?.close() },
                     act: { [weak self] in
                         self?.close()
                         if let why = self?.runNow() { AppLog.shared.write("automatic capsules: try again: \(why)") }
                     }))
    }

    private func show(_ view: AutoBox) {
        close()
        guard !quiet else { return }
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

    private func close() {
        dismiss?.cancel()
        dismiss = nil
        panel?.orderOut(nil)
        panel = nil
    }
}

private struct AutoBox: View {
    let title: String
    let detail: String?
    let action: String
    let close: () -> Void
    let act: () -> Void
    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 16, style: .continuous)
        let content = VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.system(size: 14, weight: .semibold))
                if let detail {
                    Text(detail).font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
            }
            HStack(spacing: 8) {
                Spacer()
                Button("Close", action: close)
                Button(action, action: act).keyboardShortcut(.defaultAction)
            }
        }
        .padding(16)
        .frame(width: 280)
        if #available(macOS 26, *) {
            content.glassEffect(.regular, in: shape)
        } else {
            content.background(shape.fill(.regularMaterial))
        }
    }
}
