// context-capsule — capture: the Timeline page (SPEC "Timeline") — everything recorded on one day, short enough to read
// at a glance: a header with the time recorded, a day bar coloured by project, where the time went, and each recording
// folded to one line. Opened, a recording lists its stretches (same app and title in a row), voice notes as speech
// bubbles and the stretches that weren't read; Show what was read lists a stretch's reads. Search the day, delete a
// read, a stretch or a whole recording, make a capsule from one recording.
// A lazy list: rows are made only when they scroll into view, and long text is read from the file only on Show all.

import AppKit
import SwiftUI

/// The day on show, what's in it, and what the page can do. Files are read off the main thread.
final class TimelineModel: ObservableObject {
    let paths: Paths
    /// The start of the day on show (local).
    @Published private(set) var day = Calendar.current.startOfDay(for: Date())
    @Published var query = "" { didSet { if query != oldValue { refilter() } } }
    /// The recordings to show: the day, or the day with only what matches the search.
    @Published private(set) var blocks: [Timeline.Block] = []
    /// The whole day (the header, the day bar and Where your time went don't change while searching).
    @Published private(set) var data = Timeline.Day()
    @Published private(set) var loaded = false
    /// Show all: a read's whole text, by moment id.
    @Published var expanded: [String: String] = [:]
    /// Recordings opened or closed by hand (by session id); the one recording now is open unless closed.
    @Published private var opened: Set<String> = []
    @Published private var closed: Set<String> = []
    /// Stretches with Show what was read open (by stretch id).
    @Published private(set) var showingReads: Set<String> = []
    /// The day bar asks the list to scroll to this row.
    @Published var scrollTarget: String?
    /// Days with a recording ("yyyyMMdd"): the dots in the calendar.
    @Published private(set) var recordedDays: Set<String> = []

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

    /// ‹ and ›: a day back or forward; never past today.
    func step(_ days: Int) {
        let cal = Calendar.current
        guard let next = cal.date(byAdding: .day, value: days, to: day), next <= Date() else { return }
        show(next)
    }

    func show(_ date: Date) {
        let start = Calendar.current.startOfDay(for: date)
        guard start != day else { return }
        day = start
        expanded = [:]
        opened = []
        closed = []
        showingReads = []
        loaded = false
        blocks = []
        data = Timeline.Day()
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
            let days = Timeline.recordedDays((try? FileManager.default.contentsOfDirectory(atPath: paths.sessions.path)) ?? [])
            DispatchQueue.main.async {
                guard let self, gen == self.generation else { return }
                self.recordedDays = days
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
            return
        }
        generation += 1
        let gen = generation
        queue.async { [weak self] in
            let found = Timeline.filter(data, query: query)
            DispatchQueue.main.async {
                guard let self, gen == self.generation else { return }
                self.blocks = found
            }
        }
    }

    // MARK: open and closed

    /// While searching, recordings with a match open; else the one recording now, or what was opened by hand.
    func isOpen(_ b: Timeline.Block) -> Bool {
        if opened.contains(b.session) { return true }
        if closed.contains(b.session) { return false }
        return searching ? !b.entries.isEmpty : b.recording
    }

    func toggle(_ b: Timeline.Block) {
        if isOpen(b) {
            opened.remove(b.session)
            closed.insert(b.session)
        } else {
            closed.remove(b.session)
            opened.insert(b.session)
        }
    }

    func toggleReads(_ s: Timeline.Stretch) {
        if showingReads.contains(s.id) { showingReads.remove(s.id) } else { showingReads.insert(s.id) }
    }

    /// Day bar → open the stretch's recording and scroll to it.
    func reveal(_ s: Timeline.Stretch) {
        closed.remove(s.session)
        opened.insert(s.session)
        scrollTarget = Timeline.Entry.stretch(s).id
    }

