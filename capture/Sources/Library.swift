// context-capsule — capture: the "My capsules" window — capsules by project (read, share, show in Finder),
// a "Make today's capsules" button that opens your AI with the request, and the projects list
// (rename, edit how they're found, delete), "Check before sharing", and opening a capsule someone sent.
// Plain native look.

import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// What the window shows; reloaded when it opens and after every change.
final class LibraryModel: ObservableObject {
    @Published var capsules: [CapsuleSummary] = []
    @Published var projects: [Project] = []
    @Published var tab = 0
    /// The Capsules page's search field: filters by title and "In short" as you type.
    @Published var query = ""
    let paths: Paths
    var onProjectsChanged: () -> Void = {}

    init(paths: Paths) { self.paths = paths }

    func reload() {
        capsules = CapsuleSummary.all(in: paths.capsules)
        projects = Projects.load(from: paths.projects)
    }


    func name(of project: String) -> String {
        projects.first { $0.id == project }?.name ?? project
    }

    /// Projects with capsules (the growing capsule first, then days, newest first), then capsules with no project.
    var groups: [(title: String, items: [CapsuleSummary])] {
        let shown = capsules.filter { $0.matches(query) }
        var order: [String] = []
        for c in shown where !order.contains(c.project) { order.append(c.project) }
        order.sort { a, b in a.isEmpty ? false : b.isEmpty ? true : name(of: a).localizedCaseInsensitiveCompare(name(of: b)) == .orderedAscending }
        return order.map { p in
            let items = shown.filter { $0.project == p }.sorted { a, b in
                if (a.kind == "project") != (b.kind == "project") { return a.kind == "project" }
                return (a.date, a.created) > (b.date, b.created)
            }
            return (p.isEmpty ? "Other" : name(of: p), items)
        }
    }

    func save(_ list: [Project]) {
        do {
            try Projects.save(list, to: paths.projects)
            AppLog.shared.write("projects edited: \(list.count) project(s)")
        } catch {
            AppLog.shared.write("cannot save projects.json: \(error.localizedDescription)")
        }
        reload()
        onProjectsChanged()
    }

    func rename(_ id: String, to name: String) {
        let clean = name.trimmingCharacters(in: .whitespaces)
        guard !clean.isEmpty else { return }
        save(projects.map { $0.id == id ? Project(id: $0.id, name: clean, match: $0.match) : $0 })
    }

    func setClues(_ id: String, _ text: String) {
        let clues = text.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { $0.count >= 2 }
        save(projects.map { $0.id == id ? Project(id: $0.id, name: $0.name, match: clues) : $0 })
    }

    func delete(_ id: String) { save(projects.filter { $0.id != id }) }
}

/// Sharing a capsule anywhere: copy the file (paste into WhatsApp, Slack, Gmail…), copy its text, save a copy, or
/// AirDrop / Mail / Messages. Every one first shows "Check before sharing" (when something private-looking is found),
/// and shares a copy with the checked items replaced by [removed]. The saved capsule is never changed.
enum Share {
    static var toast: ((String) -> Void)?

    static func copyFile(_ capsule: URL) {
        guard let url = checked(capsule) else { return }
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.writeObjects([url as NSURL])
        toast?("Copied. Paste it in WhatsApp, Slack, email or any chat.")
    }

    static func copyText(_ capsule: URL) {
        guard let url = checked(capsule), let text = try? String(contentsOf: url, encoding: .utf8) else { return }
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(text, forType: .string)
        toast?("Text copied. Paste it into a chat, or into your AI.")
    }

    /// AirDrop, Mail, Messages…: the system share menu, under the mouse in the front window.
    static func more(_ capsule: URL) {
        guard let url = checked(capsule) else { return }
        guard let window = NSApp.keyWindow ?? NSApp.mainWindow, let view = window.contentView else { return }
        let point = view.convert(window.mouseLocationOutsideOfEventStream, from: nil)
        NSSharingServicePicker(items: [url]).show(relativeTo: NSRect(x: point.x, y: point.y, width: 1, height: 1),
                                                  of: view, preferredEdge: .minY)
    }

