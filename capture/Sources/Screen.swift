// context-capsule — capture: what is on screen now — front window, Accessibility text, Vision OCR.
// No screen capture here (that only happens through the system picker, see Picker.swift).
// Frames live only in memory; they are never written to disk.

import AppKit
import ApplicationServices
import CoreImage
import Vision

struct FrontWindow {
    var appName: String
    var bundle: String?
    var pid: pid_t
    /// The app's front window, if it has a normal one on screen (see `WindowList.front`).
    var window: ListedWindow?
    /// Every on-screen window, front to back, with never-record apps marked.
    var list: [ListedWindow]
}

enum Screen {
    /// Front app, its front window and the on-screen window list. CGWindowList gives ids, owners,
    /// layers and frames without any permission. Window names come only if macOS gives them out
    /// (it never prompts for them).
    static func frontWindow(isExcluded: (_ bundle: String?, _ name: String?) -> Bool) -> FrontWindow? {
        guard let app = NSWorkspace.shared.frontmostApplication else { return nil }
        let list = windowList(isExcluded: isExcluded)
        return FrontWindow(appName: app.localizedName ?? "", bundle: app.bundleIdentifier, pid: app.processIdentifier,
                           window: WindowList.front(pid: app.processIdentifier, in: list), list: list)
    }

    static func windowList(isExcluded: (_ bundle: String?, _ name: String?) -> Bool) -> [ListedWindow] {
        let raw = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]] ?? []
        var bundles: [pid_t: String?] = [:]
        return raw.compactMap { e in
            guard let id = (e[kCGWindowNumber as String] as? NSNumber)?.uint32Value,
                  let pid = (e[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value,
                  let bounds = e[kCGWindowBounds as String] as? NSDictionary,
                  let frame = CGRect(dictionaryRepresentation: bounds) else { return nil }
            let owner = e[kCGWindowOwnerName as String] as? String ?? ""
            if bundles[pid] == nil { bundles[pid] = NSRunningApplication(processIdentifier: pid)?.bundleIdentifier }
            let bundle = bundles[pid] ?? nil
            return ListedWindow(id: id, pid: pid, owner: owner,
                                layer: (e[kCGWindowLayer as String] as? NSNumber)?.intValue ?? 0,
                                alpha: (e[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 1,
                                frame: frame, name: e[kCGWindowName as String] as? String,
                                excluded: isExcluded(bundle, owner))
        }
    }

    /// Global frames (points, top-left origin) of the active displays.
    static func displays() -> [CGRect] {
        var count: UInt32 = 0
        guard CGGetActiveDisplayList(0, nil, &count) == .success, count > 0 else { return [] }
        var ids = [CGDirectDisplayID](repeating: 0, count: Int(count))
        guard CGGetActiveDisplayList(count, &ids, &count) == .success else { return [] }
        return ids.prefix(Int(count)).map { CGDisplayBounds($0) }
    }

    /// OCR `region` of a frame after painting `masks` black (both normalized, origin bottom left).
    /// The copy lives only for this call.
    static func recognizeText(in buffer: CVPixelBuffer, region: CGRect, masks: [CGRect]) -> String {
        if masks.isEmpty { return recognizeText(in: buffer, region: region) }
        let image = CIContext(options: [.useSoftwareRenderer: false])
            .createCGImage(CIImage(cvPixelBuffer: buffer), from: CGRect(x: 0, y: 0, width: CVPixelBufferGetWidth(buffer),
                                                                      height: CVPixelBufferGetHeight(buffer)))
        guard let image, let masked = paint(image, masks) else { return "" } // cannot mask: read nothing
        return perform(VNImageRequestHandler(cgImage: masked, options: [:]), region: region)
    }

    static func paint(_ image: CGImage, _ masks: [CGRect]) -> CGImage? {
        let w = CGFloat(image.width), h = CGFloat(image.height)
        guard let ctx = CGContext(data: nil, width: image.width, height: image.height, bitsPerComponent: 8,
                                  bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue
                                      | CGBitmapInfo.byteOrder32Little.rawValue) else { return nil }
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
        ctx.setFillColor(CGColor(gray: 0, alpha: 1))
        // CGContext's origin is bottom left, like Vision's normalized rects. Grow a little for rounding.
        for m in masks { ctx.fill(CGRect(x: m.minX * w, y: m.minY * h, width: m.width * w, height: m.height * h)
            .insetBy(dx: -2, dy: -2)) }
        return ctx.makeImage()
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

    /// The window's page address or file path: its AXDocument, else the first web area's AXURL
    /// (browsers). nil when not trusted or not known.
    static func location(pid: pid_t) -> String? {
        guard trusted, let window = focusedWindow(pid: pid) else { return nil }
        if let doc = string(window, kAXDocumentAttribute), !doc.isEmpty { return doc }
        // A short breadth-first look for the web area; pages sit a few levels down.
        var queue: [(AXUIElement, Int)] = [(window, 0)]
        var seen = 0
        while !queue.isEmpty, seen < 300 {
            let (element, depth) = queue.removeFirst()
            seen += 1
            if string(element, kAXRoleAttribute) == "AXWebArea" {
                var value: CFTypeRef?
                if AXUIElementCopyAttributeValue(element, kAXURLAttribute as CFString, &value) == .success, let value {
                    if let url = value as? URL { return url.absoluteString }
                    if let text = value as? String { return text }
                }
            }
            guard depth < 10 else { continue }
            var children: CFTypeRef?
            if AXUIElementCopyAttributeValue(element, kAXChildrenAttribute as CFString, &children) == .success,
               let list = children as? [AXUIElement] {
                queue += list.map { ($0, depth + 1) }
            }
        }
        return nil
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
