// context-capsule — capture: the Context Capsule window — one place for everything: Home (start/stop, speak a note,
// add a screen, project, make today's capsules), Capsules (search, Ask your AI, open a capsule someone sent, Copy my day /
// Save from clipboard), Projects, Settings (setup, Connect your AI, Your AI, preferences) and Help. Capsule files
// dropped on the window are opened in your AI.
// Plain native look with a sidebar, like System Settings.

import AppKit
import Speech
import SwiftUI

/// What the window shows about the recorder, and what it can ask the app to do. Updated by the app delegate.
final class AppModel: ObservableObject {
    @Published var recording = false
    @Published var listening = false
    @Published var picking = false
    @Published var startedAt: Date?
    @Published var moments = 0
    @Published var projectID: String?
    @Published var projectPinned = false
    @Published var projects: [Project] = []
    @Published var screens = 0
    @Published var canAddScreen = false
    @Published var hasSeveralScreens = false
    @Published var allScreens = false
    /// The shortcuts, as the Mac writes them ("⌥C").
    @Published var recordKey = Shortcut.defaultRecord.symbols
    @Published var voiceKey = Shortcut.defaultVoice.symbols
    /// For the personal Home.
    @Published var name = ""
    @Published var today = WorkStats.Totals()
    @Published var week = WorkStats.Totals()
    @Published var weekCapsules = 0
    @Published var recentCapsules: [CapsuleSummary] = []
    @Published var streak = 0

    var toggleRecord: () -> Void = {}
    var toggleVoice: () -> Void = {}
    var addScreen: () -> Void = {}
    var makeCapsules: () -> Void = {}
    var pickProject: (String?) -> Void = { _ in }
    var newProject: () -> Void = {}
    var askAccessibility: () -> Void = {}
    /// New keys for Start or stop (true) or Speak a note; nil = done, else why not.
    var setShortcut: (Bool, Shortcut) -> String? = { _, _ in nil }
    /// While Settings listens for new keys, the shortcuts are off (so pressing them doesn't start anything).
    var pauseHotKeys: (Bool) -> Void = { _ in }
    var resetShortcuts: () -> Void = {}
    /// Updates (Settings → Updates).
    @Published var version = ""
    @Published var updateLine = ""
    @Published var updateReady = false
    @Published var updateWorking = false
    var checkForUpdates: () -> Void = {}
    var restartToUpdate: () -> Void = {}

    var projectName: String? { projectID.flatMap { id in projects.first { $0.id == id }?.name ?? id } }
}

enum Section: String, CaseIterable, Identifiable {
    case home, timeline, capsules, projects, settings, help
    var id: String { rawValue }
    var title: String {
        switch self {
        case .home: return "Home"
        case .timeline: return "Timeline"
        case .capsules: return "Capsules"
        case .projects: return "Projects"
        case .settings: return "Settings"
        case .help: return "Help"
        }
    }
    var icon: String {
        switch self {
        case .home: return "house"
        case .timeline: return "calendar.day.timeline.left"
        case .capsules: return "doc.text"
        case .projects: return "folder"
        case .settings: return "gearshape"
        case .help: return "questionmark.circle"
        }
    }
}

final class Navigation: ObservableObject {
    @Published var section: Section? = .home
}

struct MainView: View {
    @ObservedObject var app: AppModel
    @ObservedObject var library: LibraryModel
    @ObservedObject var nav: Navigation
    @ObservedObject var setup: SetupStatus
    @ObservedObject var prefs: Preferences
    @ObservedObject var timeline: TimelineModel
    @ObservedObject var ai = AIState.shared

    var body: some View {
        NavigationSplitView {
            List(selection: $nav.section) {
                SwiftUI.Section("Your work") {
                    ForEach([Section.home, .timeline, .capsules, .projects]) { s in Label(s.title, systemImage: s.icon).tag(s) }
                }
                SwiftUI.Section("Context Capsule") {
                    ForEach([Section.settings, .help]) { s in Label(s.title, systemImage: s.icon).tag(s) }
                }
            }
            .navigationSplitViewColumnWidth(min: 170, ideal: 190, max: 240)
        } detail: {
            // The content widens with the window up to a comfortable width, and stays centred;
            // on wide windows Home gets a second column. The timeline scrolls on its own (a lazy list: big days stay fast).
            Group {
            if nav.section == .timeline {
                TimelinePage(model: timeline, app: app, library: library)
            } else {
            GeometryReader { geo in
                ScrollView {
                    VStack(alignment: .leading, spacing: nav.section == .home ? 24 : 8) {
                        switch nav.section ?? .home {
                        case .home: HomeView(app: app, nav: nav, ai: ai, wide: geo.size.width > 900)
                        case .timeline: EmptyView()
                        case .capsules: CapsulesView(library: library, app: app, ai: ai)
                        case .projects: ProjectsView(library: library)
                        case .settings: SettingsView(status: setup, prefs: prefs, app: app, ai: ai)
                        case .help: HelpView(app: app, ai: ai)
                        }
                    }
                    .environment(\.captionGap, nav.section == .home ? 0 : 16)
                    .padding(.horizontal, 32)
                    .padding(.vertical, 24)
                    .frame(maxWidth: geo.size.width > 900 ? 1120 : 760, alignment: .leading)
                    .frame(maxWidth: .infinity)
                }
            }
            }
            }
            .navigationTitle("")
            .toolbar {
                ToolbarItem(placement: .primaryAction) { StatusPill(app: app) }
            }
        }
        .frame(minWidth: 760, minHeight: 540)
        .onAppear { library.reload() }
        // A capsule someone sent, dropped on the window: your AI carries on from it.
        .dropDestination(for: URL.self) { urls, _ in
            guard let url = urls.first(where: \.isFileURL) else { return false }
            DispatchQueue.main.async { Claude.continueFrom(url) }
            return true
        }
    }
}

