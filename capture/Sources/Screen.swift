// context-capsule — capture: what is on screen now — front window, Accessibility text, Vision OCR.
// No screen capture here (that only happens through the system picker, see Picker.swift).
// Frames live only in memory; they are never written to disk.

import AppKit
import ApplicationServices
import Vision

struct FrontWindow {
    var appName: String
    var bundle: String?
    var pid: pid_t
    /// The app's frontmost normal window, if it has one on screen.
    var window: WindowInfo?
}

enum Screen {
    /// Front app and its frontmost window. CGWindowList gives ids, owners and frames without any
    /// permission (window names would need Screen Recording, so we never read them here).
    static func frontWindow() -> FrontWindow? {
        guard let app = NSWorkspace.shared.frontmostApplication else { return nil }
        let pid = app.processIdentifier

        // Ordered front to back, so the first normal window of the app is its front one.
        let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]] ?? []
        let entry = list.first {
            ($0[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == pid
                && ($0[kCGWindowLayer as String] as? NSNumber)?.intValue == 0
        }
        var window: WindowInfo?
        if let entry, let id = (entry[kCGWindowNumber as String] as? NSNumber)?.uint32Value,
           let bounds = entry[kCGWindowBounds as String] as? NSDictionary,
           let frame = CGRect(dictionaryRepresentation: bounds) {
            window = WindowInfo(id: id, pid: pid, frame: frame)
        }
        return FrontWindow(appName: app.localizedName ?? "", bundle: app.bundleIdentifier, pid: pid, window: window)
    }

    /// Apple Vision OCR, accurate, with language correction. Lines top to bottom.
    /// `region` is normalized with the origin at the bottom left (Vision's regionOfInterest).
    static func recognizeText(in image: CGImage) -> String {
        perform(VNImageRequestHandler(cgImage: image, options: [:]), region: nil)
    }

    static func recognizeText(in buffer: CVPixelBuffer, region: CGRect?) -> String {
        perform(VNImageRequestHandler(cvPixelBuffer: buffer, options: [:]), region: region)
    }

    private static func perform(_ handler: VNImageRequestHandler, region: CGRect?) -> String {
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true
        if let region { request.regionOfInterest = region }
        do { try handler.perform([request]) } catch { return "" }
        return (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
    }
}

/// Reads the focused window's text through the Accessibility API. Optional: without the permission
/// every call returns nothing and the recorder falls back to OCR.
enum AXReader {
    static var trusted: Bool { AXIsProcessTrusted() }

    /// Shows the system "Context Capsule would like to control this computer using accessibility features" prompt.
    @discardableResult
    static func requestTrust() -> Bool {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        return AXIsProcessTrustedWithOptions([key: true] as CFDictionary)
    }

    private static let setup: Void = {
        // A frozen app must not stall sampling: every AX call gives up after 0.25 s.
        AXUIElementSetMessagingTimeout(AXUIElementCreateSystemWide(), 0.25)
    }()

    private static let lock = NSLock()
    private static var askedElectron = Set<pid_t>()

    static func focusedWindow(pid: pid_t) -> AXUIElement? {
        _ = setup
        let app = AXUIElementCreateApplication(pid)
        lock.lock()
        let first = askedElectron.insert(pid).inserted
        lock.unlock()
        if first {
            // Electron apps (Slack, VS Code, Notion…) only build their AX tree when asked to.
            AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue)
        }
        var window: CFTypeRef?
        guard AXUIElementCopyAttributeValue(app, kAXFocusedWindowAttribute as CFString, &window) == .success,
              let window, CFGetTypeID(window) == AXUIElementGetTypeID() else { return nil }
        return (window as! AXUIElement) // swiftlint:disable:this force_cast
    }

    static func title(pid: pid_t) -> String? {
        guard trusted, let window = focusedWindow(pid: pid) else { return nil }
        return string(window, kAXTitleAttribute)
    }

    /// The focused window's text, assembled by `AXText` (secure fields skipped). "" if not trusted.
    static func text(pid: pid_t) -> String {
        guard trusted, let window = focusedWindow(pid: pid) else { return "" }
        return AXText.assemble(walk(window))
    }

    /// Depth-first, parents before children, within `AXText` node / time / depth limits.
    static func walk(_ root: AXUIElement) -> [AXNode] {
        let deadline = Date().addingTimeInterval(AXText.budget)
        var nodes: [AXNode] = []
        var stack: [(AXUIElement, Int)] = [(root, 0)]
        let shapeKeys = [kAXRoleAttribute, kAXSubroleAttribute, kAXChildrenAttribute] as CFArray

        while let (element, depth) = stack.popLast(), nodes.count < AXText.maxNodes, Date() < deadline {
            var raw: CFArray?
            guard AXUIElementCopyMultipleAttributeValues(element, shapeKeys, [], &raw) == .success,
                  let values = raw as? [Any], values.count == 3 else { continue }
            var node = AXNode(depth: depth, role: values[0] as? String ?? "", subrole: values[1] as? String ?? "")

            if AXText.isSecure(role: node.role, subrole: node.subrole) {
                nodes.append(node) // recorded so assembly knows; never read, never descended
                continue
            }
            if AXText.valueRoles.contains(node.role) || AXText.labelRoles.contains(node.role) {
                node.value = string(element, kAXValueAttribute) ?? ""
                node.title = string(element, kAXTitleAttribute) ?? ""
                node.desc = string(element, kAXDescriptionAttribute) ?? ""
            }
            nodes.append(node)

            if depth < AXText.maxDepth, let children = values[2] as? [AXUIElement] {
                for child in children.reversed() { stack.append((child, depth + 1)) }
            }
        }
        return nodes
    }

    private static func string(_ element: AXUIElement, _ attribute: String) -> String? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute as CFString, &value) == .success else { return nil }
        return value as? String
    }
}
