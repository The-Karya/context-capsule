// context-capsule — ui: snap — renders a URL in an offscreen WKWebView and writes a 2× PNG (for comparing with Mac renders).
// Build: swiftc -O ui/dev/snap.swift -o /tmp/shared-ui-tools/snap
// Usage: snap <url> <out.png> <width> <height> <light|dark>
// Waits until the page sets document.documentElement.dataset.ready = "1" (10 s at most, then snaps anyway and warns).
import AppKit
import WebKit

let args = CommandLine.arguments
guard args.count == 6, let url = URL(string: args[1]), let w = Double(args[3]), let h = Double(args[4]),
      ["light", "dark"].contains(args[5]) else {
    FileHandle.standardError.write("usage: snap <url> <out.png> <width> <height> <light|dark>\n".data(using: .utf8)!)
    exit(2)
}
let out = URL(fileURLWithPath: args[2])
let dark = args[5] == "dark"
let scale = 2.0

let app = NSApplication.shared
app.setActivationPolicy(.prohibited)
app.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)

let size = NSSize(width: w, height: h)
// Offscreen but real window, so WebKit lays out and paints at the given size.
let window = NSWindow(contentRect: NSRect(x: -20000, y: -20000, width: w, height: h),
                      styleMask: [.borderless], backing: .buffered, defer: false)
window.appearance = app.appearance
window.colorSpace = .sRGB // the page drawn in sRGB, like the Mac references
let web = WKWebView(frame: NSRect(origin: .zero, size: size), configuration: WKWebViewConfiguration())
web.appearance = app.appearance
window.contentView = web
window.orderBack(nil)
web.load(URLRequest(url: url))

func fail(_ s: String) -> Never { FileHandle.standardError.write("snap: \(s)\n".data(using: .utf8)!); exit(1) }

func snapshot() {
    let config = WKSnapshotConfiguration()
    config.rect = NSRect(origin: .zero, size: size)
    // snapshotWidth is in points at the window's backing scale; ask for enough to give 2× pixels, whatever the screen.
    config.snapshotWidth = NSNumber(value: w * scale / window.backingScaleFactor)
    config.afterScreenUpdates = true
    web.takeSnapshot(with: config) { image, error in
        guard let image else { fail("snapshot failed: \(error?.localizedDescription ?? "?")") }
        let pw = Int(w * scale), ph = Int(h * scale)
        // The snapshot's pixels are the page's sRGB colours; keep them as they are (no colour conversion), tagged sRGB.
        var rect = NSRect(origin: .zero, size: size)
        let direct = (image.representations.first as? NSBitmapImageRep)?.cgImage
        guard let cg = direct ?? image.cgImage(forProposedRect: &rect, context: nil, hints: nil),
              let space = CGColorSpace(name: CGColorSpace.sRGB),
              let ctx = CGContext(data: nil, width: pw, height: ph, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { fail("bitmap") }

        ctx.interpolationQuality = .high
        if let tagged = cg.copy(colorSpace: space) { ctx.draw(tagged, in: CGRect(x: 0, y: 0, width: pw, height: ph)) }
        guard let out2 = ctx.makeImage() else { fail("bitmap") }
        let rep = NSBitmapImageRep(cgImage: out2)
        guard let png = rep.representation(using: .png, properties: [:]) else { fail("png") }
        do { try png.write(to: out) } catch { fail("can't write \(out.path): \(error.localizedDescription)") }
        exit(0)
    }
}

let start = Date()
func poll() {
    web.evaluateJavaScript("document.documentElement.dataset.ready === '1'") { value, _ in
        if value as? Bool == true {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { snapshot() } // let the last frame paint
        } else if Date().timeIntervalSince(start) > 10 {
            FileHandle.standardError.write("snap: warning: \(url) never set data-ready=1 in 10 s; snapping anyway\n".data(using: .utf8)!)
            snapshot()
        } else {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { poll() }
        }
    }
}
DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { poll() }
app.run()
