// context-capsule — capture: the Context Capsule window — one place for everything: Home (start/stop, speak a note,
// add a screen, project, make today's capsules), Capsules, Projects, Settings (setup and preferences) and Help.
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
    /// For the personal Home.
    @Published var name = ""
    @Published var today = WorkStats.Totals()
    @Published var week = WorkStats.Totals()
    @Published var weekCapsules = 0
    @Published var latestCapsule: CapsuleSummary?

    var toggleRecord: () -> Void = {}
    var toggleVoice: () -> Void = {}
    var addScreen: () -> Void = {}
    var makeCapsules: () -> Void = {}
    var pickProject: (String?) -> Void = { _ in }
    var newProject: () -> Void = {}
    var askAccessibility: () -> Void = {}

    var projectName: String? { projectID.flatMap { id in projects.first { $0.id == id }?.name ?? id } }
}

enum Section: String, CaseIterable, Identifiable {
    case home, capsules, projects, settings, help
    var id: String { rawValue }
    var title: String {
        switch self {
        case .home: return "Home"
        case .capsules: return "Capsules"
        case .projects: return "Projects"
        case .settings: return "Settings"
        case .help: return "Help"
        }
    }
    var icon: String {
        switch self {
        case .home: return "house"
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

    var body: some View {
        NavigationSplitView {
            List(selection: $nav.section) {
                ForEach(Section.allCases) { s in
                    Label(s.title, systemImage: s.icon).tag(s)
                }
            }
            .navigationSplitViewColumnWidth(min: 200, ideal: 220, max: 300)
        } detail: {
            // The content widens with the window up to a comfortable reading width, and stays centred;
            // on wide windows Home puts Today and This week side by side.
            GeometryReader { geo in
                ScrollView {
                    VStack(alignment: .leading, spacing: 24) {
                        switch nav.section ?? .home {
                        case .home: HomeView(app: app, nav: nav, wide: geo.size.width > 860)
                        case .capsules: CapsulesView(library: library, app: app)
                        case .projects: ProjectsView(library: library)
                        case .settings: SettingsView(status: setup, prefs: prefs, app: app)
                        case .help: HelpView()
                        }
                    }
                    .padding(.horizontal, 32)
                    .padding(.vertical, 28)
                    .frame(maxWidth: geo.size.width > 860 ? 920 : 760, alignment: .leading)
                    .frame(maxWidth: .infinity)
                }
            }
        }
        .frame(minWidth: 800, minHeight: 540)
        .onAppear { library.reload() }
    }
}

// MARK: Home

private struct Title: View {
    let text: String
    var body: some View { Text(text).font(.system(size: 22, weight: .semibold)) }
}

private struct Caption: View {
    let text: String
    var body: some View { Text(text).font(.system(size: 12, weight: .medium)).foregroundStyle(.secondary).padding(.leading, 4) }
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

private struct HomeView: View {
    @ObservedObject var app: AppModel
    @ObservedObject var nav: Navigation
    var wide = false

    private var greeting: String {
        let hello = WorkStats.greeting(hour: Calendar.current.component(.hour, from: Date()))
        return app.name.isEmpty ? hello : "\(hello), \(app.name)"
    }

    private func projectName(_ id: String) -> String {
        id.isEmpty ? "Not sorted yet" : app.projects.first { $0.id == id }?.name ?? id
    }

    private func bars(_ t: WorkStats.Totals) -> some View {
        VStack(spacing: 10) {
            ForEach(t.byProject.prefix(5), id: \.id) { p in
                TimeBar(name: projectName(p.id), seconds: p.seconds, total: t.byProject.first?.seconds ?? 1, muted: p.id.isEmpty)
            }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(greeting).font(.system(size: 24, weight: .semibold))
            Text(Date().formatted(.dateTime.weekday(.wide).day().month(.wide)))
                .font(.system(size: 13)).foregroundStyle(.secondary)
        }

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
                    Text("\(app.moments) moment\(app.moments == 1 ? "" : "s") saved"
                         + (app.screens > 1 ? " · \(app.screens) screens shared" : ""))
                        .font(.system(size: 13)).foregroundStyle(.secondary)
                    HStack(spacing: 10) {
                        Button(app.listening ? "Done speaking" : "Speak a note", action: app.toggleVoice)
                        if app.hasSeveralScreens {
                            Button("Add a screen", action: app.addScreen).disabled(!app.canAddScreen)
                        }
                    }
                } else {
                    Text("⌥C to start. Pick the screen or window you work on.")
                        .font(.system(size: 13)).foregroundStyle(.secondary)
                }
            }
            .padding(18)
        }

        // Projects as quick buttons: one click to say what you're working on.
        VStack(alignment: .leading, spacing: 8) {
        Caption(text: "Working on")
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                chip("Find by itself", selected: !app.projectPinned) { app.pickProject(nil) }
                ForEach(app.projects, id: \.id) { p in
                    chip(p.name, selected: app.projectPinned && app.projectID == p.id) { app.pickProject(p.id) }
                }
                Button { app.newProject() } label: { Image(systemName: "plus") }
                    .buttonStyle(.bordered).buttonBorderShape(.capsule).help("New project from this window")
            }
            .padding(.horizontal, 2)
        }
        if !app.projectPinned, let name = app.projectName, app.recording {
            Text("Now: \(name)").font(.system(size: 12)).foregroundStyle(.secondary).padding(.leading, 4)
        }
        }

        // Each caption sits right above its panel; the space is between sections.
        if wide {
            HStack(alignment: .top, spacing: 16) {
                VStack(alignment: .leading, spacing: 8) { todayBlock }.frame(maxWidth: .infinity)
                VStack(alignment: .leading, spacing: 8) { weekBlock }.frame(maxWidth: .infinity)
            }
        } else {
            VStack(alignment: .leading, spacing: 8) { todayBlock }
            VStack(alignment: .leading, spacing: 8) { weekBlock }
        }

        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Caption(text: "Latest capsule")
                Spacer()
                if app.latestCapsule != nil { Button("See all") { nav.section = .capsules }.buttonStyle(.borderless).font(.system(size: 12)) }
            }
            Panel {
                if let c = app.latestCapsule {
                    CapsuleRow(capsule: c)
                } else {
                    Text("None yet.").font(.system(size: 13)).foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading).padding(16)
                }
            }
        }

        Panel {
            HStack(spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("Make today's capsules").font(.system(size: 14, weight: .medium))
                    Text("Opens Claude: ⌘V, then Return").font(.system(size: 12)).foregroundStyle(.secondary)
                }
                Spacer()
                Button("Make", action: app.makeCapsules)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
        }
    }

    @ViewBuilder private var todayBlock: some View {
        Caption(text: "Today" + (app.today.seconds > 0 ? " · \(WorkStats.duration(app.today.seconds))" : ""))
        Panel {
            VStack(alignment: .leading, spacing: 0) {
                if app.today.byProject.isEmpty {
                    Text("Nothing yet today. Start contextualizing when you begin work.")
                        .font(.system(size: 13)).foregroundStyle(.secondary)
                } else {
                    bars(app.today)
                }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
        }

    }

    @ViewBuilder private var weekBlock: some View {
        Caption(text: "This week" + (app.week.seconds > 0 ? " · \(WorkStats.duration(app.week.seconds))" : "")
                + (app.weekCapsules > 0 ? " · \(app.weekCapsules) capsule\(app.weekCapsules == 1 ? "" : "s")" : ""))
        Panel {
            VStack(alignment: .leading, spacing: 0) {
                if app.week.byProject.isEmpty {
                    Text("Your week shows up here.").font(.system(size: 13)).foregroundStyle(.secondary)
                } else {
                    bars(app.week)
                }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
        }

    }

    private func chip(_ title: String, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) { Text(title).lineLimit(1) }
            .buttonStyle(.bordered)
            .buttonBorderShape(.capsule)
            .tint(selected ? .accentColor : nil)
            .overlay(Capsule().strokeBorder(selected ? Color.accentColor : .clear, lineWidth: 1.5))
    }
}