// MARK: Home

struct Title: View {
    let text: String
    var body: some View { Text(text).font(.system(size: 22, weight: .semibold)) }
}

private struct CaptionGap: EnvironmentKey { static let defaultValue: CGFloat = 0 }
extension EnvironmentValues {
    /// Space above a caption: pages that list caption, panel, caption, panel… use it to keep each caption with its panel.
    var captionGap: CGFloat { get { self[CaptionGap.self] } set { self[CaptionGap.self] = newValue } }
}

struct Caption: View {
    let text: String
    @Environment(\.captionGap) private var gap
    var body: some View {
        Text(text).font(.system(size: 12, weight: .medium)).foregroundStyle(.secondary).padding(.leading, 4).padding(.top, gap)
    }
}

/// One project's share of the time: name, a bar, the duration.
private struct TimeBar: View {
    let name: String
    let seconds: Double
    let total: Double
    var muted = false

    var body: some View {
        HStack(spacing: 12) {
            Text(name).font(.system(size: 13)).foregroundStyle(muted ? .secondary : .primary)
                .frame(width: 150, alignment: .leading).lineLimit(1)
            Capsule().fill(Color.primary.opacity(0.08))
                .frame(height: 8)
                .overlay(alignment: .leading) {
                    GeometryReader { geo in
                        Capsule().fill(muted ? Color.secondary.opacity(0.5) : Color.accentColor)
                            .frame(width: max(6, geo.size.width * CGFloat(total > 0 ? seconds / total : 0)))
                    }
                }
            Text(WorkStats.duration(seconds)).font(.system(size: 12).monospacedDigit()).foregroundStyle(.secondary)
                .frame(width: 70, alignment: .trailing)
        }
    }
}

/// Top right of the window: the live timer while contextualizing, else today's time.
private struct StatusPill: View {
    @ObservedObject var app: AppModel

    var body: some View {
        HStack(spacing: 6) {
            if app.recording {
                Circle().fill(Color.red).frame(width: 7, height: 7)
                TimelineView(.periodic(from: .now, by: 1)) { ctx in
                    Text(MenuBarLook.clock(ctx.date.timeIntervalSince(app.startedAt ?? ctx.date))).monospacedDigit()
                }
            } else {
                Image(systemName: "clock").foregroundStyle(.secondary)
                Text("Today \(WorkStats.duration(app.today.seconds))")
            }
        }
        .font(.system(size: 12, weight: .medium))
        .padding(.horizontal, 10)
        .help(app.recording ? "Contextualizing" : "Time contextualized today")
    }
}

/// A key on the keyboard, drawn like a keycap.
private struct Keycap: View {
    let text: String
    var body: some View {
        Text(text)
            .font(.system(size: 13, weight: .semibold, design: .rounded))
            .padding(.horizontal, 9).padding(.vertical, 4)
            .background(RoundedRectangle(cornerRadius: 6, style: .continuous).fill(Color.primary.opacity(0.07)))
            .overlay(RoundedRectangle(cornerRadius: 6, style: .continuous).strokeBorder(Color.primary.opacity(0.14)))
    }
}

/// A big number with a label above and a line below.
private struct Stat: View {
    let label: String
    let value: String
    let detail: String
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(.system(size: 12, weight: .medium)).foregroundStyle(.secondary)
            Text(value).font(.system(size: 28, weight: .semibold).monospacedDigit()).lineLimit(1).minimumScaleFactor(0.6)
            Text(detail).font(.system(size: 12)).foregroundStyle(.secondary).lineLimit(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(18)
    }
}

private struct HomeView: View {
    @ObservedObject var app: AppModel
    @ObservedObject var nav: Navigation
    @ObservedObject var ai: AIState
    var wide = false

    private var greeting: String {
        let hello = WorkStats.greeting(hour: Calendar.current.component(.hour, from: Date()))
        return (app.name.isEmpty ? hello : "\(hello), \(app.name)") + "."
    }

    private func projectName(_ id: String) -> String {
        id.isEmpty ? "Not sorted yet" : app.projects.first { $0.id == id }?.name ?? id
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(greeting).font(.system(size: 34, weight: .regular, design: .serif))
            Text(Date().formatted(.dateTime.weekday(.wide).day().month(.wide)))
                .font(.system(size: 13)).foregroundStyle(.secondary)
        }

