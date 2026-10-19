// karya-capsule — capture: `Capsule --selftest` — draws text into an image, runs Vision OCR on it,
// redacts and caps the result. Proves OCR works on this Mac without touching the screen.

import AppKit

enum SelfTest {
    static func run() -> Bool {
        let lines = ["Karya Capsule self test", "Pricing page redesign 2026", "password: hunter2"]
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
        print(ok ? "selftest: PASS" : "selftest: FAIL")
        return ok
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