    func showAll(_ m: Timeline.Moment) {
        let sessions = paths.sessions
        queue.async { [weak self] in
            let text = Timeline.fullText(m, sessions: sessions)
            DispatchQueue.main.async { self?.expanded[m.id] = text ?? m.preview }
        }
    }

    func showLess(_ m: Timeline.Moment) { expanded[m.id] = nil }

    /// The whole recording as loaded (a search leaves only part of it in `blocks`).
    func full(_ b: Timeline.Block) -> Timeline.Block { data.blocks.first { $0.session == b.session } ?? b }

    // MARK: delete (after a confirm)

    func delete(_ m: Timeline.Moment) {
        let what = m.isNote ? (m.kind == Event.voice ? "voice note" : "note") : "read"
        guard confirm("Delete this \(what)? It can't be undone.",
                      "It's removed from this Mac. Capsules already made stay as they are.") else { return }
        attempt { try Timeline.deleteMoment(m, sessions: paths.sessions, state: paths.state) }
        AppLog.shared.write("timeline: a moment deleted")
    }

    func delete(_ s: Timeline.Stretch) {
        let n = s.reads.count
        guard confirm("Delete this stretch? It can't be undone.",
                      "\(n == 1 ? "Its read" : "All \(n) reads") from \(Self.span(s.start, s.end)) "
                        + "\(n == 1 ? "is" : "are") removed from this Mac. Capsules already made stay as they are.") else { return }
        attempt { try Timeline.deleteMoments(s.reads, sessions: paths.sessions, state: paths.state) }
        AppLog.shared.write("timeline: a stretch deleted (\(n) moments)")
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

    static func time(_ d: Date) -> String { DayWords.time(d) }
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

/// Project colours: a fixed order of plain system colours, given out longest first; no project is grey.
/// Home's "Today by project" uses it too, so a project has the same colour there and on the Timeline.
struct Palette {
    static let colors: [Color] = [.blue, .teal, .orange, .green, .pink, .brown]
    static let none = Color.gray.opacity(0.45)
    /// Projects, or apps when nothing has a project (SPEC: Where your time went).
    let byProject: Bool
    let keys: [String]

    init(_ shares: (byProject: Bool, rows: [Timeline.Share])) {
        byProject = shares.byProject
        keys = shares.rows.map(\.key).filter { !$0.isEmpty }
    }

    /// Project ids, longest first (Home's totals); "" (Not sorted) is grey.
    init(projectsLongestFirst ids: [String]) {
        byProject = true
        keys = ids.filter { !$0.isEmpty }
    }

    func key(_ s: Timeline.Stretch) -> String { byProject ? s.project : s.app }
    func color(_ key: String) -> Color {
        guard !key.isEmpty, let i = keys.firstIndex(of: key) else { return Self.none }
        return Self.colors[i % Self.colors.count]
    }
    func color(_ s: Timeline.Stretch) -> Color { color(key(s)) }
}

/// One line of the lazy list. `first`/`last` draw a recording's rows as one rounded panel.
private enum Row: Identifiable {
    case head(Timeline.Block, open: Bool)
    case entry(Timeline.Entry, last: Bool)
    case read(Timeline.Moment, last: Bool)
    case nothing(Timeline.Block)

    var id: String {
        switch self {
        case .head(let b, _): return "head-\(b.session)"
        case .entry(let e, _): return e.id
        case .read(let m, _): return "read-\(m.id)"
        case .nothing(let b): return "nothing-\(b.session)"
        }
    }
}

struct TimelinePage: View {
    @ObservedObject var model: TimelineModel
    @ObservedObject var app: AppModel
    @ObservedObject var library: LibraryModel
    @ObservedObject var ai = AIState.shared

    private let live = Timer.publish(every: 4, on: .main, in: .common).autoconnect()

    private var stretches: [Timeline.Stretch] { model.data.blocks.flatMap(\.stretches) }

    private var rows: [Row] {
        var out: [Row] = []
        for b in model.blocks {
            let open = model.isOpen(b)
            out.append(.head(b, open: open))
            guard open else { continue }
            if b.entries.isEmpty {
                out.append(.nothing(b))
                continue
            }
            for (i, e) in b.entries.enumerated() {
                let lastEntry = i == b.entries.count - 1
                if case .stretch(let s) = e, model.showingReads.contains(s.id) {
                    out.append(.entry(e, last: false))
                    let reads = s.found ?? s.reads
                    for (j, m) in reads.enumerated() { out.append(.read(m, last: lastEntry && j == reads.count - 1)) }
                } else {
                    out.append(.entry(e, last: lastEntry))
                }
            }
        }
        return out
    }

    private func projectName(_ id: String) -> String { library.name(of: id) }

    var body: some View {
        let palette = Palette(Timeline.shares(stretches))
        GeometryReader { geo in
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        header.padding(.bottom, 16)
                        if !model.loaded {
                            ProgressView().controlSize(.small).padding(.top, 8)
                        } else if model.blocks.isEmpty || (model.searching && model.blocks.allSatisfy(\.entries.isEmpty)) {
                            Text(emptyLine).font(.system(size: 13)).foregroundStyle(.secondary).padding(.vertical, 8)
                        }
                        if model.loaded && !stretches.isEmpty {
                            DayBar(stretches: stretches, palette: palette) { model.reveal($0) }
                                .padding(.bottom, 20)
                            TimeWent(shares: Timeline.shares(stretches), palette: palette, name: projectName)
                                .padding(.bottom, 12)
                        }
                        if !model.blocks.isEmpty { Caption(text: "Recordings").padding(.top, 8) }
                        ForEach(rows) { row in rowView(row, palette: palette) }
                    }
                    .padding(.horizontal, 32)
                    .padding(.vertical, 24)
                    .frame(maxWidth: geo.size.width > 900 ? 1120 : 760, alignment: .leading)
                    .frame(maxWidth: .infinity)
                }
                .onChange(of: model.scrollTarget) {
                    guard let target = model.scrollTarget else { return }
                    // Let the opened recording's rows exist first.
                    DispatchQueue.main.async {
                        withAnimation { proxy.scrollTo(target, anchor: .top) }
                        model.scrollTarget = nil
                    }
                }
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

    /// "1 h 50 min recorded · 3 recordings".
    private var summary: String {
        guard model.loaded else { return " " }
        let n = model.data.blocks.count
        if n == 0 { return "Nothing recorded" }
        var parts: [String] = []
        if !stretches.isEmpty { parts.append("\(Timeline.length(Timeline.recorded(model.data.blocks))) recorded") }
        parts.append("\(n) recording\(n == 1 ? "" : "s")")
        return parts.joined(separator: " · ")
    }

    @ViewBuilder private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            DayControl(model: model)
            Text(summary).font(.system(size: 13)).foregroundStyle(.secondary)
            SearchField(text: $model.query, prompt: "Search this day").frame(maxWidth: 320).padding(.top, 8)
        }
    }

