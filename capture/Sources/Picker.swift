// context-capsule — capture: the macOS system picker (SCContentSharingPicker). The user chooses a window
// or a display; we capture only that, through one SCStream at most one frame per second. Because the user
// picked the content, macOS asks for no Screen Recording permission. Frames stay in memory.

import AppKit
import CoreMedia
import ScreenCaptureKit

final class PickerCapture: NSObject, SCContentSharingPickerObserver, SCStreamDelegate, SCStreamOutput {
    enum Outcome {
        case picked                 // the user chose content and the stream is running
        case cancelled              // the user closed the picker
        case failed(String)         // the picker or the stream could not start
        case stopped(String?)       // the stream ended on its own (nil: the user stopped sharing in macOS)
    }

    /// Called on the main thread.
    var onOutcome: ((Outcome) -> Void)?

    private(set) var isPicking = false
    var isCapturing: Bool { stream != nil }

    private var stream: SCStream?
    private var filter: SCContentFilter?
    private var observing = false
    private let frames = DispatchQueue(label: "com.thekarya.contextcapsule.frames")
    private let lock = NSLock()
    private var latest: CVPixelBuffer?
    private var latestAt: Date?
    private var latestScreenRect: CGRect?

    // MARK: main thread

    /// Show the system picker (or bring it back to the front if it is already open).
    func present() {
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
        picker.maximumStreamCount = 1
        picker.isActive = true
        isPicking = true
        NSApp.activate(ignoringOtherApps: true)
        picker.present()
    }

    /// Stop capturing and take the picker down. Safe to call in any state.
    func stop() {
        isPicking = false
        if let stream { stream.stopCapture { _ in } }
        stream = nil
        filter = nil
        lock.lock()
        latest = nil
        latestAt = nil
        latestScreenRect = nil
        lock.unlock()
        SCContentSharingPicker.shared.isActive = false
    }

    // MARK: any thread

    /// What the user picked, placed on screen as of the latest frame.
    func pickedContent() -> PickedContent? {
        guard let filter = currentFilter() else { return nil }
        lock.lock()
        let screenRect = latestScreenRect
        lock.unlock()
        return Self.describe(filter, screenRect: screenRect, displays: Screen.displays())
    }

    /// The latest frame and when it arrived. Frames only arrive when the picked content changes, so
    /// right after an app switch it may still show the previous app (see `Freshness`).
    func latestFrame() -> (buffer: CVPixelBuffer, at: Date)? {
        lock.lock()
        defer { lock.unlock() }
        guard let latest, let latestAt else { return nil }
        return (latest, latestAt)
    }

    /// Title of a picked window, when macOS tells us which windows were picked (15.2+).
    func pickedWindowTitle(id: UInt32) -> String? {
        guard #available(macOS 15.2, *), let filter = currentFilter() else { return nil }
        return filter.includedWindows.first { $0.windowID == id }?.title
    }

    private func currentFilter() -> SCContentFilter? {
        lock.lock()
        defer { lock.unlock() }
        return filter
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
            guard stream == nil, self.isPicking else { return }
            self.isPicking = false
            SCContentSharingPicker.shared.isActive = false
            self.onOutcome?(.cancelled)
        }
    }

    func contentSharingPicker(_ picker: SCContentSharingPicker, didUpdateWith filter: SCContentFilter,
                              for stream: SCStream?) {
        DispatchQueue.main.async {
            if let stream, stream === self.stream {
                // The user changed the selection from the menu-bar sharing icon.
                self.setFilter(filter)
                stream.updateContentFilter(filter) { _ in }
                stream.updateConfiguration(Self.configuration(for: filter)) { _ in }
            } else if self.isPicking, self.stream == nil {
                self.startStream(filter)
            }
        }
    }

    func contentSharingPickerStartDidFailWithError(_ error: Error) {
        DispatchQueue.main.async {
            guard self.isPicking else { return }
            self.isPicking = false
            SCContentSharingPicker.shared.isActive = false
            self.onOutcome?(.failed("picker: \(error.localizedDescription)"))
        }
    }

    // MARK: stream

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

    private func setFilter(_ filter: SCContentFilter?) {
        lock.lock()
        self.filter = filter
        latest = nil
        latestAt = nil
        latestScreenRect = nil
        lock.unlock()
    }

    private func startStream(_ filter: SCContentFilter) {
        let stream = SCStream(filter: filter, configuration: Self.configuration(for: filter), delegate: self)
        do {
            try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: frames)
        } catch {
            fail("capture: \(error.localizedDescription)")
            return
        }
        setFilter(filter)
        self.stream = stream
        stream.startCapture { error in
            DispatchQueue.main.async {
                guard self.stream === stream else { return }
                if let error {
                    self.fail("capture: \(error.localizedDescription)")
                } else {
                    self.isPicking = false
                    self.onOutcome?(.picked)
                }
            }
        }
    }

    private func fail(_ message: String) {
        stop()
        onOutcome?(.failed(message))
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        DispatchQueue.main.async {
            guard self.stream === stream else { return }
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
        latest = buffer
        latestAt = Date()
        if let screenRect, !screenRect.isEmpty { latestScreenRect = screenRect }
        lock.unlock()
    }
}
