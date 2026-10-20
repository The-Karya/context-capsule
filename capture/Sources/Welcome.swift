// context-capsule — capture: the welcome screen — plain-words setup with a ✓ per step (reading text, voice
// notes, connecting to Claude, start at login) and how to use it. Shown on first launch and from the menu.

import AppKit
import AVFoundation
import ServiceManagement
import Speech
import SwiftUI

/// What the welcome screen checks, refreshed every second while it is open.
final class SetupStatus: ObservableObject {
    @Published var readsText = false
    @Published var microphone = false
    @Published var speech = false
    @Published var dictation = false
    @Published var claude = false
    @Published var atLogin = false

    private var timer: Timer?

    func start() {
        refresh()
        timer?.invalidate()
        timer = repeating(1) { [weak self] in self?.refresh() }
    }

    func stop() { timer?.invalidate(); timer = nil }

    func refresh() {
        readsText = AXReader.trusted
        microphone = AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
        speech = SFSpeechRecognizer.authorizationStatus() == .authorized
        dictation = Setup.dictationOn
        claude = ClaudeConnect.isConnected
        atLogin = SMAppService.mainApp.status == .enabled
    }

    var voiceReady: Bool { microphone && speech && dictation }
}

enum Setup {
    /// Apple allows speech recognition only when Dictation is on (System Settings → Keyboard → Dictation).
    static var dictationOn: Bool {
        (CFPreferencesCopyAppValue("Dictation Enabled" as CFString, "com.apple.assistant.support" as CFString)
            as? NSNumber)?.boolValue ?? false
    }

    static func openSettings(_ pane: String) {
        if let url = URL(string: "x-apple.systempreferences:\(pane)") { NSWorkspace.shared.open(url) }
    }

    static let accessibilityPane = "com.apple.preference.security?Privacy_Accessibility"
    static let keyboardPane = "com.apple.Keyboard-Settings.extension"

    /// Ask for the microphone and speech recognition (the macOS prompts); open Settings if already refused.
    static func askVoice() {
        let openIfDenied = {
            if AVCaptureDevice.authorizationStatus(for: .audio) == .denied {
                openSettings("com.apple.preference.security?Privacy_Microphone")
            } else if SFSpeechRecognizer.authorizationStatus() == .denied {
                openSettings("com.apple.preference.security?Privacy_SpeechRecognition")
            }
        }
        AVCaptureDevice.requestAccess(for: .audio) { _ in
            SFSpeechRecognizer.requestAuthorization { _ in DispatchQueue.main.async(execute: openIfDenied) }
        }
    }

    static func setLaunchAtLogin(_ on: Bool) {
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
            AppLog.shared.write("start at login: \(on ? "on" : "off")")
        } catch {
            AppLog.shared.write("start at login: cannot change (\(error.localizedDescription))")
        }
    }
}

/// One setup step: a ✓ (or an empty circle), what it is in plain words, and one button.
private struct StepRow: View {
    let done: Bool
    let title: String
    let detail: String
    let button: String
    var optional = false
    let action: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: done ? "checkmark.circle.fill" : "circle")
                .font(.system(size: 22))
                .foregroundStyle(done ? Color.green : Color.secondary)
            VStack(alignment: .leading, spacing: 3) {
                Text(title + (optional ? "  (optional)" : "")).font(.headline)
                Text(detail).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 8)
            if !done { Button(button, action: action).controlSize(.large) }
        }
        .padding(.vertical, 6)
    }
}

private struct KeyRow: View {
    let keys: String
    let text: String
    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text(keys)
                .font(.system(.body, design: .rounded).weight(.semibold))
                .padding(.horizontal, 8).padding(.vertical, 3)
                .background(RoundedRectangle(cornerRadius: 6).fill(Color.secondary.opacity(0.15)))
                .frame(minWidth: 70, alignment: .leading)
            Text(text).font(.callout).fixedSize(horizontal: false, vertical: true)
        }
    }
}

struct WelcomeView: View {
    @ObservedObject var status: SetupStatus
    var onAccessibility: () -> Void
    var onDone: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Welcome to Context Capsule").font(.largeTitle.bold())
            Text("While you work, it notes what's on your screen and what you say about it. Then Claude turns your day "
                 + "into a short summary per project — a capsule — that you or anyone you share it with can use.")
                .font(.body).fixedSize(horizontal: false, vertical: true)
            Text("Everything stays on your Mac. Private apps like WhatsApp and password managers are never recorded.")
                .font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)

            Divider()
            Text("Set up").font(.title3.bold())
            StepRow(done: status.readsText, title: "Read text from your apps",
                    detail: "Gives cleaner text, window names and web addresses — that's how it knows your projects. "
                        + "In the list that opens, switch on Context Capsule.",
                    button: "Turn on", action: onAccessibility)
            StepRow(done: status.microphone && status.speech, title: "Voice notes",
                    detail: "So you can say what you're doing and why. Your voice is turned into text on this Mac and never saved.",
                    button: "Allow", optional: true, action: Setup.askVoice)
            StepRow(done: status.dictation, title: "Dictation",
                    detail: "Apple needs Dictation on for voice notes. In the page that opens, scroll down to Dictation and switch it on.",
                    button: "Open", optional: true) { Setup.openSettings(Setup.keyboardPane) }
            StepRow(done: status.claude, title: "Connect to Claude",
                    detail: "So Claude can start recordings and write your capsules. Claude asks you to confirm.",
                    button: "Connect") { ClaudeConnect.connect() }
            StepRow(done: status.atLogin, title: "Start when you log in",
                    detail: "So it's always ready in the menu bar.",
                    button: "Turn on", optional: true) { Setup.setLaunchAtLogin(true); status.refresh() }

            Divider()
            Text("How to use it").font(.title3.bold())
            VStack(alignment: .leading, spacing: 8) {
                KeyRow(keys: "⌥ C", text: "Start recording, choose your screen. Press again to stop. (⌥ is the Option key.)")
                KeyRow(keys: "⌥ V", text: "Start talking: say what you're doing and why. Press again when you're done.")
                KeyRow(keys: "Claude", text: "At the end of the day, ask Claude: “Make my capsules for today.”")
            }
            Text("The circle in the menu bar turns red while recording. Click it for everything else.")
                .font(.callout).foregroundStyle(.secondary)

            HStack {
                Spacer()
                Button("Done", action: onDone).keyboardShortcut(.defaultAction).controlSize(.large)
            }
        }
        .padding(28)
        .frame(width: 600)
    }
}

/// The welcome window. One at a time; refreshes its ✓ marks while open.
final class WelcomeWindow: NSObject, NSWindowDelegate {
    static let seenKey = "welcomeSeen"
    private var window: NSWindow?
    private let status = SetupStatus()
    var onAccessibility: () -> Void = {}

    func show() {
        if let window {
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            return
        }
        let view = WelcomeView(status: status, onAccessibility: { [weak self] in self?.onAccessibility() },
                               onDone: { [weak self] in self?.window?.close() })
        let window = NSWindow(contentViewController: NSHostingController(rootView: view))
        window.title = "Context Capsule"
        window.styleMask = [.titled, .closable]
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.center()
        self.window = window
        status.start()
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        UserDefaults.standard.set(true, forKey: Self.seenKey)
    }

    func windowWillClose(_ notification: Notification) {
        status.stop()
        window = nil
    }

    static var seen: Bool { UserDefaults.standard.bool(forKey: seenKey) }
}
