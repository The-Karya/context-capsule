// context-capsule — capture: the macOS system picker (SCContentSharingPicker). The user chooses a window or a
// display — and can add more screens (up to 4), one stream each, at most one frame per second each. Because the
// user picked the content, macOS asks for no Screen Recording permission. Frames stay in memory.

import AppKit
import CoreMedia
import ScreenCaptureKit

final class PickerCapture: NSObject, SCContentSharingPickerObserver, SCStreamDelegate, SCStreamOutput {
    enum Outcome {
        case picked                 // the user chose content and the stream is running
        case cancelled              // the user closed the picker
        case failed(String)         // the picker or the stream could not start
        case stopped(String?)       // every stream ended (nil: the user stopped sharing in macOS)
        case added(Int)             // another screen is shared too (how many now)
        case addCancelled           // the user closed the picker when adding a screen
        case removed(Int)           // one of several shared screens stopped (how many are left)
    }

    static let maxSources = 4

    /// Called on the main thread.
    var onOutcome: ((Outcome) -> Void)?

    private(set) var isPicking = false
    private var adding = false
    var isCapturing: Bool { !sources.isEmpty }
    var sourceCount: Int { sources.count }

    /// One shared screen or window: its stream, what was picked, and its latest frame.
    private final class Source {
        let id: Int
        let stream: SCStream
        var filter: SCContentFilter
        var latest: CVPixelBuffer?
        var latestAt: Date?
        var screenRect: CGRect?
        init(id: Int, stream: SCStream, filter: SCContentFilter) {
            self.id = id
            self.stream = stream
            self.filter = filter
        }
    }

    private var sources: [Source] = [] // changed on the main thread, read under `lock`
    private var nextID = 1
    private var observing = false
    private let frames = DispatchQueue(label: "com.thekarya.contextcapsule.frames")
    private let lock = NSLock()

    // MARK: main thread

    /// Show the system picker (or bring it back to the front if it is already open).
    func present() {
        configurePicker()
        isPicking = true
        adding = false
        NSApp.activate(ignoringOtherApps: true)
        SCContentSharingPicker.shared.present()
    }

    /// While recording: pick one more screen or window to share too.
    func presentAdd() {
        guard isCapturing, sources.count < Self.maxSources else { return }
        configurePicker()
        adding = true
        NSApp.activate(ignoringOtherApps: true)
        SCContentSharingPicker.shared.present()
    }

    private func configurePicker() {
        let picker = SCContentSharingPicker.shared
        if !observing {
            picker.add(self)
            observing = true
        }
        var config = SCContentSharingPickerConfiguration()
        var modes: SCContentSharingPickerMode = [.singleWindow, .singleDisplay]
        // Several windows only where we can tell which ones were picked (SCContentFilter.includedWindows).
        if #available(macOS 15.2, *) { modes.insert(.multipleWindows) }
        config.allowedPickerModes = modes
        config.excludedBundleIDs = [Bundle.main.bundleIdentifier ?? "com.thekarya.contextcapsule"]
        config.allowsChangingSelectedContent = true
        picker.defaultConfiguration = config
        picker.maximumStreamCount = Self.maxSources
        picker.isActive = true
    }

    /// Stop every stream and take the picker down. Safe to call in any state.
    func stop() {
        isPicking = false
        adding = false
        lock.lock()
        let all = sources
        sources = []
        lock.unlock()
        for s in all { s.stream.stopCapture { _ in } }
        SCContentSharingPicker.shared.isActive = false
    }

    // MARK: any thread

    /// Everything shared, placed on screen as of each source's latest frame.
    func pickedContents() -> [PickedContent] {
        lock.lock()
        let snapshot = sources.map { ($0.id, $0.filter, $0.screenRect) }
        lock.unlock()
        let displays = Screen.displays()
        return snapshot.map { id, filter, rect in
            var pick = Self.describe(filter, screenRect: rect, displays: displays)
            pick.source = id
            return pick
        }
    }

    /// The first shared content (for logs).
    func pickedContent() -> PickedContent? { pickedContents().first }

    /// The latest frame of one source and when it arrived. Frames only arrive when that content changes, so
    /// right after an app switch it may still show the previous app (see `Freshness`).
    func latestFrame(source: Int) -> (buffer: CVPixelBuffer, at: Date)? {
        lock.lock()
        defer { lock.unlock() }
        guard let s = sources.first(where: { $0.id == source }), let latest = s.latest, let at = s.latestAt else { return nil }
        return (latest, at)
    }

    /// Title of a picked window, when macOS tells us which windows were picked (15.2+).
    func pickedWindowTitle(id: UInt32) -> String? {
        guard #available(macOS 15.2, *) else { return nil }
        lock.lock()
        let filters = sources.map(\.filter)
        lock.unlock()
        for f in filters { if let w = f.includedWindows.first(where: { $0.windowID == id }) { return w.title } }
        return nil
    }

