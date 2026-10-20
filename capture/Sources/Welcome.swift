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

/// One setup step: a ✓ or an empty circle, a short label, and one button. More detail on hover.
private struct StepRow: View {
    let done: Bool
    let title: String
    let help: String
    let button: String
    let action: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: done ? "checkmark.circle.fill" : "circle")
                .font(.system(size: 20))
                .foregroundStyle(done ? Color.green : Color.secondary)
            Text(title).font(.title3)
            Spacer()
            if done {
                Text("Done").foregroundStyle(.secondary)
            } else {
                Button(button, action: action).controlSize(.large)
            }
        }
        .padding(.vertical, 4)
        .help(help)
    }
}

private struct KeyRow: View {
    let keys: String
    let text: String
    var body: some View {
        HStack(spacing: 12) {
            Text(keys)
                .font(.system(.title3, design: .rounded).weight(.semibold))
                .frame(width: 56, height: 32)
                .background(RoundedRectangle(cornerRadius: 7).fill(Color.secondary.opacity(0.15)))
            Text(text).font(.title3)
        }
    }
}

struct WelcomeView: View {
    @ObservedObject var status: SetupStatus
    var onAccessibility: () -> Void
    var onDone: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Context Capsule").font(.largeTitle.bold())
                Text("Claude sums up your work. It all stays on your Mac.").foregroundStyle(.secondary)
            }

            VStack(spacing: 2) {
                StepRow(done: status.readsText, title: "Read your apps",
                        help: "Gives clean text, window names and web addresses, so projects are found by themselves. "
                            + "Switch on Context Capsule in the list that opens.",
                        button: "Turn on", action: onAccessibility)
                StepRow(done: status.microphone && status.speech, title: "Voice notes",
                        help: "Say what you're doing and why. Turned into text on this Mac; audio is never saved.",
                        button: "Allow", action: Setup.askVoice)
                StepRow(done: status.dictation, title: "Dictation",
                        help: "Apple needs it for voice notes. Scroll down to Dictation and switch it on.",
                        button: "Turn on") { Setup.openSettings(Setup.keyboardPane) }
                StepRow(done: status.claude, title: "Connect Claude",
                        help: "Lets Claude start recordings and write your capsules. Claude asks you to install it.",
                        button: "Connect") { ClaudeConnect.connect() }
                StepRow(done: status.atLogin, title: "Open at login",
                        help: "Always ready in the menu bar.",
                        button: "Turn on") { Setup.setLaunchAtLogin(true); status.refresh() }
            }

            Divider()

            VStack(alignment: .leading, spacing: 10) {
                KeyRow(keys: "⌥ C", text: "Record")
                KeyRow(keys: "⌥ V", text: "Talk")
                KeyRow(keys: "✦", text: "Ask Claude: “Make my capsules for today”")
            }

            HStack {
                Spacer()
                Button("Done", action: onDone).keyboardShortcut(.defaultAction).controlSize(.large)
            }
        }
        .padding(28)
        .frame(width: 440)
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