        if wide {
            HStack(alignment: .top, spacing: 20) {
                VStack(alignment: .leading, spacing: 24) { main }.frame(maxWidth: .infinity)
                VStack(alignment: .leading, spacing: 16) { side }.frame(width: 280)
            }
        } else {
            main
            side
        }
    }

    // MARK: main column

    @ViewBuilder private var main: some View {
        statusCard
        stats
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Caption(text: "Today by project")
                Spacer()
                Button("See today's timeline") { nav.section = .timeline }.buttonStyle(.borderless).font(.system(size: 12))
            }
            Panel {
                VStack(alignment: .leading, spacing: 10) {
                    if app.today.byProject.isEmpty {
                        Text("Nothing yet today. Start contextualizing when you begin work.")
                            .font(.system(size: 13)).foregroundStyle(.secondary)
                    } else {
                        ForEach(app.today.byProject.prefix(5), id: \.id) { p in
                            TimeBar(name: projectName(p.id), seconds: p.seconds,
                                    total: app.today.byProject.first?.seconds ?? 1, muted: p.id.isEmpty)
                        }
                    }
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Caption(text: "Recent capsules")
                Spacer()
                if !app.recentCapsules.isEmpty {
                    Button("See all") { nav.section = .capsules }.buttonStyle(.borderless).font(.system(size: 12))
                }
            }
            Panel {
                if app.recentCapsules.isEmpty {
                    Text("None yet. At the end of the day, make today's capsules.")
                        .font(.system(size: 13)).foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading).padding(16)
                } else {
                    ForEach(Array(app.recentCapsules.enumerated()), id: \.element.url) { i, c in
                        if i > 0 { Divider().padding(.leading, 46) }
                        CapsuleRow(capsule: c)
                    }
                }
            }
        }
    }

    private var statusCard: some View {
        Panel {
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 10) {
                    if app.recording {
                        Circle().fill(Color.red).frame(width: 10, height: 10)
                        TimelineView(.periodic(from: .now, by: 1)) { ctx in
                            Text("Contextualizing  \(MenuBarLook.clock(ctx.date.timeIntervalSince(app.startedAt ?? ctx.date)))")
                                .font(.system(size: 17, weight: .semibold).monospacedDigit())
                        }
                    } else {
                        Circle().strokeBorder(Color.secondary, lineWidth: 1.5).frame(width: 10, height: 10)
                        Text(app.picking ? "Pick a window or screen" : "Not contextualizing")
                            .font(.system(size: 17, weight: .semibold))
                    }
                    Spacer()
                    Button(app.recording ? "Stop" : app.picking ? "Cancel" : "Start contextualizing", action: app.toggleRecord)
                        .controlSize(.large)
                        .keyboardShortcut(.defaultAction)
                }
                if app.recording {
                    HStack(spacing: 10) {
                        Text("\(app.moments) moment\(app.moments == 1 ? "" : "s") saved"
                             + (app.screens > 1 ? " · \(app.screens) screens" : "")
                             + (app.projectName.map { " · \($0)" } ?? ""))
                            .font(.system(size: 13)).foregroundStyle(.secondary)
                        Spacer()
                        Button(app.listening ? "Done speaking" : "Speak a note", action: app.toggleVoice)
                        if app.hasSeveralScreens {
                            Button("Add a screen", action: app.addScreen).disabled(!app.canAddScreen)
                        }
                    }
                } else {
                    Text(app.allScreens ? "Shares all your screens. Private apps are never read."
                                        : "Pick the screen or window you work on. Only that is read.")
                        .font(.system(size: 13)).foregroundStyle(.secondary)
                }
            }
            .padding(18)
        }
    }

    private var stats: some View {
        Panel {
            HStack(spacing: 0) {
                Stat(label: "Today", value: WorkStats.duration(app.today.seconds), detail: "contextualized")
                Divider().padding(.vertical, 14)
                Stat(label: "This week", value: WorkStats.duration(app.week.seconds),
                     detail: "\(app.weekCapsules) capsule\(app.weekCapsules == 1 ? "" : "s") made")
                Divider().padding(.vertical, 14)
                Stat(label: "Streak", value: "\(app.streak) day\(app.streak == 1 ? "" : "s")",
                     detail: app.streak > 0 ? "in a row" : "starts today")
            }
        }
    }

    // MARK: side column

    @ViewBuilder private var side: some View {
        Panel {
            VStack(alignment: .leading, spacing: 12) {
                Text("Shortcuts").font(.system(size: 14, weight: .semibold))
                HStack { Keycap(text: app.recordKey); Text("Start or stop").font(.system(size: 13)); Spacer() }
                HStack { Keycap(text: app.voiceKey); Text("Speak a note").font(.system(size: 13)); Spacer() }
            }
            .padding(16)
        }

        Panel {
            VStack(alignment: .leading, spacing: 2) {
                Text("Working on").font(.system(size: 14, weight: .semibold)).padding(.bottom, 8)
                projectRow("Find by itself", icon: "magnifyingglass", selected: !app.projectPinned) { app.pickProject(nil) }
                ForEach(app.projects, id: \.id) { p in
                    projectRow(p.name, icon: "folder", selected: app.projectPinned && app.projectID == p.id) { app.pickProject(p.id) }
                }
                projectRow("New project", icon: "plus", selected: false, action: app.newProject)
                    .foregroundStyle(.secondary)
            }
            .padding(16)
        }

        Panel {
            VStack(alignment: .leading, spacing: 10) {
                Text("Make today's capsules").font(.system(size: 14, weight: .semibold))
                if ai.kind.usesCopy {
                    Text(ai.dayLine ?? "Copy your day, paste it into \(ai.choiceName), then save its reply here.")
                        .font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    Button(action: app.makeCapsules) { Text("Copy my day").frame(maxWidth: .infinity) }
                        .controlSize(.large)
                        .buttonStyle(.borderedProminent)
                        .disabled(ai.working)
                    Button(action: ai.saveFromClipboard) { Text("Save from clipboard").frame(maxWidth: .infinity) }
                        .controlSize(.large)
                        .disabled(ai.working)
                } else {
                    Text(ai.kind == .claude ? "Claude opens with the request typed in. Press Return."
                                            : "\(ai.choiceName) opens. Paste the request in a new chat.")
                        .font(.system(size: 12)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    Button(action: app.makeCapsules) { Text("Make capsules").frame(maxWidth: .infinity) }
                        .controlSize(.large)
                        .buttonStyle(.borderedProminent)
                }
                HStack(spacing: 4) {
                    Text("Your AI: \(ai.kind == .other ? "any AI" : ai.choiceName)")
                    Button("Change") { nav.section = .settings }.buttonStyle(.link)
                }
                .font(.system(size: 12)).foregroundStyle(.secondary)
            }
            .padding(16)
        }
    }

    private func projectRow(_ title: String, icon: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: icon).frame(width: 16).foregroundStyle(.secondary)
                Text(title).lineLimit(1)
                Spacer()
                if selected { Image(systemName: "checkmark").foregroundStyle(Color.accentColor) }
            }
            .font(.system(size: 13))
            .padding(.vertical, 5)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

