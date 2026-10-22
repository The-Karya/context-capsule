// context-capsule — capture: the Timeline page (SPEC "Timeline") — everything recorded on one day, raw: a block per
// recording with its moments, voice notes as speech bubbles, the stretches that weren't read, and "Off" between
// recordings. Search the day, delete a moment or a whole recording, make a capsule from one recording.
// A lazy list: rows are made only when they scroll into view, and long text is read from the file only on Show all.

import AppKit
import SwiftUI

/// The day on show, what's in it, and what the page can do. Files are read off the main thread.
final class TimelineModel: ObservableObject {
    let paths: Paths
    /// The start of the day on show (local).
    @Published private(set) var day = Calendar.current.startOfDay(for: Date())
    @Published var query = "" { didSet { if query != oldValue { refilter() } } }
    /// The blocks to show: the day, or what matches the search.
    @Published private(set) var blocks: [Timeline.Block] = []
    /// "Off" after a block (by its session id); none while searching.
    @Published private(set) var offs: [String: Timeline.Off] = [:]
    @Published private(set) var loaded = false
    /// Show all: a moment's whole text, by moment id.
    @Published var expanded: [String: String] = [:]

    private var data = Timeline.Day()
    private var generation = 0
    private let queue = DispatchQueue(label: "com.thekarya.contextcapsule.timeline", qos: .userInitiated)

    init(paths: Paths) { self.paths = paths }

    var isToday: Bool { Calendar.current.isDateInToday(day) }
    var isYesterday: Bool { Calendar.current.isDateInYesterday(day) }
    var searching: Bool { !Timeline.words(query).isEmpty }
    /// YYYY-MM-DD, for Copy my day.
    var dayKey: String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd"
        return f.string(from: day)
    }

    func show(_ date: Date) {
        let start = Calendar.current.startOfDay(for: date)
        guard start != day else { return }
        day = start
        expanded = [:]
        loaded = false
        blocks = []
        offs = [:]
        reload()
    }

    /// Reads the whole day again.
    func reload() {
        generation += 1
        let gen = generation, day = self.day, paths = self.paths
        queue.async { [weak self] in
            let projects = Projects.load(from: paths.projects)
            let loaded = Timeline.load(day: day, sessions: paths.sessions, projects: projects,
                                       recording: Timeline.recordingSession(state: paths.state))
            DispatchQueue.main.async {
                guard let self, gen == self.generation else { return }
                self.data = loaded
                self.loaded = true
                self.refilter()
            }
        }
    }

    /// While recording: only the recording's own block is read again (the rest of the day doesn't change).
    func refreshLive() {
        guard isToday, loaded else { return reload() }
        guard let id = Timeline.recordingSession(state: paths.state), data.blocks.contains(where: { $0.session == id }) else {
            return reload() // a new recording started, or one just ended
        }
        generation += 1
        let gen = generation, day = self.day, paths = self.paths
        queue.async { [weak self] in
            let cal = Calendar.current
            let from = cal.startOfDay(for: day)
            let to = cal.date(byAdding: .day, value: 1, to: from) ?? from.addingTimeInterval(86_400)
            let block = Timeline.block(id, dir: paths.sessions.appendingPathComponent(id, isDirectory: true), from: from, to: to,
                                       projects: Projects.load(from: paths.projects), recording: true)
            DispatchQueue.main.async {
                guard let self, gen == self.generation, let (block, hay) = block,
                      let i = self.data.blocks.firstIndex(where: { $0.session == id }) else { return }
                if self.data.blocks[i] == block { return }
                self.data.blocks[i] = block
                self.data.haystacks.merge(hay) { _, new in new }
                self.refilter()
            }
        }
    }

    private func refilter() {
        let data = self.data, query = self.query
        guard !Timeline.words(query).isEmpty else {
            blocks = data.blocks
            offs = Dictionary(Timeline.offs(data.blocks).map { ($0.after, $0) }, uniquingKeysWith: { a, _ in a })
            return
        }
        generation += 1
        let gen = generation
        queue.async { [weak self] in
            let found = Timeline.filter(data, query: query)
            DispatchQueue.main.async {
                guard let self, gen == self.generation else { return }
                self.blocks = found
                self.offs = [:]
            }
        }
    }

    func showAll(_ m: Timeline.Moment) {
        let sessions = paths.sessions
        queue.async { [weak self] in
            let text = Timeline.fullText(m, sessions: sessions)
            DispatchQueue.main.async { self?.expanded[m.id] = text ?? m.preview }
        }
    }

    func showLess(_ m: Timeline.Moment) { expanded[m.id] = nil }

    // MARK: delete (after a confirm)

    func delete(_ m: Timeline.Moment) {
        guard confirm("Delete this moment? It can't be undone.",
                      "It's removed from this Mac. Capsules already made stay as they are.") else { return }
        attempt { try Timeline.deleteMoment(m, sessions: paths.sessions, state: paths.state) }
        AppLog.shared.write("timeline: a moment deleted")
    }

    func delete(_ b: Timeline.Block) {
        guard confirm("Delete this recording? It can't be undone.",
                      "Everything recorded from \(Self.time(b.start)) to \(Self.time(b.end)) is removed from this Mac. "
                        + "Capsules already made stay as they are.") else { return }
        attempt { try Timeline.deleteSession(b.session, sessions: paths.sessions, state: paths.state) }
        AppLog.shared.write("timeline: a recording deleted")
    }

    private func attempt(_ work: () throws -> Void) {
        do {
            try work()
        } catch Timeline.DeleteError.recording {
            tell("Stop first", "This recording is still going. Stop it, then delete.")
        } catch Timeline.DeleteError.notFound {
            tell("Already gone", "It isn't there anymore. The page is up to date now.")
        } catch {
            AppLog.shared.write("timeline: cannot delete: \(error.localizedDescription)")
            tell("Couldn't delete", error.localizedDescription)
        }
        reload()
    }

    private func confirm(_ title: String, _ text: String) -> Bool {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = text
        alert.alertStyle = .warning
        let delete = alert.addButton(withTitle: "Delete")
        delete.hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel")
        return alert.runModal() == .alertFirstButtonReturn
    }

    private func tell(_ title: String, _ text: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = text
        alert.addButton(withTitle: "OK")
        alert.runModal()
    }

    static func time(_ d: Date) -> String { d.formatted(date: .omitted, time: .shortened) }
    static func span(_ a: Date, _ b: Date) -> String {
        let x = time(a), y = time(b)
        return x == y ? x : "\(x) – \(y)"
    }
}

