// context-capsule — capture: setup checks and actions (reading text, voice notes, Dictation, your AI, open at login)
// and the shared pieces the Context Capsule window uses: a setup row, glass buttons, the blurred window background.

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
    /// At least one AI app is connected (Settings → Connect your AI).
    @Published var ai = false
    @Published var atLogin = false

    private var timer: Timer?
    private var ticks = 0

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
        // The AI apps are asked about every 10 s (it runs a small program); Claude's extension is seen every second.
        if ticks % 10 == 0 { AIState.shared.refresh() }
        ticks += 1
        ai = AIState.shared.anyConnected
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
    static let screenRecordingPane = "com.apple.preference.security?Privacy_ScreenCapture"

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
struct StepRow: View {
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
                Button(button, action: action).controlSize(.regular).modifier(GlassButton())
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 44)
        .help(help)
    }
}

/// Settings → Set up → Connect your AI: a ✓ when at least one AI is connected, then the AI apps on this Mac
/// (Connect, or a ✓ with Disconnect in a small menu); apps that aren't installed are folded under More.
struct ConnectAIRows: View {
    @ObservedObject var ai: AIState
    let done: Bool
    @State private var more = false

    private var installed: [AIClient] { ai.clients.filter(\.installed) }
    private var missing: [AIClient] { ai.clients.filter { !$0.installed } }

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "link").font(.system(size: 15)).foregroundStyle(.secondary).frame(width: 22)
            Text("Connect your AI").font(.system(size: 14))
            Spacer()
            if done {
                Image(systemName: "checkmark").font(.system(size: 13, weight: .semibold)).foregroundStyle(.secondary)
            } else if !ai.loaded {
                ProgressView().controlSize(.small)
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 44)
        .help("Lets your AI start contextualizing and write your capsules.")

        ForEach(installed, id: \.id) { c in
            Divider().padding(.leading, 48)
            row(c)
        }
        if !missing.isEmpty {
            Divider().padding(.leading, 48)
            Button { more.toggle() } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold))
                        .rotationEffect(.degrees(more ? 90 : 0))
                    Text("More").font(.system(size: 13))
                    Spacer()
                }
                .foregroundStyle(.secondary)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.leading, 48).padding(.trailing, 14)
            .frame(height: 32)
            if more {
                ForEach(missing, id: \.id) { c in
                    HStack {
                        Text(c.name).font(.system(size: 13))
                        Spacer()
                        Text("Not installed").font(.system(size: 12))
                    }
                    .foregroundStyle(.tertiary)
                    .padding(.leading, 48).padding(.trailing, 14)
                    .frame(height: 28)
                }
            }
        }
        if let note = ai.note {
            Text(note).font(.system(size: 12)).foregroundStyle(.secondary)
                .padding(.leading, 48).padding(.trailing, 14).padding(.vertical, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        Text("Any other AI (ChatGPT website, Gemini, Copilot…): use Copy my day.")
            .font(.system(size: 12)).foregroundStyle(.secondary)
            .padding(.leading, 48).padding(.trailing, 14).padding(.top, 2).padding(.bottom, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func row(_ c: AIClient) -> some View {
        HStack(spacing: 10) {
            AppIcon(client: c).frame(width: 18, height: 18)
            Text(c.name).font(.system(size: 13))
            Spacer()
            if ai.busy == c.id {
                ProgressView().controlSize(.small)
            } else if c.connected {
                Image(systemName: "checkmark").font(.system(size: 12, weight: .semibold)).foregroundStyle(.secondary)
                Menu {
                    Button("Disconnect") { ai.disconnect(c) }
                } label: {
                    Image(systemName: "ellipsis")
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .help("More")
            } else {
                Button("Connect") { ai.connect(c) }.controlSize(.small).disabled(ai.busy != nil)
            }
        }
        .padding(.leading, 48).padding(.trailing, 14)
        .frame(height: 36)
    }
}

/// The app's own icon when it can be found, else a plain symbol.
private struct AppIcon: View {
    let client: AIClient
    var body: some View {
        if let url = AIState.appURL(client) {
            Image(nsImage: NSWorkspace.shared.icon(forFile: url.path)).resizable().interpolation(.high)
        } else {
            Image(systemName: "terminal").foregroundStyle(.secondary)
        }
    }
}

struct GlassButton: ViewModifier {
    var prominent = false
    func body(content: Content) -> some View {
        if #available(macOS 26, *) {
            if prominent { content.buttonStyle(.glassProminent) } else { content.buttonStyle(.glass) }
        } else {
            if prominent { content.buttonStyle(.borderedProminent) } else { content.buttonStyle(.bordered) }
        }
    }
}

/// The desktop, blurred, behind the whole window.
struct WindowGlass: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .underWindowBackground
        view.blendingMode = .behindWindow
        view.state = .active
        return view
    }
    func updateNSView(_ view: NSVisualEffectView, context: Context) {}
}