// MARK: Capsules and Projects

private struct CapsulesView: View {
    @ObservedObject var library: LibraryModel
    @ObservedObject var app: AppModel
    @ObservedObject var ai: AIState

    private var empty: Bool { library.query.trimmingCharacters(in: .whitespaces).isEmpty }

    var body: some View {
        HStack {
            Title(text: "Capsules")
            Spacer()
            Button("Open a capsule…", action: Claude.pickCapsule)
                .help("Open a capsule someone sent you. Your AI carries on from it.")
            Button("Make today's capsules", action: app.makeCapsules)
        }
        HStack(spacing: 8) {
            SearchField(text: $library.query, prompt: "Search your capsules")
            Button(ai.kind == .claude ? "Ask Claude" : "Ask your AI") { Claude.ask(library.query) }
                .disabled(empty)
                .help("Your AI searches all your capsules for this.")
        }
        HStack(spacing: 8) {
            Text(ai.dayLine ?? "Using an AI on the web, or any other AI?")
                .font(.system(size: 12)).foregroundStyle(.secondary).lineLimit(2)
            Spacer()
            Button("Copy my day", action: ai.copyMyDay).disabled(ai.working)
                .help("Copies today's work as one message to paste into any AI.")
            Button("Save from clipboard", action: ai.saveFromClipboard).disabled(ai.working)
                .help("Saves the capsules in your AI's reply. Copy its whole reply first.")
        }
        .padding(.bottom, 4)
        if library.capsules.isEmpty {
            Text("No capsules yet. Start contextualizing (\(app.recordKey)), then make today's capsules.")
                .font(.system(size: 13)).foregroundStyle(.secondary)
        } else if library.groups.isEmpty {
            Text("No titles match. Ask your AI to look inside every capsule.")
                .font(.system(size: 13)).foregroundStyle(.secondary)
        } else {
            ForEach(library.groups, id: \.title) { group in
                VStack(alignment: .leading, spacing: 6) {
                    Caption(text: group.title)
                    Panel {
                        ForEach(Array(group.items.enumerated()), id: \.element.url) { i, c in
                            if i > 0 { Divider().padding(.leading, 46) }
                            CapsuleRow(capsule: c)
                        }
                    }
                }
            }
        }
    }
}

/// The Mac's own search field. Filters as you type; Return does nothing more.
struct SearchField: NSViewRepresentable {
    @Binding var text: String
    let prompt: String

    final class Coordinator: NSObject, NSSearchFieldDelegate {
        var parent: SearchField
        init(_ parent: SearchField) { self.parent = parent }
        func controlTextDidChange(_ note: Notification) {
            if let field = note.object as? NSSearchField { parent.text = field.stringValue }
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSSearchField {
        let field = NSSearchField()
        field.placeholderString = prompt
        field.delegate = context.coordinator
        field.sendsWholeSearchString = false
        field.sendsSearchStringImmediately = true
        field.controlSize = .large
        field.font = .systemFont(ofSize: 13)
        field.stringValue = text
        return field
    }

    func updateNSView(_ field: NSSearchField, context: Context) {
        context.coordinator.parent = self
        if field.stringValue != text { field.stringValue = text }
    }
}

private struct ProjectsView: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        Title(text: "Projects")
        Text("Work is sorted into these by itself, from words in window names and web addresses.")
            .font(.system(size: 13)).foregroundStyle(.secondary)
        if library.projects.isEmpty {
            Text("No projects yet. Your AI makes them when it sorts your day.")
                .font(.system(size: 13)).foregroundStyle(.secondary)
        } else {
            Panel {
                ForEach(Array(library.projects.enumerated()), id: \.element.id) { i, p in
                    if i > 0 { Divider().padding(.leading, 14) }
                    ProjectRow(project: p, model: library)
                }
            }
        }
    }
}

