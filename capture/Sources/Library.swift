// context-capsule — capture: the "My capsules" window — capsules by project (read, share, show in Finder),
// a "Make today's capsules" button that opens your AI with the request, and the projects list
// (rename, edit how they're found, delete), "Check before sharing", and opening a capsule someone sent.
// Plain native look.

import AppKit
import SwiftUI
import UniformTypeIdentifiers

enum Claude {
    /// Open the Claude app on a new chat with the request typed in (also on the clipboard, just in case).
    /// Not connected yet: connect first (`again` says what to click afterwards). No Claude app: say where to get it.
    static func open(_ prompt: String, why: String, again: String) {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(prompt, forType: .string)
        guard NSWorkspace.shared.urlForApplication(withBundleIdentifier: ClaudeConnect.desktopBundleID) != nil else {
            ClaudeConnect.connect() // explains how to get the Claude app
            return
        }
        // Without the connection, Claude can't see the capsules: connect first instead of opening an empty chat.
        guard ClaudeConnect.isConnected else {
            AppLog.shared.write("open Claude: not connected yet, connecting first")
            let alert = NSAlert()
            alert.messageText = "Connect Claude first"
            alert.informativeText = "Claude needs Context Capsule installed \(why). Click Connect, then Install in Claude. "
                + "After that, \(again)."
            alert.addButton(withTitle: "Connect")
            alert.addButton(withTitle: "Cancel")
            NSApp.activate(ignoringOtherApps: true)
            if alert.runModal() == .alertFirstButtonReturn { ClaudeConnect.connect() }
            return
        }
        AppLog.shared.write("open Claude with a request")
        NSWorkspace.shared.open(ClaudeLink.newChat(prefill: prompt) ?? ClaudeLink.newChat)
    }

    static func makeTodaysCapsules() {
        open(ClaudeLink.dayPrompt, why: "to read your day", again: "click Make today's capsules again")
    }

    /// Ask your capsules (the Capsules page), in your AI.
    static func ask(_ question: String) {
        guard !question.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        AIState.shared.open(ClaudeLink.searchPrompt(question), why: "to search your capsules", again: "click Ask again")
    }

    /// A capsule someone sent: check it is one, then your AI carries on from it.
    static func continueFrom(_ url: URL) {
        guard let text = try? String(contentsOf: url, encoding: .utf8), CapsuleSummary.isCapsule(text) else {
            AppLog.shared.write("open: not a capsule")
            let alert = NSAlert()
            alert.messageText = "This isn't a capsule"
            alert.informativeText = "\(url.lastPathComponent) isn't a Context Capsule file."
            alert.addButton(withTitle: "OK")
            NSApp.activate(ignoringOtherApps: true)
            alert.runModal()
            return
        }
        AppLog.shared.write("open: a capsule")
        AIState.shared.open(ClaudeLink.continuePrompt(path: url.path), why: "to read the capsule", again: "open the capsule again")
    }

    /// "Open a capsule…": pick a .md file.
    static func pickCapsule() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [UTType(filenameExtension: "md") ?? .plainText]
        panel.allowsMultipleSelection = false
        panel.prompt = "Open"
        panel.message = "Pick a capsule someone sent you."
        panel.directoryURL = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url else { return }
        continueFrom(url)
    }
}