    @ViewBuilder private func rowView(_ row: Row, palette: Palette) -> some View {
        switch row {
        case .head(let b, let open):
            let full = model.full(b)
            RecordingHead(block: full, open: open, names: names(full), ai: ai,
                          toggle: { model.toggle(b) },
                          make: { ai.makeCapsule(session: b.session, day: model.dayKey) },
                          delete: { model.delete(full) })
                .modifier(PanelSlice(first: true, last: !open))
                .padding(.top, 10)
        case .nothing:
            Text(model.searching ? "Nothing here matches." : "Nothing was recorded.")
                .font(.system(size: 13)).foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 14).padding(.bottom, 12).padding(.leading, 30)
                .modifier(PanelSlice(first: false, last: true))
        case .entry(let e, let last):
            Group {
                switch e {
                case .stretch(let s):
                    StretchRow(stretch: s, color: palette.color(s), project: s.project.isEmpty ? nil : projectName(s.project),
                               open: model.showingReads.contains(s.id), toggle: { model.toggleReads(s) },
                               delete: { model.delete(s) })
                case .note(let m): NoteRow(moment: m) { model.delete(m) }
                case .gap(let g): GapRow(gap: g)
                }
            }
            .overlay(alignment: .top) { Divider().padding(.leading, 46) }
            .modifier(PanelSlice(first: false, last: last))
        case .read(let m, let last):
            ReadRow(moment: m, full: model.expanded[m.id], showAll: { model.showAll(m) }, showLess: { model.showLess(m) },
                    delete: { model.delete(m) })
                .modifier(PanelSlice(first: false, last: last))
        }
    }

    /// What a folded recording names: its projects by time, or its top 2 apps.
    private func names(_ b: Timeline.Block) -> String {
        let n = Timeline.names(b)
        return (n.byProject ? n.keys.map(projectName) : n.keys).joined(separator: ", ")
    }
}