// MARK: Settings

/// The preferences in config.json, shown as controls; every change is saved at once.
final class Preferences: ObservableObject {
    let url: URL
    @Published var remind = true
    @Published var remindTime = Calendar.current.date(bySettingHour: 18, minute: 0, second: 0, of: Date()) ?? Date()
    @Published var keepDays = 30
    @Published var sounds = true
    @Published var voiceLocale = ""
    @Published var neverRecord: [String] = []
    @Published var languages: [(id: String, name: String)] = []
    @Published var name = ""
    /// "system", "light" or "dark".
    @Published var appearance = "system"
    /// "When you start": the picker (false) or every screen at once (true).
    @Published var allScreens = false
    @Published var screenRecording = CGPreflightScreenCaptureAccess()
    @Published var autoUpdate = true

    /// The first word of the name, for the greeting.
    var firstName: String { name.split(separator: " ").first.map(String.init) ?? "" }

    func saveName() {
        let clean = name.trimmingCharacters(in: .whitespaces)
        save("author", clean.isEmpty ? nil : clean)
    }

    /// The name in config.json ("author"), else the Mac account's full name.
    static func name(from url: URL) -> String {
        if let data = try? Data(contentsOf: url), let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let a = obj["author"] as? String, !a.trimmingCharacters(in: .whitespaces).isEmpty { return a }
        return NSFullUserName()
    }

    init(url: URL) { self.url = url }

    /// config.json "appearance": "light" or "dark"; anything else follows the Mac.
    static func appearance(from url: URL) -> String {
        guard let data = try? Data(contentsOf: url), let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let a = obj["appearance"] as? String, ["light", "dark"].contains(a) else { return "system" }
        return a
    }

    static func apply(_ appearance: String) {
        switch appearance {
        case "light": NSApp.appearance = NSAppearance(named: .aqua)
        case "dark": NSApp.appearance = NSAppearance(named: .darkAqua)
        default: NSApp.appearance = nil
        }
    }

    func saveAllScreens() {
        save("share_screens", allScreens ? "all" : nil)
        if allScreens && !CGPreflightScreenCaptureAccess() { _ = CGRequestScreenCaptureAccess() }
        screenRecording = CGPreflightScreenCaptureAccess()
    }

    func saveAppearance() {
        save("appearance", appearance == "system" ? nil : appearance)
        Self.apply(appearance)
    }

    func load() {
        let c = Config.load(from: url)
        remind = c.remindAt != nil
        if let at = c.remindAt, let m = Reminder.parse(at) {
            remindTime = Calendar.current.date(bySettingHour: m / 60, minute: m % 60, second: 0, of: Date()) ?? remindTime
        }
        keepDays = c.keepDays
        sounds = c.sounds
        autoUpdate = c.autoUpdate
        voiceLocale = c.voiceLocale ?? ""
        neverRecord = c.excludeApps
        name = Self.name(from: url)
        appearance = Self.appearance(from: url)
        allScreens = Config.load(from: url).shareAllScreens
        screenRecording = CGPreflightScreenCaptureAccess()
        if languages.isEmpty {
            // Only languages this Mac can turn into text by itself (nothing is sent to Apple).
            languages = SFSpeechRecognizer.supportedLocales()
                .filter { SFSpeechRecognizer(locale: $0)?.supportsOnDeviceRecognition == true }
                .map { ($0.identifier, Locale.current.localizedString(forIdentifier: $0.identifier) ?? $0.identifier) }
                .sorted { $0.name < $1.name }
        }
    }

    func save(_ key: String, _ value: Any?) {
        do { try ConfigFile.set(key, value, in: url) } catch { AppLog.shared.write("cannot save a setting: \(error.localizedDescription)") }
    }

    func saveReminder() {
        let c = Calendar.current.dateComponents([.hour, .minute], from: remindTime)
        save("remind_at", remind ? String(format: "%02d:%02d", c.hour ?? 18, c.minute ?? 0) : false)
    }

    /// Pick an app (in /Applications) never to record.
    func addApp() {
        let panel = NSOpenPanel()
        panel.directoryURL = URL(fileURLWithPath: "/Applications")
        panel.allowedContentTypes = [.application]
        panel.prompt = "Never record"
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url, let id = Bundle(url: url)?.bundleIdentifier,
              !neverRecord.contains(id) else { return }
        neverRecord.append(id)
        save("exclude_apps", neverRecord)
    }

    func remove(_ id: String) {
        neverRecord.removeAll { $0 == id }
        save("exclude_apps", neverRecord)
    }

    static func appName(_ id: String) -> String {
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: id) else { return id }
        return FileManager.default.displayName(atPath: url.path).replacingOccurrences(of: ".app", with: "")
    }
}

private struct SettingsView: View {
    @ObservedObject var status: SetupStatus
    @ObservedObject var prefs: Preferences
    @ObservedObject var app: AppModel
    @ObservedObject var ai: AIState

