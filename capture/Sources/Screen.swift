// karya-capsule — capture: what is on screen now — front app, window title, screenshot, OCR.
// Screenshots live only in memory; they are never written to disk.

import AppKit
import ApplicationServices
import ScreenCaptureKit
import Vision

struct FrontWindow {
    var appName: String
    var bundle: String?
    var title: String
    var displayID: CGDirectDisplayID
}

enum Screen {
    /// Front app, its focused window title and the display it is on. Call on the main thread.
    static func frontWindow() -> FrontWindow? {
        guard let app = NSWorkspace.shared.frontmostApplication else { return nil }
        let pid = app.processIdentifier

        // Window list is ordered front to back, so the first normal window of the app is the front one.
        let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]] ?? []
        let window = list.first {
            ($0[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == pid
                && ($0[kCGWindowLayer as String] as? NSNumber)?.intValue == 0
        }

        var title = window?[kCGWindowName as String] as? String ?? ""
        if AXIsProcessTrusted(), let t = accessibilityTitle(pid: pid), !t.isEmpty { title = t }

        var display = CGMainDisplayID()
        if let bounds = window?[kCGWindowBounds as String] as? NSDictionary,
           let rect = CGRect(dictionaryRepresentation: bounds) {
            var id: CGDirectDisplayID = 0
            var count: UInt32 = 0
            if CGGetDisplaysWithPoint(CGPoint(x: rect.midX, y: rect.midY), 1, &id, &count) == .success, count > 0 {
                display = id
            }
        }
        return FrontWindow(appName: app.localizedName ?? "", bundle: app.bundleIdentifier, title: title, displayID: display)
    }

    private static func accessibilityTitle(pid: pid_t) -> String? {
        let appElement = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(appElement, 0.5) // a frozen app must not stall sampling
        var window: CFTypeRef?
        guard AXUIElementCopyAttributeValue(appElement, kAXFocusedWindowAttribute as CFString, &window) == .success,
              let window, CFGetTypeID(window) == AXUIElementGetTypeID() else { return nil }
        var title: CFTypeRef?
        guard AXUIElementCopyAttributeValue(window as! AXUIElement, kAXTitleAttribute as CFString, &title) == .success
        else { return nil }
        return title as? String
    }

    /// Full-resolution image of one display. Blocks; call off the main thread.
    static func screenshot(display: CGDirectDisplayID) -> CGImage? {
        if #available(macOS 14.0, *) { return screenCaptureKitShot(display) }
        return legacyShot(display)
    }

    private final class Box<T> { var value: T? }

    @available(macOS 14.0, *)
    private static func screenCaptureKitShot(_ displayID: CGDirectDisplayID) -> CGImage? {
        let done = DispatchSemaphore(value: 0)
        let result = Box<CGImage>()
        SCShareableContent.getExcludingDesktopWindows(false, onScreenWindowsOnly: true) { content, _ in
            guard let displays = content?.displays,
                  let display = displays.first(where: { $0.displayID == displayID }) ?? displays.first else {
                done.signal(); return
            }
            let filter = SCContentFilter(display: display, excludingWindows: [])
            let config = SCStreamConfiguration()
            let scale = CGFloat(filter.pointPixelScale)
            config.width = Int(CGFloat(display.width) * scale)
            config.height = Int(CGFloat(display.height) * scale)
            config.showsCursor = false
            SCScreenshotManager.captureImage(contentFilter: filter, configuration: config) { image, _ in
                result.value = image
                done.signal()
            }
        }
        _ = done.wait(timeout: .now() + 10)
        return result.value
    }

    /// macOS 13: CGDisplayCreateImage. Looked up at run time because newer SDKs mark it unavailable.
    private static func legacyShot(_ displayID: CGDirectDisplayID) -> CGImage? {
        typealias CreateImage = @convention(c) (CGDirectDisplayID) -> Unmanaged<CGImage>?
        guard let handle = dlopen(nil, RTLD_NOW), let sym = dlsym(handle, "CGDisplayCreateImage") else { return nil }
        return unsafeBitCast(sym, to: CreateImage.self)(displayID)?.takeRetainedValue()
    }

    /// Apple Vision OCR, accurate, with language correction. Lines top to bottom.
    static func recognizeText(in image: CGImage) -> String {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true
        do { try VNImageRequestHandler(cgImage: image, options: [:]).perform([request]) } catch { return "" }
        return (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
    }
}
