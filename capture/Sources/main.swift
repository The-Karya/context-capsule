// context-capsule — capture: entry point. Runs the menu-bar app, or `--selftest` (OCR + redaction
// on a generated image; needs no screen permission) and exits.

import AppKit

if CommandLine.arguments.contains("--selftest") {
    exit(SelfTest.run() ? 0 : 1)
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory) // menu bar only, no Dock icon (LSUIElement also says so)
app.run()