    private func row<Content: View>(_ title: String, @ViewBuilder _ control: () -> Content) -> some View {
        HStack {
            Text(title).font(.system(size: 14))
            Spacer()
            control()
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 44)
    }

    var body: some View {
        Title(text: "Settings")

        Caption(text: "You")
        Panel {
            row("Your name") {
                TextField("Name", text: $prefs.name)
                    .textFieldStyle(.roundedBorder).frame(width: 220)
                    .onSubmit { prefs.saveName(); app.name = prefs.firstName }
                    .onChange(of: prefs.name) { prefs.saveName(); app.name = prefs.firstName }
            }
        }
        Text("Shown on Home, and written on your capsules as the author.")
            .font(.system(size: 12)).foregroundStyle(.secondary).padding(.leading, 4)

        Caption(text: "Set up")
        Panel {
            StepRow(icon: "text.viewfinder", done: status.readsText, title: "Read your apps",
                    help: "Clean text, window names and web addresses, so projects are found by themselves.",
                    button: "Turn On", action: app.askAccessibility)
            Divider().padding(.leading, 48)
            StepRow(icon: "mic", done: status.microphone && status.speech, title: "Voice notes",
                    help: "Say what you're doing and why. Turned into text on this Mac; audio is never saved.",
                    button: "Allow", action: Setup.askVoice)
            Divider().padding(.leading, 48)
            StepRow(icon: "waveform", done: status.dictation, title: "Dictation",
                    help: "Apple needs it for voice notes. Scroll down to Dictation and switch it on.",
                    button: "Turn On") { Setup.openSettings(Setup.keyboardPane) }
            Divider().padding(.leading, 48)
            ConnectAIRows(ai: ai, done: status.ai)
            Divider().padding(.leading, 48)
            StepRow(icon: "power", done: status.atLogin, title: "Open at login",
                    help: "Always ready in the menu bar.",
                    button: "Turn On") { Setup.setLaunchAtLogin(true); status.refresh() }
        }

        Caption(text: "Your AI")
        Panel {
            row("Make today's capsules with") {
                Picker("", selection: Binding(get: { ai.choice }, set: { ai.setChoice($0) })) {
                    ForEach(ai.clients.filter { $0.installed || $0.id == ai.choice }, id: \.id) { c in
                        Text(c.connected ? c.name : "\(c.name) (not connected)").tag(c.id)
                    }
                    if !ai.clients.contains(where: { $0.id == ai.choice }) && YourAI.of(ai.choice) == .app(ai.choice) {
                        Text(ai.choice).tag(ai.choice)
                    }
                    Divider()
                    ForEach(YourAI.websites, id: \.id) { w in Text(w.name).tag(w.id) }
                    Text("Any other AI").tag(YourAI.anyID)
                }
                .labelsHidden().frame(width: 240)
            }
        }
        Text(ai.kind.usesCopy ? "Make today's capsules copies your day to paste into \(ai.choiceName), then you save its reply."
                              : "Make today's capsules opens \(ai.choiceName) with the request.")
            .font(.system(size: 12)).foregroundStyle(.secondary).padding(.leading, 4)

        Caption(text: "Shortcuts")
        Panel {
            ShortcutRow(title: "Start or stop", record: true, app: app)
            Divider().padding(.leading, 14)
            ShortcutRow(title: "Speak a note", record: false, app: app)
        }
        HStack {
            Text("Click a shortcut, then press the keys you want.").font(.system(size: 12)).foregroundStyle(.secondary)
            Spacer()
            if app.recordKey != Shortcut.defaultRecord.symbols || app.voiceKey != Shortcut.defaultVoice.symbols {
                Button("Reset", action: app.resetShortcuts)
                .buttonStyle(.borderless).font(.system(size: 12))
            }
        }
        .padding(.leading, 4)

        Caption(text: "Screens")
        Panel {
            row("When you start") {
                Picker("", selection: $prefs.allScreens) {
                    Text("Pick a screen or window").tag(false)
                    Text("All my screens").tag(true)
                }
                .labelsHidden().frame(width: 220)
                .onChange(of: prefs.allScreens) { prefs.saveAllScreens(); app.allScreens = prefs.allScreens }
            }
            if prefs.allScreens && !prefs.screenRecording {
                Divider().padding(.leading, 14)
                row("Allow Screen Recording, then quit and reopen") {
                    Button("Open Settings") { Setup.openSettings(Setup.screenRecordingPane) }
                }
            }
        }
        Text(prefs.allScreens ? "Every screen is shared at once, with no picker. Private apps are still never read."
                              : "Each time, you pick what's shared. Add more screens while contextualizing.")
            .font(.system(size: 12)).foregroundStyle(.secondary).padding(.leading, 4)

        Caption(text: "Look")
        Panel {
            row("Appearance") {
                Picker("", selection: $prefs.appearance) {
                    Text("System").tag("system")
                    Text("Light").tag("light")
                    Text("Dark").tag("dark")
                }
                .labelsHidden().pickerStyle(.segmented).frame(width: 260)
                .onChange(of: prefs.appearance) { prefs.saveAppearance() }
            }
        }

        Caption(text: "Every day")
        Panel {
            row("Remind me to make capsules") {
                Toggle("", isOn: $prefs.remind).labelsHidden().toggleStyle(.switch).onChange(of: prefs.remind) { prefs.saveReminder() }
            }
            if prefs.remind {
                Divider().padding(.leading, 14)
                row("At") {
                    DatePicker("", selection: $prefs.remindTime, displayedComponents: .hourAndMinute)
                        .labelsHidden().onChange(of: prefs.remindTime) { prefs.saveReminder() }
                }
            }
            Divider().padding(.leading, 14)
            row("Delete old recordings after") {
                Picker("", selection: $prefs.keepDays) {
                    Text("1 week").tag(7)
                    Text("30 days").tag(30)
                    Text("90 days").tag(90)
                    Text("Never").tag(0)
                }
                .labelsHidden().fixedSize()
                .onChange(of: prefs.keepDays) { prefs.save("keep_recordings_days", prefs.keepDays) }
            }
            Divider().padding(.leading, 14)
            row("Sounds") {
                Toggle("", isOn: $prefs.sounds).labelsHidden().toggleStyle(.switch).onChange(of: prefs.sounds) { prefs.save("sounds", prefs.sounds) }
            }
        }
        Text("Capsules are never deleted.").font(.system(size: 12)).foregroundStyle(.secondary).padding(.leading, 4)

        Caption(text: "Voice notes")
        Panel {
            row("Language") {
                Picker("", selection: $prefs.voiceLocale) {
                    Text("Same as my Mac").tag("")
                    ForEach(prefs.languages, id: \.id) { l in Text(l.name).tag(l.id) }
                }
                .labelsHidden().frame(maxWidth: 240)
                .onChange(of: prefs.voiceLocale) { prefs.save("voice_locale", prefs.voiceLocale.isEmpty ? nil : prefs.voiceLocale) }
            }
        }

        Caption(text: "Never record")
        Panel {
            Text("Password managers, WhatsApp and Signal are never recorded. Add other apps here.")
                .font(.system(size: 12)).foregroundStyle(.secondary)
                .padding(.horizontal, 14).padding(.top, 10).padding(.bottom, prefs.neverRecord.isEmpty ? 0 : 4)
            ForEach(prefs.neverRecord, id: \.self) { id in
                Divider().padding(.leading, 14)
                row(Preferences.appName(id)) { Button("Remove") { prefs.remove(id) } }
            }
            Divider().padding(.leading, 14)
            row("Add an app") { Button("Choose…") { prefs.addApp() } }
        }

        Caption(text: "Updates")
        Panel {
            row("Update automatically") {
                Toggle("", isOn: $prefs.autoUpdate).labelsHidden().toggleStyle(.switch)
                    .onChange(of: prefs.autoUpdate) { prefs.save("auto_update", prefs.autoUpdate) }
            }
            Divider().padding(.leading, 14)
            row("Version \(app.version)") {
                if app.updateReady {
                    Button("Restart now", action: app.restartToUpdate)
                } else {
                    Button("Check for Updates", action: app.checkForUpdates).disabled(app.updateWorking)
                }
            }
        }
        Text(app.updateLine.isEmpty ? "New versions are signed and checked. Nothing about you is sent." : app.updateLine)
            .font(.system(size: 12)).foregroundStyle(.secondary).padding(.leading, 4)
    }
}

