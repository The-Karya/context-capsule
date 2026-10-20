// context-capsule — capture: entry point. Runs the menu-bar app, or `--selftest` (OCR + redaction
// on a generated image; needs no screen permission) and exits.

import AppKit

if CommandLine.arguments.contains("--selftest") {
    exit(SelfTest.run() ? 0 : 1)
}
if let i = CommandLine.arguments.firstIndex(of: "--render-icons"), i + 1 < CommandLine.arguments.count {
    // For checking the menu-bar look: the listening icon at three loudness levels, and the recording dot.
    let dir = URL(fileURLWithPath: CommandLine.arguments[i + 1])
    func save(_ image: NSImage, _ name: String) {
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(image.size.width * 4), pixelsHigh: Int(image.size.height * 4),
                                   bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                                   colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        rep.size = image.size
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        NSColor(white: 0.15, alpha: 1).setFill()
        NSRect(origin: .zero, size: image.size).fill()
        image.draw(in: NSRect(origin: .zero, size: image.size))
        NSGraphicsContext.restoreGraphicsState()
        try? rep.representation(using: .png, properties: [:])?.write(to: dir.appendingPathComponent(name))
    }
    for (n, l) in [("quiet", Float(0.02)), ("talking", 0.2), ("loud", 0.45)] { save(MenuBarLook.listening(level: l), "listening-\(n).png") }
    save(MenuBarLook.dot(), "recording-dot.png")
    exit(0)
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory) // menu bar only, no Dock icon (LSUIElement also says so)
app.run()
