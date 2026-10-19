# Context Capsule — roadmap

## v1 (current): macOS, Windows, Linux

### macOS

- Recorder: native Swift menu-bar agent (`capture/`), macOS 14+.
  - Capture scope: `SCContentSharingPicker` (ScreenCaptureKit), user-selected display/window per session; no Screen Recording TCC entitlement requested.
  - Text extraction: Accessibility (AX) tree of the focused window first; Vision OCR (`VNRecognizeTextRequest`) on the picked content as fallback, cropped to the frontmost window.
  - Voice notes: push-to-talk, `SFSpeechRecognizer` with `requiresOnDeviceRecognition`; no audio persisted.
- MCP server (`mcp/`): Node 18+, stdio transport, platform-independent.
- Contract: session files under `~/.context-capsule/` as defined in [SPEC.md](SPEC.md). Any recorder that writes this layout works with the MCP server unchanged.

### Windows and Linux (moved into v1)

The MCP server and the capsule format are cross-platform. The Rust recorders emit the
same `session.json` / `events.jsonl` / `state.json` / `control` contract.

Status (19 Oct 2026):

| | Built | Unit-tested | Tried on a real machine |
|---|---|---|---|
| macOS (`capture/`) | yes | yes | yes |
| Windows (`capture-rs/windows/`) | yes (cross-built from macOS; CI on `windows-latest`) | yes | in progress (`capture-rs/windows/TESTING.md`) |
| Linux (`capture-rs/linux/`) | yes (Docker; CI on `ubuntu-latest`) | yes; X11 path smoke-tested under Xvfb | not yet |

### Approach

Rust recorders for Windows and Linux (`capture-rs/windows`, `capture-rs/linux`), built
directly on each OS's own capture, accessibility and OCR APIs (see the matrix). The
macOS recorder stays native Swift (ScreenCaptureKit picker and AX are
first-class there). Shared, platform-neutral logic (redaction, never-record matching,
sample de-duplication, event serialisation, control-file protocol) moves into a small
Rust crate with golden tests generated from the Swift test vectors, so all recorders
produce byte-identical events for identical input.

### Platform matrix

| Concern | macOS | Windows | Linux |
|---|---|---|---|
| Capture scope (user consent) | `SCContentSharingPicker` | `Windows.Graphics.Capture` `GraphicsCapturePicker` | `xdg-desktop-portal` ScreenCast (PipeWire); X11 fallback with explicit window selection |
| Structured text | AX tree | UI Automation (`IUIAutomation`, TextPattern) | AT-SPI2 (D-Bus) |
| OCR fallback | Vision | `Windows.Media.Ocr` (on-device) | Tesseract CLI (system tessdata) |
| Voice notes (on-device) | `SFSpeechRecognizer` | whisper.cpp (multilingual base, CPU) | whisper.cpp (multilingual base, CPU) |
| Global hotkey | Carbon `RegisterEventHotKey` | `RegisterHotKey` | portal GlobalShortcuts (Wayland) / XGrabKey (X11) |
| Tray / menu bar | `NSStatusItem` | `Shell_NotifyIcon` | StatusNotifierItem (AppIndicator) |
| Front window + title | NSWorkspace + AX | `GetForegroundWindow` + UIA | AT-SPI active frame; `_NET_ACTIVE_WINDOW` on X11 |
| Data dir | `~/.context-capsule` | `%USERPROFILE%\.context-capsule` | `$XDG_DATA_HOME/context-capsule` (fallback `~/.context-capsule`) |
| Packaging | ad-hoc → Developer ID + notarization | MSIX / signed NSIS | AppImage + .deb |

### Milestones (1–5 done; real-machine testing in progress)

1. **Shared core crate**: port Core.swift logic to Rust; golden-vector parity tests against the Swift suite.
2. **Windows recorder**: picker-scoped capture, UIA text, Windows OCR fallback, hotkeys, tray, control/state protocol; CI on `windows-latest`.
3. **Linux recorder**: portal ScreenCast + AT-SPI2 + Tesseract; Wayland-first, X11 fallback; CI on `ubuntu-latest` (Xvfb for X11 paths).
4. **Voice on Windows/Linux**: whisper.cpp integration with model download on first use, VAD-gated, no audio persisted.
5. **MCP path resolution**: `CONTEXT_CAPSULE_HOME` defaults per OS (above); integration tests on all three runners.

Next: real-machine passes on Windows, GNOME/KDE Wayland and X11; signed Windows build
(MSIX / NSIS); AppImage + .deb.

## Later

- Remote MCP endpoint (streamable HTTP) for ChatGPT connectors and Claude.ai web.
- Capsule viewer (static HTML render of `.capsule.md`).
- Capsule diffing / merge for teams exchanging updated capsules.
- Developer ID signing and notarization for the macOS recorder.
