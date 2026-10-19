// karya-capsule — capture: the menu-bar app — status icon, menu, ⌥⇧C hotkey, permission alert.

import AppKit
import Carbon.HIToolbox

final class AppDelegate: NSObject, NSApplicationDelegate {
    private let paths = Paths.standard
    private lazy var recorder = Recorder(paths: paths)
    private var statusItem: NSStatusItem!
    private let infoItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
    private let toggleItem = NSMenuItem(title: "", action: #selector(toggle), keyEquivalent: "c")
    private let permissionItem = NSMenuItem(title: "Grant Screen Recording…", action: #selector(openScreenSettings),
                                            keyEquivalent: "")
    private var hotKey: HotKey?
    private var control: ControlWatcher?
    private var alertShown = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        try? FileManager.default.createDirectory(at: paths.sessions, withIntermediateDirectories: true)

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        let menu = NSMenu()
        infoItem.isEnabled = false
        toggleItem.keyEquivalentModifierMask = [.option, .shift]
        menu.addItem(infoItem)
        menu.addItem(.separator())
        menu.addItem(toggleItem)
        menu.addItem(permissionItem)
        menu.addItem(NSMenuItem(title: "Open sessions folder", action: #selector(openSessions), keyEquivalent: ""))
        menu.addItem(.separator())
        menu.addItem(NSMenuItem(title: "Quit Capsule", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        for item in menu.items where item.action != nil && item.action != #selector(NSApplication.terminate(_:)) {
            item.target = self
        }
        statusItem.menu = menu

        recorder.onChange = { [weak self] in self?.refresh() }
        recorder.onPermissionNeeded = { [weak self] in self?.askForScreenRecording() }

        hotKey = HotKey(keyCode: kVK_ANSI_C, modifiers: optionKey | shiftKey) { [weak self] in self?.toggle() }
        if hotKey == nil { NSLog("Capsule: ⌥⇧C is taken by another app; use the menu instead") }

        control = ControlWatcher(url: paths.control) { [weak self] cmd in
            switch cmd.kind {
            case .start: self?.recorder.start()
            case .stop: self?.recorder.stop()
            }
        }

        recorder.writeState()
        refresh()
        if !CGPreflightScreenCaptureAccess() { askForScreenRecording() }
    }

    func applicationWillTerminate(_ notification: Notification) {
        recorder.stop(wait: true)
        recorder.writeState()
    }

    private func refresh() {
        let recording = recorder.isRecording
        let symbol = recording ? "circle.fill" : "circle"
        let image = NSImage(systemSymbolName: symbol, accessibilityDescription: recording ? "Capsule: recording" : "Capsule")
        image?.isTemplate = true
        statusItem.button?.image = image
        statusItem.button?.toolTip = recording ? "Capsule is recording (⌥⇧C to stop)" : "Capsule (⌥⇧C to record)"

        if let id = recorder.sessionID, let started = recorder.startedAt {
            let time = DateFormatter.localizedString(from: started, dateStyle: .none, timeStyle: .short)
            infoItem.title = "Recording since \(time) — \(id)"
        } else {
            infoItem.title = "Not recording"
        }
        toggleItem.title = recording ? "Stop recording" : "Start recording"
        permissionItem.isHidden = CGPreflightScreenCaptureAccess()
    }

    @objc private func toggle() { recorder.toggle() }

    @objc private func openSessions() {
        try? FileManager.default.createDirectory(at: paths.sessions, withIntermediateDirectories: true)
        NSWorkspace.shared.open(paths.sessions)
    }

    @objc private func openScreenSettings() {
        CGRequestScreenCaptureAccess()
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture") {
            NSWorkspace.shared.open(url)
        }
    }

    /// One explanation per launch; after that the menu keeps a "Grant Screen Recording…" item.
    private func askForScreenRecording() {
        refresh()
        guard !alertShown else { return }
        alertShown = true
        CGRequestScreenCaptureAccess() // adds Capsule to the list in System Settings
        NSApp.activate(ignoringOtherApps: true)
        let alert = NSAlert()
        alert.messageText = "Capsule needs Screen Recording"
        alert.informativeText = """
            Capsule reads the text on your screen while you record, so your AI can tell what you worked on. \
            Screenshots stay in memory and are never saved; only the text is kept, on this Mac.

            Open System Settings → Privacy & Security → Screen Recording, turn on Capsule, \
            then quit and reopen Capsule.

            Optional: allow Capsule under Accessibility too, for better window titles.
            """
        alert.addButton(withTitle: "Open System Settings")
        alert.addButton(withTitle: "Later")
        if alert.runModal() == .alertFirstButtonReturn { openScreenSettings() }
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
