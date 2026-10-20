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

/// The capsule mark: a soft gradient pill with a sparkle.
struct CapsuleMark: View {
    var size: CGFloat = 56
    var body: some View {
        ZStack {
            Capsule()
                .fill(LinearGradient(colors: [Color(red: 0.45, green: 0.36, blue: 0.98), Color(red: 0.98, green: 0.47, blue: 0.60)],
                                     startPoint: .topLeading, endPoint: .bottomTrailing))
                .frame(width: size * 1.6, height: size)
                .shadow(color: Color(red: 0.6, green: 0.4, blue: 0.95).opacity(0.35), radius: 12, y: 6)
            Image(systemName: "sparkles")
                .font(.system(size: size * 0.42, weight: .semibold))
                .foregroundStyle(.white)
        }
    }
}

/// One setup step in a card: a coloured icon, a short label, then a pill button or a green check.
private struct StepRow: View {
    let icon: String
    let tint: Color
    let done: Bool
    let title: String
    let help: String
    let button: String
    let action: () -> Void

    var body: some View {
        HStack(spacing: 14) {
            ZStack {
                RoundedRectangle(cornerRadius: 9, style: .continuous).fill(tint.gradient)
                Image(systemName: icon).font(.system(size: 15, weight: .semibold)).foregroundStyle(.white)
            }
            .frame(width: 32, height: 32)
            Text(title).font(.system(size: 15, weight: .medium))
            Spacer()
            if done {
                Image(systemName: "checkmark.circle.fill")
                    .font(.system(size: 22))
                    .foregroundStyle(.white, .green)
                    .transition(.scale.combined(with: .opacity))
            } else {
                Button(action: action) {
                    Text(button).font(.system(size: 13, weight: .semibold)).padding(.horizontal, 6)
                }
                .buttonStyle(.bordered)
                .buttonBorderShape(.capsule)
                .tint(Color(red: 0.45, green: 0.36, blue: 0.98))
                .controlSize(.regular)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .contentShape(Rectangle())
        .help(help)
        .animation(.spring(duration: 0.35), value: done)
    }
}

/// A keyboard key.
private struct Keycap: View {
    let label: String
    var body: some View {
        Text(label)
            .font(.system(size: 15, weight: .semibold, design: .rounded))
            .frame(minWidth: 30, minHeight: 30)
            .padding(.horizontal, 4)
            .background(
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .fill(Color(nsColor: .controlBackgroundColor))
                    .shadow(color: .black.opacity(0.18), radius: 0, y: 1.5)
            )
            .overlay(RoundedRectangle(cornerRadius: 7, style: .continuous).strokeBorder(Color.primary.opacity(0.12)))
    }
}

private struct Shortcut: View {
    let keys: [String]
    let label: String
    var body: some View {
        VStack(spacing: 8) {
            HStack(spacing: 4) { ForEach(keys, id: \.self) { Keycap(label: $0) } }
            Text(label).font(.system(size: 13, weight: .medium)).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
    }
}

private struct Card<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        VStack(spacing: 0) { content }
            .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.primary.opacity(0.045)))
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Color.primary.opacity(0.07)))
    }
}

struct WelcomeView: View {
    @ObservedObject var status: SetupStatus
    var onAccessibility: () -> Void
    var onDone: () -> Void

    private var doneCount: Int {
        [status.readsText, status.microphone && status.speech, status.dictation, status.claude, status.atLogin]
            .filter { $0 }.count
    }

    var body: some View {
        VStack(spacing: 22) {
            VStack(spacing: 14) {
                CapsuleMark()
                Text("Context Capsule").font(.system(size: 26, weight: .bold, design: .rounded))
            }
            .padding(.top, 8)

            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text("Set up").font(.system(size: 13, weight: .semibold)).foregroundStyle(.secondary)
                    Spacer()
                    Text("\(doneCount) of 5").font(.system(size: 12, weight: .medium)).foregroundStyle(.secondary)
                }
                .padding(.horizontal, 4)
                Card {
                    StepRow(icon: "text.viewfinder", tint: .blue, done: status.readsText, title: "Read your apps",
                            help: "Clean text, window names and web addresses, so projects are found by themselves. "
                                + "Switch on Context Capsule in the list that opens.",
                            button: "Turn on", action: onAccessibility)
                    Divider().padding(.leading, 60)
                    StepRow(icon: "mic.fill", tint: .orange, done: status.microphone && status.speech, title: "Voice notes",
                            help: "Say what you're doing and why. Turned into text on this Mac; audio is never saved.",
                            button: "Allow", action: Setup.askVoice)
                    Divider().padding(.leading, 60)
                    StepRow(icon: "waveform", tint: .pink, done: status.dictation, title: "Dictation",
                            help: "Apple needs it for voice notes. Scroll down to Dictation and switch it on.",
                            button: "Turn on") { Setup.openSettings(Setup.keyboardPane) }
                    Divider().padding(.leading, 60)
                    StepRow(icon: "sparkles", tint: Color(red: 0.85, green: 0.47, blue: 0.34), done: status.claude,
                            title: "Connect Claude",
                            help: "Lets Claude start recordings and write your capsules. Claude asks you to install it.",
                            button: "Connect") { ClaudeConnect.connect() }
                    Divider().padding(.leading, 60)
                    StepRow(icon: "power", tint: .green, done: status.atLogin, title: "Open at login",
                            help: "Always ready in the menu bar.",
                            button: "Turn on") { Setup.setLaunchAtLogin(true); status.refresh() }
                }
            }

            HStack(spacing: 0) {
                Shortcut(keys: ["⌥", "C"], label: "Record")
                Shortcut(keys: ["⌥", "V"], label: "Talk")
                Shortcut(keys: ["✦"], label: "Ask Claude")
            }
            .padding(.vertical, 4)

            Button(action: onDone) {
                Text(doneCount == 5 ? "Start" : "Done").font(.system(size: 14, weight: .semibold)).frame(maxWidth: .infinity)
                    .padding(.vertical, 4)
            }
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.capsule)
            .controlSize(.large)
            .tint(Color(red: 0.45, green: 0.36, blue: 0.98))
            .keyboardShortcut(.defaultAction)
        }
        .padding(.horizontal, 28)
        .padding(.bottom, 26)
        .padding(.top, 18)
        .frame(width: 400)
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
