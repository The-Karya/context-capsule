# Context Capsule — roadmap

## v1 (current): macOS

- Recorder: native Swift menu-bar agent (`capture/`), macOS 14+.
  - Capture scope: `SCContentSharingPicker` (ScreenCaptureKit), user-selected display/window per session; no Screen Recording TCC entitlement requested.
  - Text extraction: Accessibility (AX) tree of the focused window first; Vision OCR (`VNRecognizeTextRequest`) on the picked content as fallback, cropped to the frontmost window.
  - Voice notes: push-to-talk, `SFSpeechRecognizer` with `requiresOnDeviceRecognition`; no audio persisted.
- MCP server (`mcp/`): Node 18+, stdio transport, platform-independent.
- Contract: session files under `~/.context-capsule/` as defined in [SPEC.md](SPEC.md). Any recorder that writes this layout works with the MCP server unchanged.

## v2: Windows and Linux recorders

The MCP server and the capsule format are already cross-platform. v2 adds recorders
that emit the same `session.json` / `events.jsonl` / `state.json` / `control` contract.

### Approach

One Rust recorder for Windows and Linux (`capture-rs/`), built on the MIT-licensed
open-source screen-capture and OCR stack that Karya's desktop engine also derives
from. The macOS recorder stays native Swift (ScreenCaptureKit picker and AX are
first-class there). Shared, platform-neutral logic (redaction, never-record matching,
sample de-duplication, event serialisation, control-file protocol) moves into a small
Rust crate with golden tests generated from the Swift test vectors, so all recorders
produce byte-identical events for identical input.

### Platform matrix

| Concern | macOS (v1) | Windows (v2) | Linux (v2) |
|---|---|---|---|
| Capture scope (user consent) | `SCContentSharingPicker` | `Windows.Graphics.Capture` `GraphicsCapturePicker` | `xdg-desktop-portal` ScreenCast (PipeWire); X11 fallback with explicit window selection |
| Structured text | AX tree | UI Automation (`IUIAutomation`, TextPattern) | AT-SPI2 (D-Bus) |
| OCR fallback | Vision | `Windows.Media.Ocr` (on-device) | Tesseract (bundled traineddata) |
| Voice notes (on-device) | `SFSpeechRecognizer` | whisper.cpp (base.en / small, CPU) or `Windows.Media.SpeechRecognition` where available | whisper.cpp |
| Global hotkey | Carbon `RegisterEventHotKey` | `RegisterHotKey` | portal GlobalShortcuts (Wayland) / XGrabKey (X11) |
| Tray / menu bar | `NSStatusItem` | `Shell_NotifyIcon` | StatusNotifierItem (AppIndicator) |
| Front window + title | NSWorkspace + AX | `GetForegroundWindow` + UIA | portal / wlr-foreign-toplevel; `_NET_ACTIVE_WINDOW` on X11 |
| Data dir | `~/.context-capsule` | `%USERPROFILE%\.context-capsule` | `$XDG_DATA_HOME/context-capsule` (fallback `~/.context-capsule`) |
| Packaging | ad-hoc → Developer ID + notarization | MSIX / signed NSIS | AppImage + .deb |

### Milestones

1. **Shared core crate**: port Core.swift logic to Rust; golden-vector parity tests against the Swift suite.
2. **Windows recorder**: picker-scoped capture, UIA text, Windows OCR fallback, hotkeys, tray, control/state protocol; CI on `windows-latest`.
3. **Linux recorder**: portal ScreenCast + AT-SPI2 + Tesseract; Wayland-first, X11 fallback; CI on `ubuntu-latest` (Xvfb for X11 paths).
4. **Voice on Windows/Linux**: whisper.cpp integration with model download on first use, VAD-gated, no audio persisted.
5. **MCP path resolution**: `CONTEXT_CAPSULE_HOME` defaults per OS (above); integration tests on all three runners.

Estimated effort: Windows 1–2 days, Linux 1–2 days, shared core 0.5 day.

## Later

- Remote MCP endpoint (streamable HTTP) for ChatGPT connectors and Claude.ai web.
- Capsule viewer (static HTML render of `.capsule.md`).
- Capsule diffing / merge for teams exchanging updated capsules.
- Developer ID signing and notarization for the macOS recorder.
