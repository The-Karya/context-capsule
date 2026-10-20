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
    private let projectItem = NSMenuItem(title: "Project: Auto-detect", action: nil, keyEquivalent: "")
    private let voiceItem = NSMenuItem(title: "Add voice note", action: #selector(toggleVoice), keyEquivalent: "v")
    private let axItem = NSMenuItem(title: "Read text directly (Accessibility)…", action: #selector(askAccessibility),
                                    keyEquivalent: "")
    private var hotKey: HotKey?
    private var voiceKey: HotKey?
    private lazy var voice = VoiceNotes(engine: SpeechEngine())
    private let toast = Toast()
    private let welcome = WelcomeWindow()
    private lazy var daily = DailyJobs(paths: paths) { [weak self] in self?.recorder.isRecording ?? false }
    private lazy var library: LibraryWindow = {
        let window = LibraryWindow(paths: paths)
        window.model.onProjectsChanged = { [weak self] in
            self?.recorder.reloadProjects(force: true)
            self?.refresh()
        }
        return window
    }()
    /// Stays up the whole time a voice note listens, so it's always clear the mic is on.
    private let listenToast = Toast()
    private var wasRecording = false
    private var wasListening = false
    private var control: ControlWatcher?
    private var askedAccessibility = false
    private var trustPoll: Timer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        try? FileManager.default.createDirectory(at: paths.sessions, withIntermediateDirectories: true)

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
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
        projectItem.submenu = NSMenu()
        menu.addItem(projectItem)
        menu.addItem(.separator())
        menu.addItem(NSMenuItem(title: "Make today's capsules", action: #selector(makeCapsules), keyEquivalent: ""))
        menu.addItem(NSMenuItem(title: "My capsules…", action: #selector(showLibrary), keyEquivalent: ""))
        menu.addItem(.separator())
        menu.addItem(NSMenuItem(title: "Set up Context Capsule…", action: #selector(showWelcome), keyEquivalent: ""))
        menu.addItem(.separator())
        menu.addItem(NSMenuItem(title: "Quit Context Capsule", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        for item in menu.items where item.action != nil && item.action != #selector(NSApplication.terminate(_:)) {
            item.target = self
        }
        menu.delegate = self
        statusItem.menu = menu

        recorder.onChange = { [weak self] in self?.refresh() }
        recorder.onEmptySession = { [weak self] in self?.warnEmpty($0) }
        recorder.willStop = { [weak self] in self?.voice.recordingWillStop() }
        recorder.onProjectChange = { [weak self] project in
            guard let self else { return }
            self.toast.show("Project: \(project?.name ?? "not sure yet")", under: self.statusItem.button, seconds: 3)
        }
        recorder.reloadProjects(force: true)
        recorder.onBusyListening = { [weak self] in
            guard let self else { return }
            self.toast.show("Finish the voice note first: press ⌥V. Then ⌥C stops recording.", under: self.statusItem.button, seconds: 4)
        }
        recorder.onAskProject = { [weak self] window in self?.askProject(window) }
        voice.onIndicator = { [weak self] on in
            self?.recorder.listening = on
            self?.refresh()
        }
        voice.onOutcome = { [weak self] outcome in
            guard let self else { return }
            self.refresh()
            self.toast.show(outcome.message, under: self.statusItem.button, seconds: {
                if case .saved = outcome { return 2 }
                return 6
            }())
        }

        hotKey = HotKey(keyCode: kVK_ANSI_C, modifiers: optionKey, id: 1) { [weak self] in self?.toggle() }
        if hotKey == nil { NSLog("Context Capsule: ⌥C is taken by another app; use the menu instead") }
        voiceKey = HotKey(keyCode: kVK_ANSI_V, modifiers: optionKey, id: 2) { [weak self] in self?.toggleVoice() }
        if voiceKey == nil { NSLog("Context Capsule: ⌥V is taken by another app; use the menu instead") }

        control = ControlWatcher(url: paths.control) { [weak self] cmd in
            switch cmd.kind {
            case .start: self?.recorder.start()
            case .stop:
                if self?.recorder.isRecording == true { AppLog.shared.write("stop: control file (Claude / MCP)") }
                self?.recorder.stop()
            case .note: if let text = cmd.text { self?.recorder.addTypedNote(text, at: cmd.at) }
            case .project: self?.recorder.pinProject(cmd.project == "auto" ? nil : cmd.project)
            }
        }

        recorder.writeState()
        refresh()

        welcome.onAccessibility = { [weak self] in self?.askAccessibility() }
        if !WelcomeWindow.seen { welcome.show() }
        daily.onMake = { [weak self] in self?.makeCapsules() }
        daily.start()

        // For testing: open a window at launch.
        if CommandLine.arguments.contains("--show-library") { library.show() }
        if CommandLine.arguments.contains("--show-welcome") { welcome.show() }
        if CommandLine.arguments.contains("--show-reminder") { daily.show() }
    }

    @objc private func showWelcome() { welcome.show() }

    @objc private func showLibrary() { library.show() }

    /// Opens the Claude app with "Make my capsules for today." typed in; stop recording first if it's on.
    @objc private func makeCapsules() {
        if recorder.isRecording && !recorder.listening {
            AppLog.shared.write("stop: make today's capsules")
            recorder.stop()
        }
        let connected = ClaudeConnect.isConnected
        Claude.makeTodaysCapsules()
        if connected { toast.show("In Claude, press Return to make your capsules.", under: statusItem.button, seconds: 5) }
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
        let symbol = listening ? "mic.fill" : recording ? "circle.fill" : "circle"
        let label = listening ? "Context Capsule: listening" : recording ? "Context Capsule: recording" : "Context Capsule"
        var image = NSImage(systemSymbolName: symbol, accessibilityDescription: label)
        if recording || listening {
            // Red while recording or listening, so it's obvious at a glance.
            image = image?.withSymbolConfiguration(NSImage.SymbolConfiguration(paletteColors: [.systemRed]))
            image?.isTemplate = false
        } else {
            image?.isTemplate = true
        }
        statusItem.button?.image = image
        announce(recording: recording, listening: listening)
        statusItem.button?.toolTip = listening ? "Listening — press ⌥V to finish the voice note"
            : recording ? "Context Capsule is recording (⌥C to stop, ⌥V for a voice note)" : "Context Capsule (⌥C to record)"
        voiceItem.title = listening ? "Finish voice note" : "Add voice note"
        buildProjectMenu()

        if let id = recorder.sessionID, let started = recorder.startedAt {
            let time = DateFormatter.localizedString(from: started, dateStyle: .none, timeStyle: .short)
            let n = recorder.eventCount
            infoItem.title = "Recording since \(time) — \(n) event\(n == 1 ? "" : "s") — \(id)"
        } else if recorder.isPicking {
            infoItem.title = "Pick a window or screen to record…"
        } else {
            infoItem.title = "Not recording"
        }
        skipItem.isHidden = !(recording && recorder.lastSkip != nil)
        skipItem.title = "Skipping: \(recorder.lastSkip?.message ?? "")"
        toggleItem.title = recording ? "Stop recording" : recorder.isPicking ? "Cancel picking" : "Start recording…"
        axItem.state = AXReader.trusted ? .on : .off
    }

    /// "Project: <name>" with Auto-detect, every project, and "New project from this window…".
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
        let new = NSMenuItem(title: "New project from this window…", action: #selector(newProject), keyEquivalent: "")
        new.target = self
        sub.addItem(new)
    }

    @objc private func pickProject(_ sender: NSMenuItem) {
        recorder.pinProject(sender.representedObject as? String)
        refresh()
    }

    /// "What are you working on?": pick a project or type a new name (prefilled from the window); the window's
    /// clue is remembered so it's found automatically next time. "Not sure" leaves it for Claude to sort.
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

    /// A clear message whenever recording or listening starts or stops.
    private func announce(recording: Bool, listening: Bool) {
        if recording != wasRecording {
            wasRecording = recording
            if recording {
                toast.show("● Recording started. Press ⌥C to stop, ⌥V for a voice note.", under: statusItem.button, seconds: 4)
            } else {
                let n = recorder.eventCount
                toast.show("■ Recording stopped: \(n) event\(n == 1 ? "" : "s") saved.", under: statusItem.button, seconds: 4)
            }
        }
        if listening != wasListening {
            wasListening = listening
            if listening {
                listenToast.show("🎙 Listening… press ⌥V to finish", under: statusItem.button, seconds: Voice.maxSeconds + 5)
            } else {
                listenToast.hideNow()
            }
        }
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
        alert.messageText = "Nothing was recorded"
        let reason = why.map { "Every few seconds, \($0.message)." } ?? "The session was too short to take a sample."
        alert.informativeText = reason + "\n\nContext Capsule only reads the window in front, and only if it is "
            + "part of the window or screen you picked. Next time, pick the screen you work on (or the window), "
            + "and keep that window in front."
        NSApp.activate(ignoringOtherApps: true)
        alert.runModal()
    }

    /// Optional. First click: the system Accessibility prompt. Later clicks: open System Settings and
    /// explain. Recording never waits for it; the check mark updates by itself once allowed.
    @objc private func askAccessibility() {
        if AXReader.trusted { refresh(); return }
        if !askedAccessibility {
            askedAccessibility = true
            AXReader.requestTrust()
        } else {
            if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") {
                NSWorkspace.shared.open(url)
            }
            let alert = NSAlert()
            alert.messageText = "Allow Context Capsule in Accessibility"
            alert.informativeText = "In System Settings → Privacy & Security → Accessibility, turn on Context Capsule. "
                + "If it is already on but this item has no check mark (this happens after a rebuild), select it, "
                + "click −, then add it again with +."
            NSApp.activate(ignoringOtherApps: true)
            alert.runModal()
        }
        trustPoll?.invalidate()
        var tries = 0
        trustPoll = repeating(1) { [weak self] in
            tries += 1
            if AXReader.trusted || tries > 120 {
                self?.trustPoll?.invalidate()
                self?.refresh()
            }
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

        // Under the status item, kept on screen.
        var origin = NSPoint(x: 100, y: 100)
        if let button, let window = button.window, let screen = window.screen {
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