    /// The file to share: the capsule itself when nothing is found or nothing is checked, a cleaned copy (same name,
    /// in a temporary folder) otherwise, or nil when cancelled.
    static func checked(_ capsule: URL) -> URL? {
        guard let text = try? String(contentsOf: capsule, encoding: .utf8) else { return capsule }
        let groups = ShareCheck.find(text).filter { !$0.items.isEmpty }
        if groups.isEmpty { return capsule }
        guard let remove = ShareCheckPanel.ask(groups) else { return nil }
        if remove.isEmpty { return capsule }
        let fm = FileManager.default
        let base = fm.temporaryDirectory.appendingPathComponent("context-capsule-share", isDirectory: true)
        // Copies from earlier shares, after a day (they may still be waiting on the clipboard before that).
        for old in (try? fm.contentsOfDirectory(at: base, includingPropertiesForKeys: [.creationDateKey])) ?? [] {
            if let made = try? old.resourceValues(forKeys: [.creationDateKey]).creationDate, made < Date().addingTimeInterval(-86_400) {
                try? fm.removeItem(at: old)
            }
        }
        let folder = base.appendingPathComponent(UUID().uuidString, isDirectory: true)
        let copy = folder.appendingPathComponent(capsule.lastPathComponent)
        do {
            try fm.createDirectory(at: folder, withIntermediateDirectories: true)
            try ShareCheck.remove(text, remove).write(to: copy, atomically: true, encoding: .utf8)
        } catch {
            AppLog.shared.write("share: cannot make the cleaned copy: \(error.localizedDescription)")
            toast?("Couldn't make the copy to share: \(error.localizedDescription)")
            return nil
        }
        AppLog.shared.write("share: \(remove.count) item(s) removed from the copy")
        return copy
    }

    static func saveCopy(_ capsule: URL) {
        guard let url = checked(capsule) else { return }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = capsule.lastPathComponent
        panel.directoryURL = FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask).first
        panel.canCreateDirectories = true
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let dest = panel.url else { return }
        try? FileManager.default.removeItem(at: dest)
        do {
            try FileManager.default.copyItem(at: url, to: dest)
            toast?("Saved to \(dest.deletingLastPathComponent().lastPathComponent).")
            NSWorkspace.shared.activateFileViewerSelecting([dest])
        } catch {
            toast?("Couldn't save the copy: \(error.localizedDescription)")
        }
    }
}

/// "Check before sharing": what might be private, a check box for each (all checked), Share or Cancel.
enum ShareCheckPanel {
    /// The checked items, or nil when cancelled.
    static func ask(_ groups: [ShareCheck.Group]) -> [String]? {
        let alert = NSAlert()
        alert.messageText = "Check before sharing"
        alert.informativeText = "Checked items become [removed] in the copy you share. Your saved capsule stays the same."
        alert.addButton(withTitle: "Share")
        alert.addButton(withTitle: "Cancel")

        let width: CGFloat = 320
        let stack = NSStackView()
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 4
        stack.edgeInsets = NSEdgeInsets(top: 2, left: 0, bottom: 2, right: 0)
        var boxes: [(NSButton, String)] = []
        for (i, group) in groups.enumerated() {
            let title = NSTextField(labelWithString: group.title)
            title.font = .systemFont(ofSize: NSFont.smallSystemFontSize, weight: .semibold)
            title.textColor = .secondaryLabelColor
            stack.addArrangedSubview(title)
            if i > 0 { stack.setCustomSpacing(12, after: stack.arrangedSubviews[stack.arrangedSubviews.count - 2]) }
            for item in group.items {
                let box = NSButton(checkboxWithTitle: item, target: nil, action: nil)
                box.state = .on
                box.lineBreakMode = .byTruncatingMiddle
                box.toolTip = item
                box.widthAnchor.constraint(lessThanOrEqualToConstant: width - 16).isActive = true
                box.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
                stack.addArrangedSubview(box)
                boxes.append((box, item))
            }
        }
        stack.translatesAutoresizingMaskIntoConstraints = false
        let fitting = stack.fittingSize.height
        let height = min(fitting, 300)

        let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: width, height: height))
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = fitting > height
        scroll.borderType = .noBorder
        let flipped = FlippedView()
        flipped.translatesAutoresizingMaskIntoConstraints = false
        flipped.addSubview(stack)
        scroll.documentView = flipped
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: flipped.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: flipped.trailingAnchor),
            stack.topAnchor.constraint(equalTo: flipped.topAnchor),
            stack.bottomAnchor.constraint(equalTo: flipped.bottomAnchor),
            flipped.widthAnchor.constraint(equalToConstant: width - (fitting > height ? 16 : 0)),
        ])
        alert.accessoryView = scroll

        NSApp.activate(ignoringOtherApps: true)
        guard alert.runModal() == .alertFirstButtonReturn else { return nil }
        return boxes.filter { $0.0.state == .on }.map(\.1)
    }

    private final class FlippedView: NSView { override var isFlipped: Bool { true } }
}

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

struct CapsuleRow: View {
    let capsule: CapsuleSummary
    @State private var hover = false

