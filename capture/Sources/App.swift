// context-capsule — capture: the menu-bar app — status icon, menu, ⌥C / ⌥V hotkeys, voice notes, optional Accessibility.

import AppKit
import Carbon.HIToolbox

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private let paths = Paths.standard
    private lazy var recorder = Recorder(paths: paths)
    private var statusItem: NSStatusItem!
    private let infoItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
    private let skipItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
    private let toggleItem = NSMenuItem(title: "", action: #selector(toggle), keyEquivalent: "c")
    private let addScreenItem = NSMenuItem(title: "Add a screen", action: #selector(addScreen), keyEquivalent: "")
    private let projectItem = NSMenuItem(title: "Project: Auto-detect", action: nil, keyEquivalent: "")
    private let voiceItem = NSMenuItem(title: "Speak a note", action: #selector(toggleVoice), keyEquivalent: "v")
    private let axItem = NSMenuItem(title: "Read text directly (Accessibility)…", action: #selector(askAccessibility),
                                    keyEquivalent: "")
    private var hotKey: HotKey?
    private var voiceKey: HotKey?
    private let speech = SpeechEngine()
    private lazy var voice = VoiceNotes(engine: speech)
    /// Redraws the menu-bar icon while recording (the timer) or listening (the voice bars).
    private var iconTimer: Timer?
    private let toast = Toast()
    private lazy var daily = DailyJobs(paths: paths) { [weak self] in self?.recorder.isRecording ?? false }
    private let appModel = AppModel()
    private lazy var main: MainWindow = {
        let window = MainWindow(app: appModel, paths: paths)
        window.library.onProjectsChanged = { [weak self] in
            self?.recorder.reloadProjects(force: true)
            self?.refresh()
        }
        return window
    }()
    private var wasRecording = false
    private var wasListening = false
    private var control: ControlWatcher?
    private lazy var updater = Updater(paths: paths) { [weak self] in
        guard let self else { return false }
        return self.recorder.isRecording || self.recorder.listening || self.recorder.isPicking
    }
    private let updatePrompt = UpdatePrompt()
    private let updateItem = NSMenuItem(title: "Check for Updates", action: #selector(updateClicked), keyEquivalent: "")
    /// Shown when Your AI is a website or any other AI: save the capsules in its copied reply.
    private let saveReplyItem = NSMenuItem(title: "Save from clipboard", action: #selector(saveFromClipboard), keyEquivalent: "")

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Run from the disk image or Downloads: offer once to move to Applications (it then reopens from there).
        if MoveToApplications.offerIfNeeded() { return }
        try? FileManager.default.createDirectory(at: paths.sessions, withIntermediateDirectories: true)

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        let menu = NSMenu()
        infoItem.isEnabled = false
        skipItem.isEnabled = false
        toggleItem.keyEquivalentModifierMask = [.option]
        voiceItem.keyEquivalentModifierMask = [.option]
        menu.addItem(infoItem)
        menu.addItem(skipItem)
        menu.addItem(.separator())
        menu.addItem(toggleItem)
        menu.addItem(voiceItem)
        menu.addItem(addScreenItem)
        projectItem.submenu = NSMenu()
        menu.addItem(projectItem)
        menu.addItem(.separator())
        menu.addItem(NSMenuItem(title: "Open \(AppName.full)", action: #selector(showMain), keyEquivalent: "o"))
        menu.addItem(NSMenuItem(title: "Make today's capsules", action: #selector(makeCapsules), keyEquivalent: ""))
        menu.addItem(saveReplyItem)
        menu.addItem(.separator())
        menu.addItem(updateItem)
        menu.addItem(NSMenuItem(title: "Quit \(AppName.full)", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        for item in menu.items where item.action != nil && item.action != #selector(NSApplication.terminate(_:)) {
            item.target = self
        }
        menu.delegate = self
        statusItem.menu = menu

        recorder.onChange = { [weak self] in self?.refresh() }
        recorder.onEmptySession = { [weak self] in self?.warnEmpty($0) }
        recorder.willStop = { [weak self] in self?.voice.recordingWillStop() }
        // Routine changes (project, screens) show in the menu and the window, never as text on screen.
        recorder.onProjectChange = { [weak self] _ in self?.refresh() }
        recorder.reloadProjects(force: true)
        recorder.onScreens = { [weak self] count, added in
            guard let self else { return }
            self.refresh()
        }
        Share.toast = { [weak self] text in
            guard let self else { return }
            self.toast.show(text, under: self.statusItem.button, seconds: 4)
        }
        let ai = AIState.shared
        ai.configURL = paths.config
        ai.toast = { [weak self] text in
            guard let self else { return }
            self.toast.show(text, under: self.statusItem.button, seconds: 8)
        }
        ai.onSaved = { [weak self] in
            guard let self else { return }
            self.main.library.reload()
            self.main.refreshStats()
        }
        ai.onChange = { [weak self] in self?.refresh() }
        ai.refresh()
        recorder.onNeedsScreenRecording = { [weak self] in self?.askScreenRecording() }
        recorder.onAlreadyShared = { [weak self] in
            guard let self else { return }
            self.toast.show("That screen is already shared. Pick a different one.", under: self.statusItem.button, seconds: 4)
        }
        recorder.onBusyListening = { [weak self] in
            guard let self else { return }
            let keys = Shortcut.current
            self.toast.show("Finish the voice note first: press \(keys.voice.symbols). Then \(keys.record.symbols) stops contextualizing.",
                            under: self.statusItem.button, seconds: 4)
        }
        recorder.onAskProject = { [weak self] window in self?.askProject(window) }
        voice.onIndicator = { [weak self] on in
            self?.recorder.listening = on
            self?.refresh()
        }
        voice.onOutcome = { [weak self] outcome in
            guard let self else { return }
            self.refresh()
            if case .saved = outcome { return } // the sound says it; only problems are shown
            self.toast.show(outcome.message, under: self.statusItem.button, seconds: 6)
        }

        Shortcut.current = Shortcut.load(from: paths.config)
        registerHotKeys()

        control = ControlWatcher(url: paths.control) { [weak self] cmd in
            switch cmd.kind {
            case .start: self?.recorder.start()
            case .stop:
                if self?.recorder.isRecording == true { AppLog.shared.write("stop: control file (your AI / MCP)") }
                self?.recorder.stop()
            case .note: if let text = cmd.text { self?.recorder.addTypedNote(text, at: cmd.at) }
            case .project: self?.recorder.pinProject(cmd.project == "auto" ? nil : cmd.project)
            }
        }

        recorder.writeState()
        refresh()

        // Open at login by default, so people never forget to start the app. Once only: if they turn it off
        // (Setup or System Settings → General → Login Items), it stays off.
        if !UserDefaults.standard.bool(forKey: "loginItemSet") {
            UserDefaults.standard.set(true, forKey: "loginItemSet")
            Setup.setLaunchAtLogin(true)
        }

        appModel.toggleRecord = { [weak self] in self?.toggle() }
        appModel.toggleVoice = { [weak self] in self?.toggleVoice() }
        appModel.addScreen = { [weak self] in self?.addScreen() }
        appModel.makeCapsules = { [weak self] in self?.makeCapsules() }
        appModel.pickProject = { [weak self] id in self?.recorder.pinProject(id); self?.refresh() }
        appModel.newProject = { [weak self] in self?.newProject() }
        appModel.askAccessibility = { [weak self] in self?.askAccessibility() }
        appModel.setShortcut = { [weak self] record, keys in self?.setShortcut(record: record, to: keys) }
        appModel.resetShortcuts = { [weak self] in self?.resetShortcuts() }
        appModel.pauseHotKeys = { [weak self] paused in
            guard let self else { return }
            if paused { self.hotKey = nil; self.voiceKey = nil } else { self.registerHotKeys() }
        }
        // First launch: the window opens on Settings, to set things up.
        if !MainWindow.seen {
            UserDefaults.standard.set(true, forKey: MainWindow.seenKey)
            main.show(.settings)
        }
        daily.onMake = { [weak self] in self?.makeCapsules() }
        daily.start()

        appModel.version = Updater.currentVersion
        appModel.checkForUpdates = { [weak self] in self?.updater.check(manual: true) }
        appModel.restartToUpdate = { [weak self] in self?.restartToUpdate() }
        updater.onChange = { [weak self] in self?.refresh() }
        updater.onReady = { [weak self] version in
            guard let self else { return }
            self.updatePrompt.show(version: version) { [weak self] in self?.restartToUpdate() }
        }
        updater.start()

        // Notice when Accessibility is switched on or off (no event for it): log it and say so.
        lastTrusted = AXReader.trusted
        AppLog.shared.write("accessibility: \(lastTrusted ? "on" : "off") at launch")
        trustWatch = repeating(2) { [weak self] in self?.checkTrust() }

        // For testing: open a window at launch.
        if CommandLine.arguments.contains("--show-main") { main.show(.home) }
        if CommandLine.arguments.contains("--show-library") { main.show(.capsules) }
        if CommandLine.arguments.contains("--show-welcome") { main.show(.settings) }
        if CommandLine.arguments.contains("--show-reminder") { daily.show() }
        // --show-timeline [YYYY-MM-DD]: the Timeline, on that day if one is given.
        if let i = CommandLine.arguments.firstIndex(of: "--show-timeline") {
            main.show(.timeline)
            let f = DateFormatter()
            f.locale = Locale(identifier: "en_US_POSIX")
            f.dateFormat = "yyyy-MM-dd"
            if i + 1 < CommandLine.arguments.count, let day = f.date(from: CommandLine.arguments[i + 1]) { main.timeline.show(day) }
        }
    }

    private var lastTrusted = false
    private var trustWatch: Timer?

    private func checkTrust() {
        let now = AXReader.trusted
        guard now != lastTrusted else { return }
        lastTrusted = now
        AppLog.shared.write("accessibility: turned \(now ? "on" : "off")")
        refresh() // Settings shows the ✓
    }

    @objc private func showMain() { main.show(.home) }

    /// "Check for Updates", or "Restart to Update" once a new version is ready.
    @objc private func updateClicked() {
        if updater.readyVersion != nil { restartToUpdate(); return }
        updater.check(manual: true) { [weak self] text in
            guard let self, self.updater.readyVersion == nil else { return }
            self.toast.show(text, under: self.statusItem.button, seconds: 4)
        }
    }

    private func restartToUpdate() {
        if let problem = updater.restartNow() { toast.show(problem, under: statusItem.button, seconds: 5) }
    }

    @objc private func addScreen() { recorder.addScreen() }



    /// Make today's capsules with your AI (Claude with the request typed in, another app with it on the clipboard,
    /// or Copy my day); stop recording first if it's on.
    @objc private func makeCapsules() {
        if recorder.isRecording && !recorder.listening {
            AppLog.shared.write("stop: make today's capsules")
            recorder.stop()
        }
        AIState.shared.makeTodaysCapsules()
    }

    @objc private func saveFromClipboard() { AIState.shared.saveFromClipboard() }

    /// A capsule someone sent, opened with Context Capsule (Finder → Open With): your AI carries on from it.
    /// At launch this can come before applicationDidFinishLaunching, so it waits a turn.
    func application(_ application: NSApplication, open urls: [URL]) {
        let files = urls.filter(\.isFileURL)
        DispatchQueue.main.async { files.forEach(Claude.continueFrom) }
    }

    func menuWillOpen(_ menu: NSMenu) { refresh() } // the Accessibility check mark may have changed

    func applicationWillTerminate(_ notification: Notification) {
        if recorder.isRecording { AppLog.shared.write("stop: quit") }
        recorder.stop(wait: true)
        AppLog.shared.flush()
        recorder.writeState()
    }

    private func refresh() {
        let recording = recorder.isRecording
        let listening = recorder.listening
        drawIcon()
        if (recording || listening) && iconTimer == nil {
            iconTimer = repeating(1.0 / 12) { [weak self] in self?.drawIcon() }
        } else if !recording && !listening {
            iconTimer?.invalidate()
            iconTimer = nil
        }
        announce(recording: recording, listening: listening)
        let (rk, vk) = (Shortcut.current.record.symbols, Shortcut.current.voice.symbols)
        statusItem.button?.toolTip = listening ? "Listening — press \(vk) to finish the voice note"
            : recording ? "Contextualizing (\(rk) to stop, \(vk) to talk)" : "\(AppName.full) (\(rk) to start contextualizing)"
        voiceItem.title = listening ? "Done speaking" : "Speak a note"
        addScreenItem.isHidden = !recording || NSScreen.screens.count < 2
        addScreenItem.isEnabled = recorder.canAddScreen
        addScreenItem.title = recorder.sharedScreens > 1 ? "Add a screen (\(recorder.sharedScreens) shared)" : "Add a screen"
        buildProjectMenu()
        updateModel(recording: recording, listening: listening)

        if recorder.sessionID != nil, let started = recorder.startedAt {
            let time = DateFormatter.localizedString(from: started, dateStyle: .none, timeStyle: .short)
            let n = recorder.eventCount
            infoItem.title = "Contextualizing since \(time) — \(n) moment\(n == 1 ? "" : "s")"
        } else if recorder.isPicking {
            infoItem.title = "Pick a window or screen…"
        } else {
            infoItem.title = "Not contextualizing"
        }
        skipItem.isHidden = !(recording && recorder.lastSkip != nil)
        skipItem.title = "Skipping: \(recorder.lastSkip?.message ?? "")"
        toggleItem.title = recording ? "Stop contextualizing" : recorder.isPicking ? "Cancel" : "Start contextualizing"
        axItem.state = AXReader.trusted ? .on : .off
        updateItem.title = updater.readyVersion != nil ? "Restart to Update" : "Check for Updates"
        saveReplyItem.isHidden = !AIState.shared.kind.usesCopy
    }

    /// What the window shows (it updates live while open).
    // MARK: shortcuts

    /// Registers the shortcuts in Shortcut.current; one another app already has stays unregistered (logged).
    private func registerHotKeys() {
        hotKey = nil
        voiceKey = nil
        let keys = Shortcut.current
        hotKey = Self.hotKey(keys.record, id: 1) { [weak self] in self?.toggle() }
        if hotKey == nil { AppLog.shared.write("shortcut \(keys.record.text) is taken by another app; use the menu instead") }
        voiceKey = Self.hotKey(keys.voice, id: 2) { [weak self] in self?.toggleVoice() }
        if voiceKey == nil { AppLog.shared.write("shortcut \(keys.voice.text) is taken by another app; use the menu instead") }
        for (item, s) in [(toggleItem, keys.record), (voiceItem, keys.voice)] {
            item.keyEquivalent = KeyCodes.menuKey(s)
            item.keyEquivalentModifierMask = KeyCodes.menuModifiers(s)
        }
        appModel.recordKey = keys.record.symbols
        appModel.voiceKey = keys.voice.symbols
    }

    private static func hotKey(_ s: Shortcut, id: UInt32, action: @escaping () -> Void) -> HotKey? {
        guard let code = KeyCodes.code(s.key) else { return nil }
        return HotKey(keyCode: code, modifiers: KeyCodes.carbonModifiers(s), id: id, action: action)
    }

    /// Settings: use new keys for "Start or stop" (record) or "Speak a note". nil = done; else why not (old keys stay).
    private func setShortcut(record: Bool, to keys: Shortcut) -> String? {
        let current = Shortcut.current
        let other = record ? current.voice : current.record
        switch keys.problem(other: other) {
        case .needsModifier: return "Add ⌃, ⌥ or ⌘"
        case .sameAsOther: return "Already used for \(record ? "Speak a note" : "Start or stop")"
        case nil: break
        }
        // Free our own old keys first, then see if the new ones can be had.
        hotKey = nil
        voiceKey = nil
        let free = Self.hotKey(keys, id: 9) {} != nil // let go again at once
        guard free else {
            registerHotKeys()
            return "Another app uses these keys"
        }
        if record { Shortcut.current.record = keys } else { Shortcut.current.voice = keys }
        saveShortcuts()
        registerHotKeys()
        refresh()
        return nil
    }

    private func resetShortcuts() {
        Shortcut.current = (Shortcut.defaultRecord, Shortcut.defaultVoice)
        saveShortcuts()
        registerHotKeys()
        refresh()
    }

    private func saveShortcuts() {
        let keys = Shortcut.current
        let isDefault = keys.record == Shortcut.defaultRecord && keys.voice == Shortcut.defaultVoice
        do {
            try ConfigFile.set("hotkeys", isDefault ? nil : ["record": keys.record.text, "voice": keys.voice.text], in: paths.config)
            AppLog.shared.write("shortcuts: \(keys.record.text), \(keys.voice.text)")
        } catch {
            AppLog.shared.write("cannot save shortcuts: \(error.localizedDescription)")
        }
    }

    private func updateModel(recording: Bool, listening: Bool) {
        let m = appModel
        m.recording = recording
        m.listening = listening
        m.picking = recorder.isPicking
        m.startedAt = recorder.startedAt
        m.moments = recorder.eventCount
        m.projects = recorder.projects
        m.projectID = recorder.projectChoice.pinned ?? recorder.currentProject?.id
        m.projectPinned = recorder.projectChoice.pinned != nil
        m.screens = recorder.sharedScreens
        m.canAddScreen = recorder.canAddScreen
        m.hasSeveralScreens = NSScreen.screens.count > 1
        m.updateLine = updater.line
        m.updateReady = updater.readyVersion != nil
        m.updateWorking = updater.working
    }

    /// "Project: <name>" with Auto-detect, every project, and "New project from this window".
    private func buildProjectMenu() {
        recorder.reloadProjects()
        let pinned = recorder.projectChoice.pinned
        let current = recorder.currentProject.flatMap { c in recorder.projects.first { $0.id == c.id } }
        projectItem.title = pinned != nil ? "Project: \(current?.name ?? pinned!)"
            : "Project: Auto-detect" + (current.map { " (\($0.name))" } ?? "")
        guard let sub = projectItem.submenu else { return }
        sub.removeAllItems()
        let auto = NSMenuItem(title: "Auto-detect", action: #selector(pickProject(_:)), keyEquivalent: "")
        auto.target = self
        auto.state = pinned == nil ? .on : .off
        sub.addItem(auto)
        if !recorder.projects.isEmpty { sub.addItem(.separator()) }
        for project in recorder.projects {
            let item = NSMenuItem(title: project.name, action: #selector(pickProject(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = project.id
            item.state = pinned == project.id ? .on : .off
            sub.addItem(item)
        }
        sub.addItem(.separator())
        let new = NSMenuItem(title: "New project from this window", action: #selector(newProject), keyEquivalent: "")
        new.target = self
        sub.addItem(new)
    }

    @objc private func pickProject(_ sender: NSMenuItem) {
        recorder.pinProject(sender.representedObject as? String)
        refresh()
    }

    /// "What are you working on?": pick a project or type a new name (prefilled from the window); the window's
    /// clue is remembered so it's found automatically next time. "Not sure" leaves it for your AI to sort.
    private func askProject(_ window: (title: String, location: String, app: String)) {
        guard recorder.isRecording, !recorder.listening else { return }
        let clue = Projects.clue(title: window.title, location: window.location)
        let alert = NSAlert()
        alert.messageText = "What are you working on?"
        alert.informativeText = "Pick a project or type a new one. Context Capsule will remember this window"
            + (clue.map { " (“\($0)”)" } ?? "") + " and know it by itself next time."
        let box = NSComboBox(frame: NSRect(x: 0, y: 0, width: 280, height: 26))
        box.addItems(withObjectValues: recorder.projects.map(\.name))
        box.completes = true
        box.placeholderString = "Project name"
        box.stringValue = recorder.projects.isEmpty ? (clue ?? "") : ""
        alert.accessoryView = box
        alert.addButton(withTitle: "OK")
        alert.addButton(withTitle: "Not sure")
        NSApp.activate(ignoringOtherApps: true)
        alert.window.initialFirstResponder = box
        guard alert.runModal() == .alertFirstButtonReturn else {
            AppLog.shared.write("project question: not sure")
            return
        }
        recorder.answerProject(name: box.stringValue, window: window)
        refresh()
    }

    /// Ask for a name (prefilled from the window), remember the window's best clue, and switch to it.
    @objc private func newProject() {
        let window = recorder.currentWindow()
        let clue = window.flatMap { Projects.clue(title: $0.title, location: $0.location) }
        let alert = NSAlert()
        alert.messageText = "New project"
        alert.informativeText = clue.map { "Windows with “\($0)” will count as this project from now on." }
            ?? "Name the project. Pick it from this menu while you work on it."
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
        field.stringValue = clue ?? ""
        field.placeholderString = "Project name"
        alert.accessoryView = field
        alert.addButton(withTitle: "Create")
        alert.addButton(withTitle: "Cancel")
        NSApp.activate(ignoringOtherApps: true)
        alert.window.initialFirstResponder = field
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        let name = field.stringValue.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty, let project = recorder.createProject(name: name, clue: clue) else { return }
        // With a clue, auto-detect finds it from now on; without one, stick to it until changed.
        if clue == nil || recorder.projectChoice.pinned != nil { recorder.pinProject(project.id) }
        refresh()
    }

    /// Idle: the Karya panther (black or white with the menu bar). Recording: a red dot and the time ("● 12:34"). Listening: a red mic with bars
    /// that move with your voice. All in the menu bar; nothing on the screen.
    private func drawIcon() {
        guard let button = statusItem.button else { return }
        if recorder.listening {
            let image = MenuBarLook.listening(level: speech.currentLevel)
            image.isTemplate = false
            button.image = image
            button.title = ""
            button.setAccessibilityLabel("\(AppName.full): listening")
        } else if recorder.isRecording, let since = recorder.startedAt {
            let image = MenuBarLook.dot()
            image.isTemplate = false
            button.image = image
            button.imagePosition = .imageLeading
            let time = MenuBarLook.clock(Date().timeIntervalSince(since))
            if button.title != " " + time {
                button.attributedTitle = NSAttributedString(string: " " + time, attributes: [
                    .font: NSFont.monospacedDigitSystemFont(ofSize: NSFont.systemFontSize, weight: .medium),
                    .foregroundColor: NSColor.systemRed,
                ])
            }
            button.setAccessibilityLabel("\(AppName.full): contextualizing, \(time)")
        } else {
            button.image = MenuBarLook.idle()
            button.title = ""
            button.setAccessibilityLabel(AppName.full)
        }
    }

    /// Starting and stopping is a soft sound and the menu-bar icon: no text on screen.
    private func announce(recording: Bool, listening: Bool) {
        if listening != wasListening { listening ? Cue.micOpened() : Cue.micClosed() }
        if recording != wasRecording { recording ? Cue.recordingStarted() : Cue.recordingStopped() }
        wasRecording = recording
        wasListening = listening
    }

    /// "All my screens" without Screen Recording: ask once per launch; the picker opens meanwhile.
    private var askedScreenRecording = false
    private func askScreenRecording() {
        guard !askedScreenRecording else { return }
        askedScreenRecording = true
        _ = CGRequestScreenCaptureAccess()
        let alert = NSAlert()
        alert.messageText = "Allow Screen Recording"
        alert.informativeText = "To share all your screens at once, turn on \(AppName.full) in Screen Recording, then quit "
            + "and reopen it. For now, pick a screen."
        alert.addButton(withTitle: "Open Settings")
        alert.addButton(withTitle: "Not now")
        NSApp.activate(ignoringOtherApps: true)
        if alert.runModal() == .alertFirstButtonReturn { Setup.openSettings(Setup.screenRecordingPane) }
    }

    @objc private func toggle() { recorder.toggle() }

    /// ⌥V: start talking, press again to stop. Only while recording.
    @objc private func toggleVoice() { voice.toggle(locale: recorder.voiceLocale, target: recorder.noteTarget()) }

    @objc private func openSessions() {
        try? FileManager.default.createDirectory(at: paths.sessions, withIntermediateDirectories: true)
        NSWorkspace.shared.open(paths.sessions)
    }

    /// A session ended with no events: say why, so it is not a silent empty file.
    private func warnEmpty(_ why: Skip?) {
        let alert = NSAlert()
        alert.messageText = "Nothing was saved"
        let reason = why.map { "Every few seconds, \($0.message)." } ?? "It stopped too soon to save anything."
        alert.informativeText = reason + "\n\nContext Capsule only reads the window in front, and only if it is "
            + "part of the window or screen you picked. Next time, pick the screen you work on (or the window), "
            + "and keep that window in front."
        NSApp.activate(ignoringOtherApps: true)
        alert.runModal()
    }

    /// Optional. First click: the system Accessibility prompt. Later clicks: open System Settings and
    /// explain. Recording never waits for it; the check mark updates by itself once allowed.
    /// "Turn on" for reading apps. macOS remembers the permission per app signature, so an entry left from an
    /// older build looks switched on but doesn't count. Clear that first, then ask fresh and open the right page.
    @objc private func askAccessibility() {
        if AXReader.trusted { refresh(); return }
        let reset = Process()
        reset.executableURL = URL(fileURLWithPath: "/usr/bin/tccutil")
        reset.arguments = ["reset", "Accessibility", Bundle.main.bundleIdentifier ?? "com.thekarya.contextcapsule"]
        try? reset.run()
        reset.waitUntilExit()
        AppLog.shared.write("accessibility: asked (old entry cleared: \(reset.terminationStatus == 0 ? "yes" : "no"))")
        AXReader.requestTrust() // adds Context Capsule to the list and shows macOS's prompt
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") {
            NSWorkspace.shared.open(url)
        }
    }
}

/// A global hotkey through Carbon's RegisterEventHotKey (needs no extra permission).
final class HotKey {
    private var hotKeyRef: EventHotKeyRef?
    private var handlerRef: EventHandlerRef?
    private let action: () -> Void

    private let id: UInt32

    init?(keyCode: Int, modifiers: Int, id: UInt32, action: @escaping () -> Void) {
        self.action = action
        self.id = id
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let me = Unmanaged.passUnretained(self).toOpaque()
        let installed = InstallEventHandler(GetApplicationEventTarget(), { _, event, userData in
            guard let userData, let event else { return OSStatus(eventNotHandledErr) }
            let hotKey = Unmanaged<HotKey>.fromOpaque(userData).takeUnretainedValue()
            var pressed = EventHotKeyID()
            let got = GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID),
                                        nil, MemoryLayout<EventHotKeyID>.size, nil, &pressed)
            // Every hotkey's handler sees every press: act only on our own.
            guard got == noErr, pressed.id == hotKey.id else { return OSStatus(eventNotHandledErr) }
            DispatchQueue.main.async { hotKey.action() }
            return noErr
        }, 1, &spec, me, &handlerRef)
        guard installed == noErr else { return nil }

        let hotKeyID = EventHotKeyID(signature: OSType(0x4B43_4150), id: id) // 'KCAP'
        guard RegisterEventHotKey(UInt32(keyCode), UInt32(modifiers), hotKeyID, GetApplicationEventTarget(), 0, &hotKeyRef) == noErr
        else { return nil }
    }

    deinit {
        if let hotKeyRef { UnregisterEventHotKey(hotKeyRef) }
        if let handlerRef { RemoveEventHandler(handlerRef) }
    }
}

/// A small message under the menu-bar icon that fades by itself ("Voice note saved",
/// "Start recording first (⌥C)"). Needs no notification permission.
final class Toast {
    private var panel: NSPanel?
    private var hide: DispatchWorkItem?

    func hideNow() {
        hide?.cancel()
        panel?.orderOut(nil)
        panel = nil
    }

    func show(_ text: String, under button: NSStatusBarButton?, seconds: TimeInterval) {
        hide?.cancel()
        panel?.orderOut(nil)

        let label = NSTextField(wrappingLabelWithString: text)
        label.font = .systemFont(ofSize: 13)
        label.textColor = .labelColor
        label.preferredMaxLayoutWidth = 300
        let size = label.fittingSize
        let pad: CGFloat = 12
        let frame = NSRect(x: 0, y: 0, width: size.width + pad * 2, height: size.height + pad * 2)

        let panel = NSPanel(contentRect: frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.level = .statusBar
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.ignoresMouseEvents = true
        panel.collectionBehavior = [.canJoinAllSpaces, .transient]
        let box = NSVisualEffectView(frame: frame)
        box.material = .hudWindow
        box.state = .active
        box.wantsLayer = true
        box.layer?.cornerRadius = 10
        label.frame = NSRect(x: pad, y: pad, width: size.width, height: size.height)
        box.addSubview(label)
        panel.contentView = box

        // Under the status item, kept on screen. When the icon is hidden (a full menu bar, or behind the notch),
        // at the top of the screen, in the middle.
        var origin = NSPoint(x: 100, y: 100)
        if let screen = NSScreen.main {
            origin = NSPoint(x: screen.visibleFrame.midX - frame.width / 2, y: screen.visibleFrame.maxY - frame.height - 8)
        }
        if let button, let window = button.window, let screen = window.screen, window.isVisible {
            let icon = window.convertToScreen(button.convert(button.bounds, to: nil))
            origin = NSPoint(x: min(max(icon.midX - frame.width / 2, screen.visibleFrame.minX + 8),
                                    screen.visibleFrame.maxX - frame.width - 8),
                             y: icon.minY - frame.height - 6)
        }
        panel.setFrameOrigin(origin)
        panel.orderFrontRegardless()
        self.panel = panel

        let item = DispatchWorkItem { [weak self, weak panel] in
            panel?.orderOut(nil)
            if self?.panel === panel { self?.panel = nil }
        }
        hide = item
        DispatchQueue.main.asyncAfter(deadline: .now() + seconds, execute: item)
    }
}

/// Key names (see Shortcut.keys) ↔ Mac key codes, and modifiers for Carbon hotkeys and menus.
enum KeyCodes {
    static let table: [String: Int] = [
        "A": kVK_ANSI_A, "B": kVK_ANSI_B, "C": kVK_ANSI_C, "D": kVK_ANSI_D, "E": kVK_ANSI_E, "F": kVK_ANSI_F,
        "G": kVK_ANSI_G, "H": kVK_ANSI_H, "I": kVK_ANSI_I, "J": kVK_ANSI_J, "K": kVK_ANSI_K, "L": kVK_ANSI_L,
        "M": kVK_ANSI_M, "N": kVK_ANSI_N, "O": kVK_ANSI_O, "P": kVK_ANSI_P, "Q": kVK_ANSI_Q, "R": kVK_ANSI_R,
        "S": kVK_ANSI_S, "T": kVK_ANSI_T, "U": kVK_ANSI_U, "V": kVK_ANSI_V, "W": kVK_ANSI_W, "X": kVK_ANSI_X,
        "Y": kVK_ANSI_Y, "Z": kVK_ANSI_Z,
        "0": kVK_ANSI_0, "1": kVK_ANSI_1, "2": kVK_ANSI_2, "3": kVK_ANSI_3, "4": kVK_ANSI_4,
        "5": kVK_ANSI_5, "6": kVK_ANSI_6, "7": kVK_ANSI_7, "8": kVK_ANSI_8, "9": kVK_ANSI_9,
        "F1": kVK_F1, "F2": kVK_F2, "F3": kVK_F3, "F4": kVK_F4, "F5": kVK_F5, "F6": kVK_F6,
        "F7": kVK_F7, "F8": kVK_F8, "F9": kVK_F9, "F10": kVK_F10, "F11": kVK_F11, "F12": kVK_F12,
    ]

    static func code(_ key: String) -> Int? { table[key] }
    static func key(_ code: UInt16) -> String? { table.first { $0.value == Int(code) }?.key }

    static func carbonModifiers(_ s: Shortcut) -> Int {
        (s.ctrl ? controlKey : 0) | (s.alt ? optionKey : 0) | (s.shift ? shiftKey : 0) | (s.cmd ? cmdKey : 0)
    }

    /// The keys pressed in a key-down event, or nil when it isn't a key a shortcut can use.
    static func shortcut(from event: NSEvent) -> Shortcut? {
        guard let key = key(event.keyCode) else { return nil }
        let f = event.modifierFlags
        return Shortcut(ctrl: f.contains(.control), alt: f.contains(.option), shift: f.contains(.shift),
                        cmd: f.contains(.command), key: key)
    }

    static func menuKey(_ s: Shortcut) -> String {
        if s.isFunctionKey, let n = Int(s.key.dropFirst()), let scalar = UnicodeScalar(NSF1FunctionKey + n - 1) {
            return String(Character(scalar))
        }
        return s.key.lowercased()
    }

    static func menuModifiers(_ s: Shortcut) -> NSEvent.ModifierFlags {
        var f: NSEvent.ModifierFlags = []
        if s.ctrl { f.insert(.control) }
        if s.alt { f.insert(.option) }
        if s.shift { f.insert(.shift) }
        if s.cmd { f.insert(.command) }
        return f
    }
}
