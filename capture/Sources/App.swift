// context-capsule — capture: the menu-bar app — status icon, menu, ⌥⇧C hotkey, optional Accessibility.

import AppKit
import Carbon.HIToolbox

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private let paths = Paths.standard
    private lazy var recorder = Recorder(paths: paths)
    private var statusItem: NSStatusItem!
    private let infoItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
    private let skipItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
    private let toggleItem = NSMenuItem(title: "", action: #selector(toggle), keyEquivalent: "c")
    private let axItem = NSMenuItem(title: "Read text directly (Accessibility)…", action: #selector(askAccessibility),
                                    keyEquivalent: "")
    private var hotKey: HotKey?
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
        menu.addItem(infoItem)
        menu.addItem(skipItem)
        menu.addItem(.separator())
        menu.addItem(toggleItem)
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

        hotKey = HotKey(keyCode: kVK_ANSI_C, modifiers: optionKey | shiftKey) { [weak self] in self?.toggle() }
        if hotKey == nil { NSLog("Context Capsule: ⌥⇧C is taken by another app; use the menu instead") }

        control = ControlWatcher(url: paths.control) { [weak self] cmd in
            switch cmd.kind {
            case .start: self?.recorder.start()
            case .stop: self?.recorder.stop()
            }
        }

        recorder.writeState()
        refresh()
    }

    func menuWillOpen(_ menu: NSMenu) { refresh() } // the Accessibility check mark may have changed

    func applicationWillTerminate(_ notification: Notification) {
        recorder.stop(wait: true)
        recorder.writeState()
    }

    private func refresh() {
        let recording = recorder.isRecording
        let symbol = recording ? "circle.fill" : "circle"
        let image = NSImage(systemSymbolName: symbol, accessibilityDescription: recording ? "Context Capsule: recording" : "Context Capsule")
        image?.isTemplate = true
        statusItem.button?.image = image
        statusItem.button?.toolTip = recording ? "Context Capsule is recording (⌥⇧C to stop)" : "Context Capsule (⌥⇧C to record)"

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

    @objc private func toggle() { recorder.toggle() }

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

    init?(keyCode: Int, modifiers: Int, action: @escaping () -> Void) {
        self.action = action
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let me = Unmanaged.passUnretained(self).toOpaque()
        let installed = InstallEventHandler(GetApplicationEventTarget(), { _, _, userData in
            guard let userData else { return OSStatus(eventNotHandledErr) }
            let hotKey = Unmanaged<HotKey>.fromOpaque(userData).takeUnretainedValue()
            DispatchQueue.main.async { hotKey.action() }
            return noErr
        }, 1, &spec, me, &handlerRef)
        guard installed == noErr else { return nil }

        let id = EventHotKeyID(signature: OSType(0x4B43_4150), id: 1) // 'KCAP'
        guard RegisterEventHotKey(UInt32(keyCode), UInt32(modifiers), id, GetApplicationEventTarget(), 0, &hotKeyRef) == noErr
        else { return nil }
    }

    deinit {
        if let hotKeyRef { UnregisterEventHotKey(hotKeyRef) }
        if let handlerRef { RemoveEventHandler(handlerRef) }
    }
}
