// context-capsule — capture: what the menu-bar icon shows — the panther when idle, a red dot and a timer while recording, a red mic with
// bars that move with your voice while listening — and soft sounds at start/stop. Takes no screen space.

import AppKit

enum MenuBarLook {
    /// "12:34" (or "1:02:03").
    static func clock(_ seconds: TimeInterval) -> String {
        let s = max(0, Int(seconds))
        return s >= 3600 ? String(format: "%d:%02d:%02d", s / 3600, s / 60 % 60, s % 60) : String(format: "%d:%02d", s / 60, s % 60)
    }

    /// Idle: the Karya panther as a template image (macOS draws it black or white), 18 pt tall.
    /// A hollow circle if the image is missing from the app.
    static func idle() -> NSImage? {
        let image = Bundle.main.image(forResource: "MenuBarMark")
            ?? NSImage(systemSymbolName: "circle", accessibilityDescription: nil)
        image?.isTemplate = true
        // Fixed point size: the PNG's own resolution info would otherwise decide it (a 25 dpi file came out 3× too big).
        if let image, image.size.height > 0 {
            image.size = NSSize(width: (18 * image.size.width / image.size.height).rounded(), height: 18)
        }
        image?.accessibilityDescription = AppName.full
        return image
    }

    /// A small red dot (recording).
    static func dot() -> NSImage {
        NSImage(size: NSSize(width: 10, height: 10), flipped: false) { r in
            NSColor.systemRed.setFill()
            NSBezierPath(ovalIn: r.insetBy(dx: 0.5, dy: 0.5)).fill()
            return true
        }
    }

    /// A red mic followed by five bars whose height follows `level` (0…1).
    static func listening(level: Float) -> NSImage {
        let l = CGFloat(min(1, max(0, level * 2.5)))
        let weights: [CGFloat] = [0.45, 0.8, 1, 0.75, 0.5]
        let size = NSSize(width: 38, height: 16)
        return NSImage(size: size, flipped: false) { _ in
            NSColor.systemRed.set()
            if let mic = NSImage(systemSymbolName: "mic.fill", accessibilityDescription: nil)?
                .withSymbolConfiguration(NSImage.SymbolConfiguration(pointSize: 12, weight: .semibold)
                    .applying(NSImage.SymbolConfiguration(paletteColors: [.systemRed]))) {
                mic.draw(in: NSRect(x: 0, y: 1, width: 12, height: 14))
            }
            for (i, w) in weights.enumerated() {
                let h = 3 + 11 * l * w
                let bar = NSRect(x: 17 + CGFloat(i) * 4.5, y: (size.height - h) / 2, width: 2.5, height: h)
                NSBezierPath(roundedRect: bar, xRadius: 1.25, yRadius: 1.25).fill()
            }
            return true
        }
    }
}

/// Soft system sounds for start/stop of recording and of the mic (quiet; off with "sounds": false).
enum Cue {
    static func play(_ name: String) {
        guard Config.load(from: Paths.standard.config).sounds else { return }
        let sound = NSSound(named: NSSound.Name(name))
        sound?.volume = 0.35
        sound?.play()
    }
    static func recordingStarted() { play("Pop") }
    static func recordingStopped() { play("Bottle") }
    static func micOpened() { play("Tink") }
    static func micClosed() { play("Pop") }
}