/// App icons, looked up once per app.
private enum AppIcons {
    static var cache: [String: NSImage] = [:]
    static func icon(bundle: String, app: String) -> NSImage? {
        let key = bundle.isEmpty ? app : bundle
        if key.isEmpty { return nil }
        if let hit = cache[key] { return hit }
        let ws = NSWorkspace.shared
        let url = bundle.isEmpty ? nil : ws.urlForApplication(withBundleIdentifier: bundle)
        let image = url.map { ws.icon(forFile: $0.path) }
        if let image { cache[key] = image }
        return image
    }
}

/// One line of the lazy list.
private enum Row: Identifiable {
    case head(Timeline.Block)
    case item(Timeline.Item, block: Timeline.Block, first: Bool, last: Bool)
    case nothing(Timeline.Block)
    case off(Timeline.Off)

    var id: String {
        switch self {
        case .head(let b): return "head-\(b.session)"
        case .item(let i, _, _, _): return i.id
        case .nothing(let b): return "nothing-\(b.session)"
        case .off(let o): return "off-\(o.after)"
        }
    }
}

struct TimelinePage: View {
    @ObservedObject var model: TimelineModel
    @ObservedObject var app: AppModel
    @ObservedObject var library: LibraryModel
    @ObservedObject var ai = AIState.shared

    private let live = Timer.publish(every: 4, on: .main, in: .common).autoconnect()

    private var rows: [Row] {
        var out: [Row] = []
        for b in model.blocks {
            out.append(.head(b))
            if b.items.isEmpty {
                out.append(.nothing(b))
            } else {
                for (i, item) in b.items.enumerated() {
                    out.append(.item(item, block: b, first: i == 0, last: i == b.items.count - 1))
                }
            }
            if let off = model.offs[b.session] { out.append(.off(off)) }
        }
        return out
    }

    private func projectName(_ id: String) -> String { library.name(of: id) }