/// "‹  Thursday 23 October  ›": a day back or forward; the date opens a month calendar. Today, when another day shows.
private struct DayControl: View {
    @ObservedObject var model: TimelineModel
    @State private var picking = false

    /// The same style as Home's date.
    private var label: String { DayWords.long(model.day) }

    var body: some View {
        HStack(spacing: 4) {
            Button { model.step(-1) } label: {
                Image(systemName: "chevron.left").frame(width: 22, height: 22).contentShape(Rectangle())
            }
            .help("The day before")
            Button { picking.toggle() } label: {
                Text(label).font(.system(size: 22, weight: .semibold)).padding(.horizontal, 4).contentShape(Rectangle())
            }
            .help("Pick a day")
            .popover(isPresented: $picking, arrowEdge: .bottom) {
                MonthCalendar(selected: model.day, dots: model.recordedDays) { day in
                    picking = false
                    model.show(day)
                }
                .padding(12)
            }
            Button { model.step(1) } label: {
                Image(systemName: "chevron.right").frame(width: 22, height: 22).contentShape(Rectangle())
            }
            .disabled(model.isToday)
            .help("The next day")
            if !model.isToday {
                Button("Today") { model.show(Date()) }
                    .buttonStyle(.bordered).font(.system(size: 13)).padding(.leading, 10)
            }
        }
        .buttonStyle(.borderless)
        .font(.system(size: 15, weight: .medium))
    }
}

/// A small month calendar: days with a recording have a dot; no future days.
private struct MonthCalendar: View {
    let selected: Date
    let dots: Set<String>
    let pick: (Date) -> Void
    @State private var month: Date

    init(selected: Date, dots: Set<String>, pick: @escaping (Date) -> Void) {
        self.selected = selected
        self.dots = dots
        self.pick = pick
        let cal = Calendar.current
        _month = State(initialValue: cal.dateInterval(of: .month, for: selected)?.start ?? selected)
    }

    private var cal: Calendar { Calendar.current }

    /// The month's days, with blanks before the first so it starts on the right weekday.
    private var cells: [Date?] {
        guard let range = cal.range(of: .day, in: .month, for: month) else { return [] }
        let lead = (cal.component(.weekday, from: month) - cal.firstWeekday + 7) % 7
        return Array(repeating: nil, count: lead) + range.compactMap { cal.date(byAdding: .day, value: $0 - 1, to: month) }
    }

    private var weekdays: [String] {
        let s = cal.veryShortStandaloneWeekdaySymbols
        return Array(s[(cal.firstWeekday - 1)...] + s[..<(cal.firstWeekday - 1)])
    }

    private var thisMonth: Bool { cal.isDate(month, equalTo: Date(), toGranularity: .month) }

