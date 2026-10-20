// context-capsule — capture: the "My capsules" window — capsules by project (read, share, show in Finder),
// a "Make today's capsules" button that opens Claude with the request typed in, and the projects list
// (rename, edit how they're found, delete). Plain native look.

import AppKit
import SwiftUI

/// What the window shows; reloaded when it opens and after every change.
final class LibraryModel: ObservableObject {
    @Published var capsules: [CapsuleSummary] = []
    @Published var projects: [Project] = []
    @Published var tab = 0
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
        var order: [String] = []
        for c in capsules where !order.contains(c.project) { order.append(c.project) }
        order.sort { a, b in a.isEmpty ? false : b.isEmpty ? true : name(of: a).localizedCaseInsensitiveCompare(name(of: b)) == .orderedAscending }
        return order.map { p in
            let items = capsules.filter { $0.project == p }.sorted { a, b in
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

/// Sharing a capsule anywhere: copy the file (paste into WhatsApp, Slack, Gmail…), copy its text, or save a copy.
enum Share {
    static var toast: ((String) -> Void)?

    static func copyFile(_ url: URL) {
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.writeObjects([url as NSURL])
        toast?("Copied. Paste it in WhatsApp, Slack, email or any chat.")
    }

    static func copyText(_ url: URL) {
        guard let text = try? String(contentsOf: url, encoding: .utf8) else { return }
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(text, forType: .string)
        toast?("Text copied. Paste it into a chat, or into Claude.")
    }

    static func saveCopy(_ url: URL) {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = url.lastPathComponent
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

enum Claude {
    /// Open the Claude app on a new chat, with the request on the clipboard (⌘V, Return).
    /// Not connected yet: connect first. No Claude app: say where to get it.
    static func makeTodaysCapsules() {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(ClaudeLink.dayPrompt, forType: .string)
        guard NSWorkspace.shared.urlForApplication(withBundleIdentifier: ClaudeConnect.desktopBundleID) != nil else {
            ClaudeConnect.connect() // explains how to get the Claude app
            return
        }
        // Without the connection, Claude can't see the recordings: connect first instead of opening an empty chat.
        guard ClaudeConnect.isConnected else {
            AppLog.shared.write("make today's capsules: not connected yet, connecting first")
            let alert = NSAlert()
            alert.messageText = "Connect Claude first"
            alert.informativeText = "Claude needs Context Capsule installed to read your day. Click Connect, then Install in Claude. "
                + "After that, click Make today's capsules again."
            alert.addButton(withTitle: "Connect")
            alert.addButton(withTitle: "Cancel")
            NSApp.activate(ignoringOtherApps: true)
            if alert.runModal() == .alertFirstButtonReturn { ClaudeConnect.connect() }
            return
        }
        AppLog.shared.write("make today's capsules: opening Claude")
        NSWorkspace.shared.open(ClaudeLink.newChat) // a clean chat; the request is on the clipboard
    }
}

private struct CapsuleRow: View {
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
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        guard let d = f.date(from: ymd) else { return ymd }
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
                    ShareLink("More…", item: capsule.url)
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
            Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([capsule.url]) }
        }
    }
}

private struct ProjectRow: View {
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

private struct Panel<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 14, style: .continuous)
        if #available(macOS 26, *) {
            VStack(spacing: 0) { content }.glassEffect(.regular, in: shape)
        } else {
            VStack(spacing: 0) { content }.background(shape.fill(.regularMaterial))
        }
    }
}

struct LibraryView: View {
    @ObservedObject var model: LibraryModel

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Picker("", selection: $model.tab) {
                    Text("Capsules").tag(0)
                    Text("Projects").tag(1)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(width: 220)
                Spacer()
                Button("Make today's capsules") { Claude.makeTodaysCapsules() }
                    .controlSize(.large)
            }

            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    if model.tab == 0 { capsules } else { projectList }
                }
                .padding(.bottom, 8)
            }
        }
        .padding(.horizontal, 22)
        .padding(.bottom, 22)
        .padding(.top, 2)
        .frame(width: 520, height: 560)
        .onAppear { model.reload() }
    }

    @ViewBuilder private var capsules: some View {
        if model.capsules.isEmpty {
            empty("No capsules yet", "Press ⌥C to start contextualizing, then click Make today's capsules.")
        } else {
            ForEach(model.groups, id: \.title) { group in
                VStack(alignment: .leading, spacing: 6) {
                    Text(group.title).font(.system(size: 12, weight: .medium)).foregroundStyle(.secondary).padding(.leading, 4)
                    Panel {
                        ForEach(Array(group.items.enumerated()), id: \.element.url) { i, c in
                            if i > 0 { Divider().padding(.leading, 46) }
                            CapsuleRow(capsule: c)
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder private var projectList: some View {
        if model.projects.isEmpty {
            empty("No projects yet", "They're made for you when Claude sorts your day, or from the menu: Project → New project from this window.")
        } else {
            Panel {
                ForEach(Array(model.projects.enumerated()), id: \.element.id) { i, p in
                    if i > 0 { Divider().padding(.leading, 14) }
                    ProjectRow(project: p, model: model)
                }
            }
        }
    }

    private func empty(_ title: String, _ detail: String) -> some View {
        VStack(spacing: 6) {
            Text(title).font(.system(size: 15, weight: .medium))
            Text(detail).font(.system(size: 13)).foregroundStyle(.secondary).multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 120)
    }
}

/// The "My capsules" window. One at a time; refreshed each time it's shown.
final class LibraryWindow: NSObject, NSWindowDelegate {
    private var window: NSWindow?
    let model: LibraryModel

    init(paths: Paths) { model = LibraryModel(paths: paths) }

    func show() {
        model.reload()
        if let window {
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            return
        }
        let host = NSHostingController(rootView: LibraryView(model: model).background(WindowGlass().ignoresSafeArea()))
        let window = NSWindow(contentViewController: host)
        window.title = "My capsules"
        window.styleMask = [.titled, .closable, .fullSizeContentView]
        window.titlebarAppearsTransparent = true
        window.isMovableByWindowBackground = true
        window.isOpaque = false
        window.backgroundColor = .clear
        window.isReleasedWhenClosed = false
        window.delegate = self
        window.center()
        self.window = window
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    func windowWillClose(_ notification: Notification) { window = nil }
}
