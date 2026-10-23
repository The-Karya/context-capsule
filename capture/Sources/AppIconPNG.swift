// context-capsule — capture: app icons for the shared window (bridge `apps.icons`): the app by its bundle id, else
// by its name (a running app, then the usual Applications folders), drawn as a 64 px PNG (32 pt @2x). Each app is
// looked up once; answers (found or not) are kept for the program's life.

import AppKit

enum AppIconPNG {
    /// Pixels per side: icons are drawn at 32 × 32 points, @2x.
    static let size = 64
    /// At most this many apps in one call.
    static let maxApps = 200

    private static var cache: [String: String?] = [:]
    private static let lock = NSLock()

    /// The answer's key: the bundle id, else the app's name.
    static func key(app: String, bundle: String) -> String {
        let b = bundle.trimmingCharacters(in: .whitespaces)
        return b.isEmpty ? app.trimmingCharacters(in: .whitespaces) : b
    }

    /// `apps.icons`: `{ key: "data:image/png;base64,…" | null }`. Blocks while drawing: call off the main thread.
    static func answer(_ apps: [[String: Any]]) -> [String: Any] {
        var out: [String: Any] = [:]
        for a in apps.prefix(maxApps) {
            let app = (a["app"] as? String ?? "").trimmingCharacters(in: .whitespaces)
            let bundle = (a["bundle"] as? String ?? "").trimmingCharacters(in: .whitespaces)
            let k = key(app: app, bundle: bundle)
            if k.isEmpty || k.count > 512 || out[k] != nil { continue }
            lock.lock()
            let hit = cache[k]
            lock.unlock()
            let url: String?
            if let hit { url = hit } else {
                url = png(app: app, bundle: bundle).map { "data:image/png;base64," + $0.base64EncodedString() }
                lock.lock()
                cache[k] = .some(url)
                lock.unlock()
            }
            out[k] = url ?? NSNull()
        }
        return out
    }

    /// Where the app is: by bundle id, else a running app with that name, else `<name>.app` in the usual folders.
    static func appURL(app: String, bundle: String) -> URL? {
        let ws = NSWorkspace.shared
        if !bundle.isEmpty, let u = ws.urlForApplication(withBundleIdentifier: bundle) { return u }
        guard !app.isEmpty, !app.contains("/") else { return nil }
        if let running = ws.runningApplications.first(where: { $0.localizedName == app && $0.bundleURL != nil }) {
            return running.bundleURL
        }
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        for dir in ["/Applications", "/System/Applications", "/System/Applications/Utilities", "/Applications/Utilities",
                    home + "/Applications"] {
            let p = "\(dir)/\(app).app"
            if FileManager.default.fileExists(atPath: p) { return URL(fileURLWithPath: p) }
        }
        return nil
    }

    /// The app's icon as a PNG of `size` × `size` pixels; nil when the app isn't found.
    static func png(app: String, bundle: String) -> Data? {
        guard let url = appURL(app: app, bundle: bundle) else { return nil }
        return png(NSWorkspace.shared.icon(forFile: url.path))
    }

    /// Any image drawn into `size` × `size` pixels with alpha, as a PNG.
    static func png(_ image: NSImage) -> Data? {
        guard let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
                                         samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                         bytesPerRow: 0, bitsPerPixel: 0),
              let context = NSGraphicsContext(bitmapImageRep: rep) else { return nil }
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = context
        context.imageInterpolation = .high
        image.draw(in: NSRect(x: 0, y: 0, width: size, height: size), from: .zero, operation: .copy, fraction: 1)
        context.flushGraphics()
        NSGraphicsContext.restoreGraphicsState()
        rep.size = NSSize(width: size / 2, height: size / 2) // 32 pt @2x
        return rep.representation(using: .png, properties: [:])
    }
}