    var body: some View {
        VStack(spacing: 8) {
            HStack {
                Text(month.formatted(.dateTime.month(.wide).year())).font(.system(size: 13, weight: .semibold))
                Spacer()
                Button { move(-1) } label: { Image(systemName: "chevron.left") }.help("The month before")
                Button { move(1) } label: { Image(systemName: "chevron.right") }.disabled(thisMonth).help("The next month")
            }
            .buttonStyle(.borderless)
            let columns = Array(repeating: GridItem(.fixed(28), spacing: 2), count: 7)
            LazyVGrid(columns: columns, spacing: 2) {
                ForEach(Array(weekdays.enumerated()), id: \.offset) { _, d in
                    Text(d).font(.system(size: 11)).foregroundStyle(.secondary).frame(height: 18)
                }
                ForEach(Array(cells.enumerated()), id: \.offset) { _, day in
                    if let day { dayCell(day) } else { Color.clear.frame(height: 30) }
                }
            }
        }
        .frame(width: 214)
    }

    private func dayCell(_ day: Date) -> some View {
        let isSelected = cal.isDate(day, inSameDayAs: selected)
        let isToday = cal.isDateInToday(day)
        let future = day > Date()
        let dot = dots.contains(Timeline.dayKey(day))
        return Button { pick(day) } label: {
            VStack(spacing: 1) {
                Text("\(cal.component(.day, from: day))")
                    .font(.system(size: 12, weight: isToday ? .semibold : .regular).monospacedDigit())
                    .foregroundStyle(isSelected ? Color.white : isToday ? Color.accentColor : future ? Color.secondary : Color.primary)
                Circle().fill(isSelected ? Color.white : Color.secondary).frame(width: 4, height: 4).opacity(dot ? 1 : 0)
            }
            .frame(width: 28, height: 30)
            .background(Circle().fill(isSelected ? Color.accentColor : Color.clear).frame(width: 27, height: 27))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(future)
        .help(dot ? "Recorded this day" : "")
    }

    private func move(_ months: Int) {
        if let m = cal.date(byAdding: .month, value: months, to: month) { month = m }
    }
}

/// A piece of a Panel: the rows of one recording share one rounded background, drawn row by row so the list stays lazy.
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

/// The day in one row: each stretch a segment in its project's colour, hour labels below. Click a segment to see it.
private struct DayBar: View {
    let stretches: [Timeline.Stretch]
    let palette: Palette
    let reveal: (Timeline.Stretch) -> Void

    var body: some View {
        if let range = Timeline.barRange(stretches) {
            let total = max(1, range.to.timeIntervalSince(range.from))
            let hours = Int((total / 3600).rounded())
            let step = max(1, Int((Double(hours) / 8).rounded(.up)))
            VStack(spacing: 4) {
                GeometryReader { geo in
                    let w = geo.size.width
                    ZStack(alignment: .leading) {
                        RoundedRectangle(cornerRadius: 4).fill(Color.primary.opacity(0.06))
                        ForEach(stretches) { s in
                            let x = w * CGFloat(s.start.timeIntervalSince(range.from) / total)
                            let width = max(3, w * CGFloat(s.length / total))
                            Rectangle().fill(palette.color(s))
                                .frame(width: width, height: 18)
                                .contentShape(Rectangle())
                                .help("\(TimelineModel.span(s.start, s.end)) · \(s.name)")
                                .onTapGesture { reveal(s) }
                                .offset(x: min(x, w - width))
                        }
                    }
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                }
                .frame(height: 18)
                GeometryReader { geo in
                    ForEach(Array(stride(from: 0, through: hours, by: step)), id: \.self) { h in
                        let d = range.from.addingTimeInterval(Double(h) * 3600)
                        Text(d.formatted(.dateTime.hour()))
                            .font(.system(size: 10).monospacedDigit()).foregroundStyle(.secondary)
                            .fixedSize()
                            .position(x: geo.size.width * CGFloat(Double(h) * 3600 / total), y: 6)
                    }
                }
                .frame(height: 12)
            }
        }
    }
}

/// Where your time went: one row per project (or app), longest first.
private struct TimeWent: View {
    let shares: (byProject: Bool, rows: [Timeline.Share])
    let palette: Palette
    let name: (String) -> String

