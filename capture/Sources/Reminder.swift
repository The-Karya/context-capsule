// context-capsule — capture: the evening "Make today's capsules?" box (one click does it with your AI: opens it with the request
// typed in) and the clean-up of old raw recordings. Capsules are never deleted.

import AppKit
import SwiftUI

final class DailyJobs {
    private let paths: Paths
    private let isRecording: () -> Bool
    private var timer: Timer?
    private var panel: NSPanel?
    private var lastCleanup: String?
    var onMake: () -> Void = {}

    init(paths: Paths, isRecording: @escaping () -> Bool) {
        self.paths = paths
        self.isRecording = isRecording
    }

    func start() {
        tick()
        timer = repeating(60) { [weak self] in self?.tick() }
    }

    private static func today(_ date: Date = Date()) -> String { Clock.sessionID(date).prefix(8).description }

    private func tick() {
        let config = Config.load(from: paths.config)
        let day = Self.today()
        if lastCleanup != day {
            lastCleanup = day
            cleanup(keepDays: config.keepDays)
        }
        let cal = Calendar.current
        let now = Date()
        let minutes = cal.component(.hour, from: now) * 60 + cal.component(.minute, from: now)
        let asked = UserDefaults.standard.string(forKey: "remindedDay") == day
        guard panel == nil,
              Reminder.due(now: minutes, at: config.remindAt, recordedToday: recordedToday(day),
                           capsuleToday: capsuleToday(day), askedToday: asked, recording: isRecording()) else { return }
        UserDefaults.standard.set(day, forKey: "remindedDay")
        AppLog.shared.write("evening reminder shown")
        show()
    }

    private func recordedToday(_ day: String) -> Bool {
        let ids = (try? FileManager.default.contentsOfDirectory(atPath: paths.sessions.path)) ?? []
        return ids.contains { id in
            guard id.hasPrefix(day) else { return false }
            let events = paths.sessions.appendingPathComponent(id).appendingPathComponent("events.jsonl")
            return ((try? FileManager.default.attributesOfItem(atPath: events.path))?[.size] as? Int ?? 0) > 0
        }
    }

    private func capsuleToday(_ day: String) -> Bool {
        let ymd = "\(day.prefix(4))-\(day.dropFirst(4).prefix(2))-\(day.suffix(2))"
        return CapsuleSummary.all(in: paths.capsules).contains { c in
            c.date == ymd || (c.kind != "project" && c.created.count >= 10 && Self.localDay(c.created) == ymd)
        }
    }

    private static func localDay(_ iso: String) -> String {
        let f = ISO8601DateFormatter()
        guard let d = f.date(from: iso) else { return String(iso.prefix(10)) }
        let out = DateFormatter()
        out.dateFormat = "yyyy-MM-dd"
        return out.string(from: d)
    }

    /// Raw recordings older than `keepDays` days are removed; the current one never is.
    private func cleanup(keepDays: Int) {
        let ids = (try? FileManager.default.contentsOfDirectory(atPath: paths.sessions.path)) ?? []
        let old = Cleanup.oldSessions(ids, keepDays: keepDays, now: Date())
        guard !old.isEmpty else { return }
        var removed = 0
        for id in old where (try? FileManager.default.removeItem(at: paths.sessions.appendingPathComponent(id))) != nil {
            removed += 1
        }
        AppLog.shared.write("clean-up: removed \(removed) recording(s) older than \(keepDays) days (capsules kept)")
    }

    // MARK: the box

    func show() {
        let view = ReminderView(
            make: { [weak self] in self?.close(); self?.onMake() },
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

    private func close() {
        panel?.orderOut(nil)
        panel = nil
    }
}

private struct ReminderView: View {
    let make: () -> Void
    let later: () -> Void
    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 16, style: .continuous)
        let content = VStack(alignment: .leading, spacing: 12) {
            Text("Make today's capsules?").font(.system(size: 14, weight: .semibold))
            HStack(spacing: 8) {
                Spacer()
                Button("Not now", action: later)
                Button("Make", action: make).keyboardShortcut(.defaultAction)
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