// MARK: Capsules and Projects

private struct CapsulesView: View {
    @ObservedObject var library: LibraryModel
    @ObservedObject var app: AppModel

    var body: some View {
        HStack {
            Title(text: "Capsules")
            Spacer()
            Button("Make today's capsules", action: app.makeCapsules)
        }
        if library.capsules.isEmpty {
            Text("No capsules yet. Start contextualizing (⌥C), then make today's capsules.")
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

private struct ProjectsView: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        Title(text: "Projects")
        Text("Work is sorted into these by itself, from words in window names and web addresses.")
            .font(.system(size: 13)).foregroundStyle(.secondary)
        if library.projects.isEmpty {
            Text("No projects yet. Claude makes them when it sorts your day.")
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

    func load() {
        let c = Config.load(from: url)
        remind = c.remindAt != nil
        if let at = c.remindAt, let m = Reminder.parse(at) {
            remindTime = Calendar.current.date(bySettingHour: m / 60, minute: m % 60, second: 0, of: Date()) ?? remindTime
        }
        keepDays = c.keepDays
        sounds = c.sounds
        voiceLocale = c.voiceLocale ?? ""
        neverRecord = c.excludeApps
        name = Self.name(from: url)
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
            StepRow(icon: "link", done: status.claude, title: "Connect Claude",
                    help: "Lets Claude start contextualizing and write your capsules.",
                    button: "Connect") { ClaudeConnect.connect() }
            Divider().padding(.leading, 48)
            StepRow(icon: "power", done: status.atLogin, title: "Open at login",
                    help: "Always ready in the menu bar.",
                    button: "Turn On") { Setup.setLaunchAtLogin(true); status.refresh() }
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
    }
}

// MARK: Help

private struct HelpView: View {
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
            line("⌥ C", "Start or stop contextualizing")
            Divider().padding(.leading, 14)
            line("⌥ V", "Speak a note, then again when done")
            Divider().padding(.leading, 14)
            line("in Claude", "Make my capsules for today")
        }
        Caption(text: "How it works")
        Panel {
            VStack(alignment: .leading, spacing: 8) {
                Text("While you work, Context Capsule saves the text of the screen you picked, and what you say in voice notes.")
                Text("At the end of the day, Claude sorts it into projects and writes a capsule for each: what you did, what you decided and why, and what's next.")
                Text("Send a capsule to anyone. Their Claude reads it and can carry on your work.")
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
                Text("Text goes to Claude only when you ask Claude to make or read a capsule.")
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
    let setup = SetupStatus()
    let prefs: Preferences
    private var window: NSWindow?

    init(app: AppModel, paths: Paths) {
        self.app = app
        library = LibraryModel(paths: paths)
        prefs = Preferences(url: paths.config)
        self.paths = paths
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
        app.latestCapsule = capsules.first
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
        if let window {
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            return
        }
        let view = MainView(app: app, library: library, nav: nav, setup: setup, prefs: prefs)
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