    var body: some View {
        let most = shares.rows.map(\.seconds).max() ?? 0
        VStack(alignment: .leading, spacing: 8) {
            Caption(text: "Where your time went")
            VStack(alignment: .leading, spacing: 8) {
                ForEach(shares.rows, id: \.key) { row in
                    HStack(spacing: 12) {
                        Text(label(row.key)).font(.system(size: 13)).foregroundStyle(row.key.isEmpty ? .secondary : .primary)
                            .frame(width: 150, alignment: .leading).lineLimit(1)
                        Capsule().fill(Color.primary.opacity(0.06))
                            .frame(height: 8)
                            .overlay(alignment: .leading) {
                                GeometryReader { geo in
                                    Capsule().fill(palette.color(row.key))
                                        .frame(width: max(6, geo.size.width * CGFloat(most > 0 ? row.seconds / most : 0)))
                                }
                            }
                        Text(Timeline.length(row.seconds)).font(.system(size: 12).monospacedDigit()).foregroundStyle(.secondary)
                            .frame(width: 90, alignment: .trailing)
                    }
                }
            }
            .padding(14)
            .background(RoundedRectangle(cornerRadius: 14, style: .continuous).fill(Color.primary.opacity(0.045)))
        }
    }

    private func label(_ key: String) -> String {
        if key.isEmpty { return shares.byProject ? "Not sorted" : "Other" }
        return shares.byProject ? name(key) : key
    }
}

/// A recording on one line: "10:00 – 11:40 · 1 h 40 min · Website, Client report", Make a capsule, and Delete.
/// Click it to open or close.
private struct RecordingHead: View {
    let block: Timeline.Block
    let open: Bool
    let names: String
    @ObservedObject var ai: AIState
    let toggle: () -> Void
    let make: () -> Void
    let delete: () -> Void

    private var line: String {
        var parts = [TimelineModel.span(block.start, block.end), Timeline.length(block.end.timeIntervalSince(block.start))]
        if !names.isEmpty { parts.append(names) }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "chevron.right")
                .font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary)
                .rotationEffect(.degrees(open ? 90 : 0))
                .frame(width: 20)
            if block.recording {
                Circle().fill(Color.red).frame(width: 7, height: 7).help("Recording now")
            }
            Text(line).font(.system(size: 13, weight: .semibold)).lineLimit(1).truncationMode(.tail)
            if block.recording { Text("Recording now").font(.system(size: 12)).foregroundStyle(.secondary).fixedSize() }
            Spacer(minLength: 8)
            Button("Make a capsule", action: make)
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
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
        .onTapGesture(perform: toggle)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(line + (open ? ", open" : ", closed"))
    }
}

/// One stretch: app and title, the time and how long, where, a peek at the text, how many reads; Show what was read.
private struct StretchRow: View {
    let stretch: Timeline.Stretch
    let color: Color
    let project: String?
    let open: Bool
    let toggle: () -> Void
    let delete: () -> Void
    @State private var hover = false

    private var details: String {
        var parts: [String] = []
        if !stretch.location.isEmpty { parts.append(stretch.location) }
        if let project { parts.append(project) }
        return parts.joined(separator: " · ")
    }

