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

/// One setup step: a quiet icon, a short label, then a plain button or a check.
private struct StepRow: View {
    let icon: String
    let done: Bool
    let title: String
    let help: String
    let button: String
    let action: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: icon)
                .font(.system(size: 15))
                .foregroundStyle(.secondary)
                .frame(width: 22)
            Text(title).font(.system(size: 14))
            Spacer()
            if done {
                Image(systemName: "checkmark").font(.system(size: 13, weight: .semibold)).foregroundStyle(.secondary)
            } else {
                Button(button, action: action).controlSize(.regular)
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 44)
        .help(help)
    }
}

private struct Shortcut: View {
    let keys: String
    let label: String
    var body: some View {
        HStack {
            Text(label).font(.system(size: 14))
            Spacer()
            Text(keys).font(.system(size: 14)).foregroundStyle(.secondary)
        }
        .padding(.horizontal, 14)
        .frame(height: 40)
    }
}

private struct Group<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        VStack(spacing: 0) { content }
            .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(Color(nsColor: .controlBackgroundColor)))
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Color.primary.opacity(0.08)))
    }
}

struct WelcomeView: View {
    @ObservedObject var status: SetupStatus
    var onAccessibility: () -> Void
    var onDone: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Context Capsule").font(.system(size: 22, weight: .semibold))

            VStack(alignment: .leading, spacing: 6) {
                Text("Set up").font(.system(size: 12, weight: .medium)).foregroundStyle(.secondary).padding(.leading, 4)
                Group {
                    StepRow(icon: "text.viewfinder", done: status.readsText, title: "Read your apps",
                            help: "Clean text, window names and web addresses, so projects are found by themselves. "
                                + "Switch on Context Capsule in the list that opens.",
                            button: "Turn On", action: onAccessibility)
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
                            help: "Lets Claude start recordings and write your capsules. Claude asks you to install it.",
                            button: "Connect") { ClaudeConnect.connect() }
                    Divider().padding(.leading, 48)
                    StepRow(icon: "power", done: status.atLogin, title: "Open at login",
                            help: "Always ready in the menu bar.",
                            button: "Turn On") { Setup.setLaunchAtLogin(true); status.refresh() }
                }
            }

            VStack(alignment: .leading, spacing: 6) {
                Text("Use").font(.system(size: 12, weight: .medium)).foregroundStyle(.secondary).padding(.leading, 4)
                Group {
                    Shortcut(keys: "⌥ C", label: "Record")
                    Divider().padding(.leading, 14)
                    Shortcut(keys: "⌥ V", label: "Talk")
                    Divider().padding(.leading, 14)
                    Shortcut(keys: "in Claude", label: "Make my capsules")
                }
            }

            HStack {
                Spacer()
                Button("Done", action: onDone).keyboardShortcut(.defaultAction).controlSize(.large)
            }
        }
        .padding(24)
        .frame(width: 380)
    }
}

/// `--render-welcome <png>`: draw the welcome screen to an image (for checking the design without the app).
enum WelcomeRender {
    @MainActor
    static func run(to path: String, dark: Bool) -> Bool {
        let status = SetupStatus()
        status.readsText = true
        status.claude = false
        let view = WelcomeView(status: status, onAccessibility: {}, onDone: {})
            .background(Color(nsColor: .windowBackgroundColor))
            .environment(\.colorScheme, dark ? .dark : .light)
        let renderer = ImageRenderer(content: view)
        renderer.scale = 2
        guard let image = renderer.nsImage, let tiff = image.tiffRepresentation,
              let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) else { return false }
        return FileManager.default.createFile(atPath: path, contents: png)
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
        window.title = ""
        window.styleMask = [.titled, .closable, .fullSizeContentView]
        window.titlebarAppearsTransparent = true
        window.isMovableByWindowBackground = true
        window.backgroundColor = .windowBackgroundColor
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
