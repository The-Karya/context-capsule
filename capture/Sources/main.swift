// context-capsule — capture: entry point. Runs the menu-bar app, or `--selftest` (OCR + redaction
// on a generated image; needs no screen permission) and exits.

import AppKit

if CommandLine.arguments.contains("--selftest") {
    exit(SelfTest.run() ? 0 : 1)
}
if let i = CommandLine.arguments.firstIndex(of: "--render-icons"), i + 1 < CommandLine.arguments.count {
    // For checking the menu-bar look: the listening icon at three loudness levels, the recording dot, the idle panther.
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
    if let idle = MenuBarLook.idle() { // a template image: drawn white here, as on a dark menu bar
        save(NSImage(size: idle.size, flipped: false) { r in
            idle.draw(in: r)
            NSColor.white.set()
            r.fill(using: .sourceIn)
            return true
        }, "idle.png")
    }
    exit(0)
}

if let i = CommandLine.arguments.firstIndex(of: "--test-update"), i + 3 < CommandLine.arguments.count {
    // For checking a release by hand, offline: --test-update latest.json latest.json.sig Context-Capsule-<ver>-mac.zip
    // runs the same checks as a real update (signature, version, size, SHA-256, unzip, same signer); installs nothing.
    let a = CommandLine.arguments
    guard let feed = try? Data(contentsOf: URL(fileURLWithPath: a[i + 1])),
          let sig = try? Data(contentsOf: URL(fileURLWithPath: a[i + 2])) else { print("cannot read the feed"); exit(1) }
    switch Update.decide(feed: feed, signature: sig, current: Updater.currentVersion) {
    case .update(let version, let asset):
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("cc-test-update-\(getpid())")
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let zip = folder.appendingPathComponent("update.zip")
        try? FileManager.default.copyItem(at: URL(fileURLWithPath: a[i + 3]), to: zip)
        let result = Updater.stage(zip: zip, asset: asset, version: version, in: folder)
        try? FileManager.default.removeItem(at: folder)
        switch result {
        case .success: print("ok: \(version) would install"); exit(0)
        case .failure(let why): print("refused: \(why.reason)"); exit(1)
        }
    case let other: print("no update: \(other)"); exit(1)
    }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory) // menu bar only, no Dock icon (LSUIElement also says so)
app.run()
