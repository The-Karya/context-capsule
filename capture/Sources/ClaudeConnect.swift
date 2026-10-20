// context-capsule — capture: "Connect to Claude" — installs the Context Capsule extension in Claude Desktop
// (a .mcpb file shipped inside this app; Claude shows an Install button) and tells whether it's connected.

import AppKit

enum ClaudeConnect {
    static let desktopBundleID = "com.anthropic.claudefordesktop"
    static let downloadURL = URL(string: "https://claude.ai/download")!

    /// The extension file inside this app (built by mcp/extension/build.sh, copied in by build.sh).
    static var extensionFile: URL? {
        Bundle.main.url(forResource: "context-capsule", withExtension: "mcpb")
    }

    /// Connected in Claude Desktop (extension or config entry) or in Claude Code.
    static var isConnected: Bool {
        let fm = FileManager.default
        let support = fm.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/Claude")
        let extensions = support.appendingPathComponent("Claude Extensions")
        if let names = try? fm.contentsOfDirectory(atPath: extensions.path),
           names.contains(where: { $0.lowercased().contains("context-capsule") }) { return true }
        if hasServer(in: support.appendingPathComponent("claude_desktop_config.json")) { return true }
        return hasServer(in: fm.homeDirectoryForCurrentUser.appendingPathComponent(".claude.json"))
    }

    /// Any "mcpServers" object (at any depth) with a "context-capsule" entry.
    static func hasServer(in file: URL) -> Bool {
        guard let data = try? Data(contentsOf: file), let json = try? JSONSerialization.jsonObject(with: data) else { return false }
        return containsServer(json)
    }

    static func containsServer(_ value: Any) -> Bool {
        if let dict = value as? [String: Any] {
            if let servers = dict["mcpServers"] as? [String: Any], servers["context-capsule"] != nil { return true }
            return dict.values.contains(where: containsServer)
        }
        if let list = value as? [Any] { return list.contains(where: containsServer) }
        return false
    }

    /// Open the extension in Claude Desktop (it asks the user to install). No Claude Desktop: say where to get it.
    static func connect() {
        guard let claude = NSWorkspace.shared.urlForApplication(withBundleIdentifier: desktopBundleID) else {
            let alert = NSAlert()
            alert.messageText = "Install the Claude app first"
            alert.informativeText = "Context Capsule works with the Claude desktop app. Download it, sign in, then click Connect again."
            alert.addButton(withTitle: "Download Claude")
            alert.addButton(withTitle: "Cancel")
            NSApp.activate(ignoringOtherApps: true)
            if alert.runModal() == .alertFirstButtonReturn { NSWorkspace.shared.open(downloadURL) }
            return
        }
        guard let file = extensionFile else {
            AppLog.shared.write("connect: the extension file is missing from the app")
            let alert = NSAlert()
            alert.messageText = "Something is missing"
            alert.informativeText = "This copy of Context Capsule doesn't include the Claude extension. Please download Context Capsule again."
            NSApp.activate(ignoringOtherApps: true)
            alert.runModal()
            return
        }
        AppLog.shared.write("connect: opening the extension in Claude")
        let config = NSWorkspace.OpenConfiguration()
        config.activates = true
        NSWorkspace.shared.open([file], withApplicationAt: claude, configuration: config) { _, error in
            if let error { AppLog.shared.write("connect: cannot open in Claude (\(error.localizedDescription))") }
        }
    }
}