    /// Daily capsules are named by their day, the growing one "Whole project", others by their title.
    private var label: String {
        if capsule.kind == "project" { return "Whole project" }
        if capsule.kind == "daily", !capsule.date.isEmpty { return Self.day(capsule.date) }
        return capsule.title
    }

    /// The day shown on the right, for capsules not already named by their day.
    private var when: String {
        guard capsule.kind != "daily", capsule.created.count >= 10 else { return "" }
        return Self.day(String(capsule.created.prefix(10)))
    }

    static func day(_ ymd: String) -> String {
        guard let d = DayWords.parseKey(ymd) else { return ymd }
        if Calendar.current.isDateInToday(d) { return "Today" }
        if Calendar.current.isDateInYesterday(d) { return "Yesterday" }
        return d.formatted(.dateTime.day().month(.abbreviated))
    }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: capsule.kind == "project" ? "folder" : "doc.text")
                .font(.system(size: 15)).foregroundStyle(.secondary).frame(width: 20).padding(.top, 2)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(label).font(.system(size: 14, weight: .medium)).lineLimit(1)
                    if capsule.draft { Text("Draft").font(.system(size: 11)).foregroundStyle(.secondary) }
                }
                if !capsule.inShort.isEmpty {
                    Text(capsule.inShort).font(.system(size: 12)).foregroundStyle(.secondary).lineLimit(2)
                }
            }
            Spacer(minLength: 8)
            if !when.isEmpty { Text(when).font(.system(size: 12)).foregroundStyle(.secondary).padding(.top, 2) }
            HStack(spacing: 4) {
                Menu {
                    Button("Copy file") { Share.copyFile(capsule.url) }
                    Button("Copy text") { Share.copyText(capsule.url) }
                    Button("Save a copy…") { Share.saveCopy(capsule.url) }
                    Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([capsule.url]) }
                    Divider()
                    Button("More…") { Share.more(capsule.url) }
                } label: {
                    Image(systemName: "square.and.arrow.up")
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .help("Share")
                Button { NSWorkspace.shared.open(capsule.url) } label: { Image(systemName: "eye") }
                    .help("Open")
            }
            .buttonStyle(.borderless)
            .opacity(hover ? 1 : 0.35)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .contentShape(Rectangle())
        .onHover { hover = $0 }
        .onTapGesture(count: 2) { NSWorkspace.shared.open(capsule.url) }
        .contextMenu {
            Button("Open") { NSWorkspace.shared.open(capsule.url) }
            Button("Copy file") { Share.copyFile(capsule.url) }
            Button("Copy text") { Share.copyText(capsule.url) }
            Button("Save a copy…") { Share.saveCopy(capsule.url) }
            Button("More…") { Share.more(capsule.url) }
            Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([capsule.url]) }
        }
    }
}

struct ProjectRow: View {
    let project: Project
    @ObservedObject var model: LibraryModel
    @State private var editing = false
    @State private var name = ""
    @State private var clues = ""
    @State private var confirmDelete = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if editing {
                TextField("Name", text: $name).textFieldStyle(.roundedBorder)
                TextField("Found by (comma-separated words in window titles or web addresses)", text: $clues)
                    .textFieldStyle(.roundedBorder)
                HStack {
                    Spacer()
                    Button("Cancel") { editing = false }
                    Button("Save") {
                        model.rename(project.id, to: name)
                        model.setClues(project.id, clues)
                        editing = false
                    }
                    .keyboardShortcut(.defaultAction)
                }
            } else {
                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(project.name).font(.system(size: 14, weight: .medium))
                        Text(project.match.isEmpty ? "Not found automatically yet" : "Found by: " + project.match.joined(separator: ", "))
                            .font(.system(size: 12)).foregroundStyle(.secondary).lineLimit(1)
                    }
                    Spacer()
                    Button("Edit") {
                        name = project.name
                        clues = project.match.joined(separator: ", ")
                        editing = true
                    }
                    Button("Delete") { confirmDelete = true }
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .confirmationDialog("Delete “\(project.name)”?", isPresented: $confirmDelete) {
            Button("Delete", role: .destructive) { model.delete(project.id) }
        } message: {
            Text("Its capsules stay. New work won't be sorted into it anymore.")
        }
    }
}

struct Panel<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 14, style: .continuous)
        if #available(macOS 26, *) {
            VStack(spacing: 0) { content }
                .background(shape.fill(Color.primary.opacity(0.035))) // always visible, even where the glass has nothing behind it
                .glassEffect(.regular, in: shape)
        } else {
            VStack(spacing: 0) { content }.background(shape.fill(.regularMaterial))
        }
    }
}