    var body: some View {
        GeometryReader { geo in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    header.padding(.bottom, 12)
                    if !model.loaded {
                        ProgressView().controlSize(.small).padding(.top, 8)
                    } else if model.blocks.isEmpty {
                        Text(emptyLine).font(.system(size: 13)).foregroundStyle(.secondary).padding(.top, 8)
                    }
                    ForEach(rows) { row in rowView(row) }
                }
                .padding(.horizontal, 32)
                .padding(.vertical, 24)
                .frame(maxWidth: geo.size.width > 900 ? 1120 : 760, alignment: .leading)
                .frame(maxWidth: .infinity)
            }
        }
        .onAppear { model.reload() }
        .onReceive(live) { _ in if app.recording && model.isToday { model.refreshLive() } }
        .onChange(of: app.recording) { model.reload() }
    }

    private var emptyLine: String {
        if model.searching { return "Nothing on this day matches." }
        return model.isToday ? "Nothing recorded today yet. Start contextualizing (\(app.recordKey)) when you begin work."
                             : "Nothing recorded on this day."
    }

    @ViewBuilder private var header: some View {
        VStack(alignment: .leading, spacing: 12) {
            Title(text: "Timeline")
            HStack(spacing: 10) {
                Picker("", selection: Binding(get: { model.isToday ? 0 : model.isYesterday ? 1 : -1 }, set: { pick in
                    let cal = Calendar.current
                    model.show(pick == 0 ? Date() : cal.date(byAdding: .day, value: -1, to: cal.startOfDay(for: Date())) ?? Date())
                })) {
                    Text("Today").tag(0)
                    Text("Yesterday").tag(1)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .fixedSize()
                DatePicker("", selection: Binding(get: { model.day }, set: { model.show($0) }), in: ...Date(),
                           displayedComponents: .date)
                    .labelsHidden()
                    .datePickerStyle(.field)
                    .fixedSize()
                    .help("Pick a day")
                SearchField(text: $model.query, prompt: "Search this day")
            }
            Text("Everything recorded on this day, as it was read. It stays on this Mac.")
                .font(.system(size: 12)).foregroundStyle(.secondary)
        }
    }

    @ViewBuilder private func rowView(_ row: Row) -> some View {
        switch row {
        case .head(let b): BlockHead(block: b, project: b.project.isEmpty ? nil : projectName(b.project), ai: ai,
                                     make: { ai.makeCapsule(session: b.session, day: model.dayKey) },
                                     delete: { model.delete(b) })
        case .nothing:
            Text("Nothing was recorded.")
                .font(.system(size: 13)).foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(14)
                .modifier(PanelSlice(first: true, last: true))
        case .item(let item, _, let first, let last):
            Group {
                switch item {
                case .moment(let m) where m.isNote: NoteRow(moment: m) { model.delete(m) }
                case .moment(let m):
                    MomentRow(moment: m, project: m.project.isEmpty ? nil : projectName(m.project), full: model.expanded[m.id],
                              showAll: { model.showAll(m) }, showLess: { model.showLess(m) }, delete: { model.delete(m) })
                case .gap(let g): GapRow(gap: g)
                }
            }
            .overlay(alignment: .top) { if !first { Divider().padding(.leading, 46) } }
            .modifier(PanelSlice(first: first, last: last))
        case .off(let o):
            HStack(spacing: 10) {
                Rectangle().fill(Color.primary.opacity(0.12)).frame(width: 24, height: 1)
                Text("Off  \(TimelineModel.span(o.from, o.to))").font(.system(size: 12)).foregroundStyle(.secondary)
                Rectangle().fill(Color.primary.opacity(0.12)).frame(height: 1)
            }
            .padding(.vertical, 14)
        }
    }
}

/// A piece of a Panel: the rows of one block share one rounded background, drawn row by row so the list stays lazy.
private struct PanelSlice: ViewModifier {
    let first: Bool
    let last: Bool
    func body(content: Content) -> some View {
        let r: CGFloat = 14
        let shape = UnevenRoundedRectangle(topLeadingRadius: first ? r : 0, bottomLeadingRadius: last ? r : 0,
                                           bottomTrailingRadius: last ? r : 0, topTrailingRadius: first ? r : 0,
                                           style: .continuous)
        content.background(shape.fill(Color.primary.opacity(0.045)))
    }
}

/// "10:02 – 11:30 · 12 moments · Website", Make a capsule from this, and a menu with Delete.
private struct BlockHead: View {
    let block: Timeline.Block
    let project: String?
    @ObservedObject var ai: AIState
    let make: () -> Void
    let delete: () -> Void

    private var line: String {
        var parts = [TimelineModel.span(block.start, block.end)]
        parts.append("\(block.moments) moment\(block.moments == 1 ? "" : "s")")
        if let project { parts.append(project) }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        HStack(spacing: 8) {
            if block.recording {
                Circle().fill(Color.red).frame(width: 7, height: 7).help("Recording now")
            }
            Text(line).font(.system(size: 13, weight: .semibold)).lineLimit(1)
            if block.recording { Text("Recording now").font(.system(size: 12)).foregroundStyle(.secondary) }
            Spacer()
            Button("Make a capsule from this", action: make)
                .disabled(ai.working)
                .help("Your AI makes a capsule from this recording.")
            Menu {
                Button("Delete this recording…", role: .destructive, action: delete)
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
            .help("More")
        }
        .padding(.leading, 4)
        .padding(.top, 18)
        .padding(.bottom, 8)
    }
}

/// One moment: time, app icon, title, where, project, the first 3 lines of text with Show all.
private struct MomentRow: View {
    let moment: Timeline.Moment
    let project: String?
    let full: String?
    let showAll: () -> Void
    let showLess: () -> Void
    let delete: () -> Void
    @State private var hover = false