// MARK: Help

/// One shortcut in Settings: click, press the new keys (Esc cancels).
private struct ShortcutRow: View {
    let title: String
    let record: Bool
    @ObservedObject var app: AppModel
    @State private var listening = false
    @State private var problem: String?
    @State private var monitor: Any?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(title).font(.system(size: 14))
                Spacer()
                Button(listening ? "Press keys…" : (record ? app.recordKey : app.voiceKey)) { listening ? stop() : start() }
                    .frame(minWidth: 110)
            }
            if let problem {
                Text(problem).font(.system(size: 12)).foregroundStyle(.red)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .frame(minHeight: 44)
        .onDisappear { stop() }
    }

    private func start() {
        problem = nil
        listening = true
        app.pauseHotKeys(true)
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            if event.keyCode == 53 { stop(); return nil } // Esc
            guard let keys = KeyCodes.shortcut(from: event) else { return nil }
            stop()
            problem = app.setShortcut(record, keys)
            return nil
        }
    }

    private func stop() {
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
        if listening { app.pauseHotKeys(false) }
        listening = false
    }
}

private struct HelpView: View {
    @ObservedObject var app: AppModel
    @ObservedObject var ai: AIState

    private func line(_ keys: String, _ text: String) -> some View {
        HStack {
            Text(text).font(.system(size: 14))
            Spacer()
            Text(keys).font(.system(size: 14)).foregroundStyle(.secondary)
        }
        .padding(.horizontal, 14)
        .frame(height: 40)
    }

