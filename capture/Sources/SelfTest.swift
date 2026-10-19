// context-capsule — capture: `ContextCapsule --selftest` — draws text into an image, runs Vision OCR on it (whole,
// and cropped like a picked display), redacts and caps the result. Proves OCR works without the screen.

import AppKit

enum SelfTest {
    static func run() -> Bool {
        let lines = ["Context Capsule self test", "Pricing page redesign 2026", "password: hunter2"]
        guard let image = render(lines) else { print("selftest: could not draw image"); return false }

        let start = Date()
        let raw = Screen.recognizeText(in: image)
        let ms = Int(Date().timeIntervalSince(start) * 1000)
        let clean = Redactor.clean(raw)
        print("selftest: OCR took \(ms) ms\n--- raw ---\n\(raw)\n--- written ---\n\(clean)\n---")

        var ok = true
        func expect(_ cond: Bool, _ what: String) {
            print("selftest: \(cond ? "ok  " : "FAIL") \(what)")
            ok = ok && cond
        }
        let lower = raw.lowercased()
        expect(lower.contains("capsule self test"), "reads line 1")
        expect(lower.contains("pricing page redesign"), "reads line 2")
        expect(!clean.contains("hunter2") && clean.contains(Redactor.mark), "password line redacted")

        // The same image as a stream frame, cropped to a "window" covering only the top line.
        if let buffer = pixelBuffer(image) {
            let h = CGFloat(image.height), w = CGFloat(image.width)
            let display = CGRect(x: 0, y: 0, width: w, height: h)
            let window = CGRect(x: 0, y: 0, width: w, height: h / 3) // covers line 1 only
            let region = Membership.ocrRegion(PickedContent(style: .display, rect: display), window: window)
            let top = region.map { Screen.recognizeText(in: buffer, region: $0) }?.lowercased() ?? ""
            print("--- cropped ---\n\(top)\n---")
            expect(top.contains("capsule self test") && !top.contains("pricing"), "crop reads only the window")
        } else {
            expect(false, "pixel buffer")
        }
        print(ok ? "selftest: PASS" : "selftest: FAIL")
        return ok
    }

    private static func pixelBuffer(_ image: CGImage) -> CVPixelBuffer? {
        var buffer: CVPixelBuffer?
        let attrs = [kCVPixelBufferCGImageCompatibilityKey: true, kCVPixelBufferCGBitmapContextCompatibilityKey: true]
        guard CVPixelBufferCreate(nil, image.width, image.height, kCVPixelFormatType_32BGRA,
                                  attrs as CFDictionary, &buffer) == kCVReturnSuccess, let buffer else { return nil }
        CVPixelBufferLockBaseAddress(buffer, [])
        defer { CVPixelBufferUnlockBaseAddress(buffer, []) }
        guard let ctx = CGContext(data: CVPixelBufferGetBaseAddress(buffer), width: image.width, height: image.height,
                                  bitsPerComponent: 8, bytesPerRow: CVPixelBufferGetBytesPerRow(buffer),
                                  space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue
                                      | CGBitmapInfo.byteOrder32Little.rawValue) else { return nil }
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        return buffer
    }

    /// Black text on white, like a plain document window.
    private static func render(_ lines: [String]) -> CGImage? {
        let size = NSSize(width: 1400, height: 120 + 90 * lines.count)
        let image = NSImage(size: size, flipped: true) { rect in
            NSColor.white.setFill()
            rect.fill()
            let attrs: [NSAttributedString.Key: Any] = [.font: NSFont.systemFont(ofSize: 56), .foregroundColor: NSColor.black]
            for (i, line) in lines.enumerated() {
                (line as NSString).draw(at: NSPoint(x: 60, y: 60 + 90 * CGFloat(i)), withAttributes: attrs)
            }
            return true
        }
        var rect = NSRect(origin: .zero, size: size)
        return image.cgImage(forProposedRect: &rect, context: nil, hints: nil)
    }
}