    private var details: String {
        var parts: [String] = []
        if !moment.title.isEmpty && !moment.app.isEmpty { parts.append(moment.app) }
        if !moment.location.isEmpty { parts.append(moment.location) }
        if let project { parts.append(project) }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Group {
                if let icon = AppIcons.icon(bundle: moment.bundle, app: moment.app) {
                    Image(nsImage: icon).resizable().interpolation(.high)
                } else {
                    Image(systemName: "macwindow").foregroundStyle(.secondary)
                }
            }
            .frame(width: 20, height: 20)
            .padding(.top, 1)
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(moment.title.isEmpty ? (moment.app.isEmpty ? "A window" : moment.app) : moment.title)
                        .font(.system(size: 13, weight: .medium)).lineLimit(1)
                    if moment.lookedAt {
                        Text("Looked at")
                            .font(.system(size: 11, weight: .medium)).foregroundStyle(.secondary)
                            .padding(.horizontal, 6).padding(.vertical, 1)
                            .background(Capsule().strokeBorder(Color.secondary.opacity(0.5)))
                            .help("The mouse was over it on another screen. You weren't typing in it.")
                    }
                    Spacer(minLength: 8)
                    Text(TimelineModel.span(moment.t, moment.end))
                        .font(.system(size: 12).monospacedDigit()).foregroundStyle(.secondary).lineLimit(1)
                }
                if !details.isEmpty {
                    Text(details).font(.system(size: 12)).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                }
                if let full {
                    Text(full).font(.system(size: 12)).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 2)
                    Button("Show less", action: showLess).buttonStyle(.link).font(.system(size: 12))
                } else if !moment.preview.isEmpty {
                    Text(moment.preview).font(.system(size: 12)).textSelection(.enabled).lineLimit(3)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 2)
                    if moment.more { Button("Show all", action: showAll).buttonStyle(.link).font(.system(size: 12)) }
                } else {
                    Text("Nothing new on screen").font(.system(size: 12)).foregroundStyle(.tertiary)
                }
            }
            MomentMenu(delete: delete).opacity(hover ? 1 : 0.35)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .contentShape(Rectangle())
        .onHover { hover = $0 }
        .contextMenu { Button("Delete this moment…", role: .destructive, action: delete) }
    }
}

private struct MomentMenu: View {
    let delete: () -> Void
    var body: some View {
        Menu {
            Button("Delete this moment…", role: .destructive, action: delete)
        } label: {
            Image(systemName: "ellipsis")
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .help("More")
    }
}

/// A voice or typed note: the user's own words, in full, in a speech bubble.
private struct NoteRow: View {
    let moment: Timeline.Moment
    let delete: () -> Void
    @State private var hover = false

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: moment.kind == Event.voice ? "mic" : "keyboard")
                .foregroundStyle(.secondary).frame(width: 20, height: 20).padding(.top, 6)
            VStack(alignment: .leading, spacing: 4) {
                Text(moment.preview.isEmpty ? "…" : moment.preview)
                    .font(.system(size: 13))
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 12).padding(.vertical, 8)
                    .background(RoundedRectangle(cornerRadius: 12, style: .continuous).fill(Color.accentColor.opacity(0.13)))
                Text((moment.kind == Event.voice ? "You said" : "You typed") + " · " + TimelineModel.time(moment.t)
                     + (moment.app.isEmpty ? "" : " · in \(moment.app)"))
                    .font(.system(size: 11)).foregroundStyle(.secondary).padding(.leading, 4)
            }
            Spacer(minLength: 8)
            MomentMenu(delete: delete).opacity(hover ? 1 : 0.35)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .contentShape(Rectangle())
        .onHover { hover = $0 }
        .contextMenu { Button("Delete this moment…", role: .destructive, action: delete) }
    }
}

/// "Not read 10:40 – 10:55: a private app was in front".
private struct GapRow: View {
    let gap: Timeline.Gap
    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "eye.slash").foregroundStyle(.secondary).frame(width: 20)
            Text("Not read \(TimelineModel.span(gap.t, gap.end)): \(gap.message)")
                .font(.system(size: 12)).foregroundStyle(.secondary)
            Spacer()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
    }
}