    /// For a display, `rect` is its global frame (empty if unknown: then nothing is read).
    static func describe(_ filter: SCContentFilter, screenRect: CGRect?, displays: [CGRect]) -> PickedContent {
        let style: PickStyle
        switch filter.style {
        case .display: style = .display
        case .window: style = .window
        case .application: style = .application
        default: style = .unknown
        }
        var pick = PickedContent(style: style, rect: screenRect ?? filter.contentRect)
        if #available(macOS 15.2, *) {
            if style == .window { pick.windowIDs = filter.includedWindows.map(\.windowID) }
            if style == .application { pick.pids = filter.includedApplications.map(\.processID) }
            if style == .display, let display = filter.includedDisplays.first {
                pick.rect = CGDisplayBounds(display.displayID) // global, right for any display
                return pick
            }
        }
        if style == .display {
            // contentRect can be (0,0)-based; a second display then looked like the first.
            pick.rect = Membership.resolveDisplay(pick.rect, displays: displays) ?? .zero
        }
        return pick
    }

    // MARK: SCContentSharingPickerObserver (any thread)

    func contentSharingPicker(_ picker: SCContentSharingPicker, didCancelFor stream: SCStream?) {
        DispatchQueue.main.async {
            // Cancelling a change to a running stream keeps the old selection.
            guard stream == nil else { return }
            if self.adding {
                self.adding = false
                self.onOutcome?(.addCancelled)
                return
            }
            guard self.isPicking else { return }
            self.isPicking = false
            if self.sources.isEmpty { SCContentSharingPicker.shared.isActive = false }
            self.onOutcome?(.cancelled)
        }
    }

    func contentSharingPicker(_ picker: SCContentSharingPicker, didUpdateWith filter: SCContentFilter,
                              for stream: SCStream?) {
        DispatchQueue.main.async {
            if let stream, let source = self.sources.first(where: { $0.stream === stream }) {
                // The user changed one selection from the menu-bar sharing icon.
                self.lock.lock()
                source.filter = filter
                source.latest = nil
                source.latestAt = nil
                source.screenRect = nil
                self.lock.unlock()
                stream.updateContentFilter(filter) { _ in }
                stream.updateConfiguration(Self.configuration(for: filter)) { _ in }
            } else if stream == nil, self.isPicking || self.adding {
                self.startStream(filter)
            }
        }
    }

    func contentSharingPickerStartDidFailWithError(_ error: Error) {
        DispatchQueue.main.async {
            if self.adding {
                self.adding = false
                self.onOutcome?(.addCancelled)
                return
            }
            guard self.isPicking else { return }
            self.isPicking = false
            if self.sources.isEmpty { SCContentSharingPicker.shared.isActive = false }
            self.onOutcome?(.failed("picker: \(error.localizedDescription)"))
        }
    }

    // MARK: streams

    private static func configuration(for filter: SCContentFilter) -> SCStreamConfiguration {
        let config = SCStreamConfiguration()
        let scale = CGFloat(filter.pointPixelScale)
        config.width = max(2, Int(filter.contentRect.width * scale))
        config.height = max(2, Int(filter.contentRect.height * scale))
        // Frames only come when the content changes; 1 s keeps them fresh after an app switch.
        config.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        config.queueDepth = 3
        config.showsCursor = false
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.capturesAudio = false
        return config
    }

    private func startStream(_ filter: SCContentFilter) {
        let wasAdding = adding
        let stream = SCStream(filter: filter, configuration: Self.configuration(for: filter), delegate: self)
        do {
            try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: frames)
        } catch {
            return failStart("capture: \(error.localizedDescription)", adding: wasAdding)
        }
        let source = Source(id: nextID, stream: stream, filter: filter)
        nextID += 1
        lock.lock()
        sources.append(source)
        lock.unlock()
        stream.startCapture { error in
            DispatchQueue.main.async {
                guard self.sources.contains(where: { $0 === source }) else { return }
                if let error {
                    self.drop(source)
                    self.failStart("capture: \(error.localizedDescription)", adding: wasAdding)
                } else if wasAdding {
                    self.adding = false
                    self.onOutcome?(.added(self.sources.count))
                } else {
                    self.isPicking = false
                    self.onOutcome?(.picked)
                }
            }
        }
    }

    private func failStart(_ message: String, adding wasAdding: Bool) {
        if wasAdding {
            adding = false
            onOutcome?(.addCancelled)
            AppLog.shared.write("add a screen failed: \(message)")
        } else {
            stop()
            onOutcome?(.failed(message))
        }
    }

    private func drop(_ source: Source) {
        lock.lock()
        sources.removeAll { $0 === source }
        lock.unlock()
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        DispatchQueue.main.async {
            guard let source = self.sources.first(where: { $0.stream === stream }) else { return }
            self.drop(source)
            if !self.sources.isEmpty {
                self.onOutcome?(.removed(self.sources.count))
                return
            }
            let userStopped = (error as NSError).code == SCStreamError.userStopped.rawValue
            self.stop()
            self.onOutcome?(.stopped(userStopped ? nil : error.localizedDescription))
        }
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen, sampleBuffer.isValid,
              let attachments = CMSampleBufferGetSampleAttachmentsArray(sampleBuffer, createIfNecessary: false)
                as? [[SCStreamFrameInfo: Any]],
              let info = attachments.first,
              let raw = info[.status] as? Int, SCFrameStatus(rawValue: raw) == .complete,
              let buffer = sampleBuffer.imageBuffer else { return }
        var screenRect: CGRect?
        if let dict = info[.screenRect] as? NSDictionary { screenRect = CGRect(dictionaryRepresentation: dict) }
        lock.lock()
        if let source = sources.first(where: { $0.stream === stream }) {
            source.latest = buffer
            source.latestAt = Date()
            if let screenRect, !screenRect.isEmpty { source.screenRect = screenRect }
        }
        lock.unlock()
    }
}