    var body: some View {
        Title(text: "Help")
        Caption(text: "Shortcuts")
        Panel {
            line(app.recordKey, "Start or stop contextualizing")
            Divider().padding(.leading, 14)
            line(app.voiceKey, "Speak a note, then again when done")
            Divider().padding(.leading, 14)
            line(ai.kind.usesCopy ? "Copy my day" : "in your AI", "Make my capsules for today")
        }
        Caption(text: "How it works")
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                Text("While you work, Context Capsule saves the text of the screen you picked, and what you say in voice notes.")
                Text("At the end of the day, your AI sorts it into projects and writes a capsule for each: what you did, what you decided and why, and what's next.")
                Text("Any AI works. Apps like Claude or Cursor connect in Settings. For any other AI, such as a website: Copy my day, paste it in, then Save from clipboard.")
                Text("Send a capsule to anyone. Their AI reads it and can carry on your work.")
            }
            .font(.system(size: 13))
            .padding(14)
        }
        Caption(text: "Privacy")
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                Text("Recordings stay on this Mac. No account, no cloud.")
                Text("Only text is saved: no screenshots, no audio.")
                Text("Password managers, WhatsApp, Signal and password fields are never recorded.")
                Text("Text goes to your AI only when you ask it to make or read a capsule, or paste your day into it yourself.")
            }
            .font(.system(size: 13))
            .padding(14)
        }
    }
}

/// The window: one at a time; shows in the Dock and ⌘-Tab while it's open.
final class MainWindow: NSObject, NSWindowDelegate {
    /// Set once the window has opened on first launch (the old welcome screen used the same key).
    static let seenKey = "welcomeSeen"
    static var seen: Bool { UserDefaults.standard.bool(forKey: seenKey) }

    let app: AppModel
    let library: LibraryModel
    let nav = Navigation()
    let timeline: TimelineModel
    let setup = SetupStatus()
    let prefs: Preferences
    private var window: NSWindow?

    init(app: AppModel, paths: Paths) {
        self.app = app
        library = LibraryModel(paths: paths)
        timeline = TimelineModel(paths: paths)
        prefs = Preferences(url: paths.config)
        self.paths = paths
        Preferences.apply(Preferences.appearance(from: paths.config))
    }

    private let paths: Paths
    private var statsTimer: Timer?

    /// Home's numbers: today, this week, the latest capsule, the name for the greeting.
    func refreshStats() {
        let cal = Calendar.current
        let now = Date()
        let dayStart = cal.startOfDay(for: now)
        let weekStart = cal.dateInterval(of: .weekOfYear, for: now)?.start ?? dayStart.addingTimeInterval(-6 * 86_400)
        let projects = Projects.load(from: paths.projects)
        app.today = WorkStats.totals(sessions: paths.sessions, from: dayStart, to: now.addingTimeInterval(60), projects: projects)
        app.week = WorkStats.totals(sessions: paths.sessions, from: weekStart, to: now.addingTimeInterval(60), projects: projects)
        let capsules = CapsuleSummary.all(in: paths.capsules)
        let iso = ISO8601DateFormatter()
        app.weekCapsules = capsules.filter { c in c.kind != "project" && (iso.date(from: c.created).map { $0 >= weekStart } ?? false) }.count
        app.recentCapsules = Array(capsules.prefix(3))
        app.streak = WorkStats.streak(sessionIDs: (try? FileManager.default.contentsOfDirectory(atPath: paths.sessions.path)) ?? [], today: now)
        app.allScreens = Config.load(from: paths.config).shareAllScreens
        app.name = Preferences.name(from: paths.config).split(separator: " ").first.map(String.init) ?? ""
    }

    func show(_ section: Section? = nil) {
        if let section { nav.section = section }
        library.reload()
        prefs.load()
        refreshStats()
        if statsTimer == nil { statsTimer = repeating(60) { [weak self] in self?.refreshStats() } }
        setup.start()
        NSApp.setActivationPolicy(.regular)
        // While the window is open the app is a normal app, so it needs the usual menus: without them, a full-screen
        // window never shows its close / minimise buttons, and ⌘C / ⌘V don't work in text fields.
        if NSApp.mainMenu?.items.isEmpty ?? true { NSApp.mainMenu = MainMenu.make { [weak self] in self?.show(.settings) } }
        if let window {
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            return
        }
        let view = MainView(app: app, library: library, nav: nav, setup: setup, prefs: prefs, timeline: timeline)
        let window = NSWindow(contentViewController: NSHostingController(rootView: view))
        window.title = "Context Capsule"
        window.styleMask = [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView]
        window.titlebarAppearsTransparent = true
        window.toolbarStyle = .unified
        window.isReleasedWhenClosed = false
        window.setFrameAutosaveName("ContextCapsuleMain")
        window.delegate = self
        if !window.setFrameUsingName("ContextCapsuleMain") { window.setContentSize(NSSize(width: 860, height: 620)); window.center() }
        self.window = window
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) {
        setup.stop()
        statsTimer?.invalidate()
        statsTimer = nil
        window = nil
        NSApp.setActivationPolicy(.accessory) // back to menu bar only
    }
}

/// The menu bar while the window is open: Context Capsule, Edit, View, Window.
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
        submenu("Context Capsule", [
            item("About Context Capsule", #selector(NSApplication.orderFrontStandardAboutPanel(_:)), ""),
            .separator(),
            settingsItem,
            .separator(),
            item("Hide Context Capsule", #selector(NSApplication.hide(_:)), "h"),
            .separator(),
            item("Quit Context Capsule", #selector(NSApplication.terminate(_:)), "q"),
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
