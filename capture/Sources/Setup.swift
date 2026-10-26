// context-capsule — capture: setup checks and actions (reading text, voice notes, Dictation, your AI, open at login)
// for the window's Set up steps (setup.status, setup.do).

import AppKit
import AVFoundation
import ServiceManagement
import Speech

/// What the Set up steps check, refreshed each time the page asks.
final class SetupStatus {
    var readsText = false
    var microphone = false
    var speech = false
    var dictation = false
    /// At least one AI app is connected (Settings → Connect your AI).
    var ai = false
    var atLogin = false

    private var ticks = 0

    func refresh() {
        readsText = AXReader.trusted
        microphone = AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
        speech = SFSpeechRecognizer.authorizationStatus() == .authorized
        dictation = Setup.dictationOn
        // The AI apps are asked about every 10th time (it runs a small program); Claude's extension is seen every time.
        if ticks % 10 == 0 { AIState.shared.refresh() }
        ticks += 1
        ai = AIState.shared.anyConnected
        atLogin = SMAppService.mainApp.status == .enabled
    }
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