    private var reads: String {
        let n = stretch.reads.count
        if let found = stretch.found { return "\(found.count) of \(n) read\(n == 1 ? "" : "s") match" }
        return "\(n) read\(n == 1 ? "" : "s")"
    }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Group {
                if let icon = AppIcons.icon(bundle: stretch.bundle, app: stretch.app) {
                    Image(nsImage: icon).resizable().interpolation(.high)
                } else {
                    Image(systemName: "macwindow").foregroundStyle(.secondary)
                }
            }
            .frame(width: 20, height: 20)
            .padding(.top, 1)
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Circle().fill(color).frame(width: 7, height: 7)
                    Text(stretch.name).font(.system(size: 13, weight: .medium)).lineLimit(1)
                    Spacer(minLength: 8)
                    Text("\(TimelineModel.span(stretch.start, stretch.end)) · \(Timeline.length(stretch.length))")
                        .font(.system(size: 12).monospacedDigit()).foregroundStyle(.secondary).lineLimit(1).fixedSize()
                }
                if !details.isEmpty {
                    Text(details).font(.system(size: 12)).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                }
                if !stretch.peek.isEmpty {
                    Text(stretch.peek).font(.system(size: 12)).lineLimit(2).textSelection(.enabled)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 2)
                }
                HStack(spacing: 10) {
                    Text(reads).font(.system(size: 12)).foregroundStyle(.secondary)
                    Button(open ? "Hide what was read" : "Show what was read", action: toggle)
                        .buttonStyle(.link).font(.system(size: 12))
                }
                .padding(.top, 2)
            }
            RowMenu(title: "Delete this stretch…", delete: delete).opacity(hover ? 1 : 0.35)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .contentShape(Rectangle())
        .onHover { hover = $0 }
        .contextMenu { Button("Delete this stretch…", role: .destructive, action: delete) }
    }
}

/// One read inside an open stretch: time, where, Looked at, the first 3 lines of text with Show all.
private struct ReadRow: View {
    let moment: Timeline.Moment
    let full: String?
    let showAll: () -> Void
    let showLess: () -> Void
    let delete: () -> Void
    @State private var hover = false

    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            Text(TimelineModel.span(moment.t, moment.end))
                .font(.system(size: 11).monospacedDigit()).foregroundStyle(.secondary)
                .frame(width: 110, alignment: .leading)
                .padding(.top, 1)
            VStack(alignment: .leading, spacing: 3) {
                if moment.lookedAt || !moment.location.isEmpty {
                    HStack(spacing: 8) {
                        if moment.lookedAt {
                            Text("Looked at")
                                .font(.system(size: 11, weight: .medium)).foregroundStyle(.secondary)
                                .padding(.horizontal, 6).padding(.vertical, 1)
                                .background(Capsule().strokeBorder(Color.secondary.opacity(0.5)))
                                .help("The mouse was over it on another screen. You weren't typing in it.")
                        }
                        if !moment.location.isEmpty {
                            Text(moment.location).font(.system(size: 12)).foregroundStyle(.secondary)
                                .lineLimit(1).truncationMode(.middle)
                        }
                    }
                }
                if let full {
                    Text(full).font(.system(size: 12)).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                    Button("Show less", action: showLess).buttonStyle(.link).font(.system(size: 12))
                } else if !moment.preview.isEmpty {
                    Text(moment.preview).font(.system(size: 12)).textSelection(.enabled).lineLimit(3)
                        .fixedSize(horizontal: false, vertical: true)
                    if moment.more { Button("Show all", action: showAll).buttonStyle(.link).font(.system(size: 12)) }
                } else {
                    Text("Nothing new on screen").font(.system(size: 12)).foregroundStyle(.tertiary)
                }
            }
            Spacer(minLength: 8)
            RowMenu(title: "Delete this read…", delete: delete).opacity(hover ? 1 : 0.35)
        }
        .padding(.leading, 46)
        .padding(.trailing, 14)
        .padding(.vertical, 6)
        .contentShape(Rectangle())
        .onHover { hover = $0 }
        .contextMenu { Button("Delete this read…", role: .destructive, action: delete) }
    }
}

private struct RowMenu: View {
    let title: String
    let delete: () -> Void
    var body: some View {
        Menu {
            Button(title, role: .destructive, action: delete)
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

    private var deleteTitle: String { moment.kind == Event.voice ? "Delete this voice note…" : "Delete this note…" }

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
            RowMenu(title: deleteTitle, delete: delete).opacity(hover ? 1 : 0.35)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .contentShape(Rectangle())
        .onHover { hover = $0 }
        .contextMenu { Button(deleteTitle, role: .destructive, action: delete) }
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
