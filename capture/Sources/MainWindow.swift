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
            .navigationSplitViewColumnWidth(min: 170, ideal: 180, max: 220)
        } detail: {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    switch nav.section ?? .home {
                    case .home: HomeView(app: app)
                    case .capsules: CapsulesView(library: library, app: app)
                    case .projects: ProjectsView(library: library)
                    case .settings: SettingsView(status: setup, prefs: prefs, app: app)
                    case .help: HelpView()
                    }
                }
                .padding(28)
                .frame(maxWidth: 640, alignment: .leading)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .frame(minWidth: 760, minHeight: 540)
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

private struct HomeView: View {
    @ObservedObject var app: AppModel

    var body: some View {
        Title(text: "Home")

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
                    Text("Shortcut: ⌥C. Pick the screen or window you work on.")
                        .font(.system(size: 13)).foregroundStyle(.secondary)
                }
            }
            .padding(18)
        }

        Caption(text: "Project")
        Panel {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text(app.projectName ?? (app.recording ? "Not sure yet" : "Found by itself"))
                        .font(.system(size: 14, weight: .medium))
                    Text(app.projectPinned ? "You picked it" : "Found from what's on screen")
                        .font(.system(size: 12)).foregroundStyle(.secondary)
                }
                Spacer()
                Menu("Change") {
                    Button("Find by itself") { app.pickProject(nil) }
                    if !app.projects.isEmpty { Divider() }
                    ForEach(app.projects, id: \.id) { p in Button(p.name) { app.pickProject(p.id) } }
                    Divider()
                    Button("New project from this window", action: app.newProject)
                }
                .fixedSize()
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 12)
        }

        Caption(text: "Capsules")
        Panel {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Make today's capsules").font(.system(size: 14, weight: .medium))
                    Text("Opens Claude. Press ⌘V, then Return.").font(.system(size: 12)).foregroundStyle(.secondary)
                }
                Spacer()
                Button("Make", action: app.makeCapsules)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 12)
        }
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
    }

    func show(_ section: Section? = nil) {
        if let section { nav.section = section }
        library.reload()
        prefs.load()
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
        window = nil
        NSApp.setActivationPolicy(.accessory) // back to menu bar only
    }
}
