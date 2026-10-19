// context-capsule — capture: the menu-bar app — status icon, menu, ⌥⇧C / ⌥⇧V hotkeys, voice notes, optional Accessibility.

import AppKit
import Carbon.HIToolbox

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private let paths = Paths.standard
    private lazy var recorder = Recorder(paths: paths)
    private var statusItem: NSStatusItem!
    private let infoItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
    private let skipItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
    private let toggleItem = NSMenuItem(title: "", action: #selector(toggle), keyEquivalent: "c")
    private let voiceItem = NSMenuItem(title: "Add voice note", action: #selector(toggleVoice), keyEquivalent: "v")
    private let axItem = NSMenuItem(title: "Read text directly (Accessibility)…", action: #selector(askAccessibility),
                                    keyEquivalent: "")
    private var hotKey: HotKey?
    private var voiceKey: HotKey?
    private lazy var voice = VoiceNotes(engine: SpeechEngine())
    private let toast = Toast()
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
        toggleItem.keyEquivalentModifierMask = [.option, .shift]
        voiceItem.keyEquivalentModifierMask = [.option, .shift]
        menu.addItem(infoItem)
        menu.addItem(skipItem)
        menu.addItem(.separator())
        menu.addItem(toggleItem)
        menu.addItem(voiceItem)
        menu.addItem(axItem)
        menu.addItem(NSMenuItem(title: "Open sessions folder", action: #selector(openSessions), keyEquivalent: ""))
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

        hotKey = HotKey(keyCode: kVK_ANSI_C, modifiers: optionKey | shiftKey, id: 1) { [weak self] in self?.toggle() }
        if hotKey == nil { NSLog("Context Capsule: ⌥⇧C is taken by another app; use the menu instead") }
        voiceKey = HotKey(keyCode: kVK_ANSI_V, modifiers: optionKey | shiftKey, id: 2) { [weak self] in self?.toggleVoice() }
        if voiceKey == nil { NSLog("Context Capsule: ⌥⇧V is taken by another app; use the menu instead") }

        control = ControlWatcher(url: paths.control) { [weak self] cmd in
            switch cmd.kind {
            case .start: self?.recorder.start()
            case .stop:
                if self?.recorder.isRecording == true { AppLog.shared.write("stop: control file (Claude / MCP)") }
                self?.recorder.stop()
            case .note: if let text = cmd.text { self?.recorder.addTypedNote(text, at: cmd.at) }
            }
        }

        recorder.writeState()
        refresh()
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
        statusItem.button?.toolTip = listening ? "Listening — press ⌥⇧V to finish the voice note"
            : recording ? "Context Capsule is recording (⌥⇧C to stop, ⌥⇧V for a voice note)" : "Context Capsule (⌥⇧C to record)"
        voiceItem.title = listening ? "Finish voice note" : "Add voice note"

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

    /// A clear message whenever recording or listening starts or stops.
    private func announce(recording: Bool, listening: Bool) {
        if recording != wasRecording {
            wasRecording = recording
            if recording {
                toast.show("● Recording started. Press ⌥⇧C to stop, ⌥⇧V for a voice note.", under: statusItem.button, seconds: 4)
            } else {
                let n = recorder.eventCount
                toast.show("■ Recording stopped: \(n) event\(n == 1 ? "" : "s") saved.", under: statusItem.button, seconds: 4)
            }
        }
        if listening != wasListening {
            wasListening = listening
            if listening {
                listenToast.show("🎙 Listening… press ⌥⇧V to finish", under: statusItem.button, seconds: Voice.maxSeconds + 5)
            } else {
                listenToast.hideNow()
            }
        }
    }

    @objc private func toggle() { recorder.toggle() }

    /// ⌥⇧V: start talking, press again to stop. Only while recording.
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
/// "Start recording first (⌥⇧C)"). Needs no notification permission.
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
