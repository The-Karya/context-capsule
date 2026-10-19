# Context Capsule — the capture app

A small macOS menu-bar app (macOS 14+). Press **⌥⇧C** (Option-Shift-C): the macOS
window/screen picker opens. Pick a window or a display and recording starts; close
the picker to cancel. Press ⌥⇧C again to stop.

While recording, every 3 seconds it notes the front app and, **only if the front
window is part of what you picked**, its title and text. Text comes from the
Accessibility API when you allow it (exact, and secure fields are never read);
otherwise, or when an app shows too little text that way (Figma, video, canvas apps),
from Apple Vision OCR of the picked content. Each event says which: `"src":"ax"` or
`"src":"ocr"`. Only text is saved, on this Mac, in `~/.context-capsule/sessions/`.
Frames stay in memory and are never written to disk. The file format is in
[`../SPEC.md`](../SPEC.md).

Menu-bar icon: hollow circle = idle, filled circle = recording.
Menu: Start / Stop recording, Read text directly (Accessibility)…, Open sessions
folder, Quit. While macOS shares the picked content you also see its own sharing
icon in the menu bar; stopping there stops the recording.

## Build

Needs the Xcode command line tools (`xcode-select --install`). No Xcode project.

```sh
./build.sh                 # universal (arm64 + x86_64) → "build/Context Capsule.app"
ARCHS=arm64 ./build.sh     # faster, one arch
open "build/Context Capsule.app"
```

The app is **ad-hoc signed** (`codesign -s -`). macOS remembers permissions by
signature, and every rebuild gets a new one, so after a rebuild the Accessibility
check mark may be gone (turn Context Capsule off and on again in System Settings → Privacy &
Security → Accessibility).

## Permissions

- **Screen Recording** — not needed. Context Capsule never asks for it and never captures
  anything except what you pick in the system picker.
- **Accessibility** — optional. Menu → *Read text directly (Accessibility)…* shows the
  macOS prompt; the item has a check mark once allowed. With it, text and window
  titles come straight from apps; without it, text comes from OCR and titles are
  empty unless you picked a single window (macOS 15.2+). Recording never waits for it.

## Privacy

- Never records password managers and private apps: any bundle id or app name
  containing `1password`, `bitwarden`, `keychain`, `lastpass`, `dashlane`, `signal`,
  `whatsapp` (plus Apple Passwords and the lock screen). Those samples are skipped
  entirely: nothing is read.
- Add your own in `~/.context-capsule/config.json` (exact bundle id or app name):
  ```json
  {"exclude_apps": ["com.example.app"]}
  ```
- Before writing, text and titles are scrubbed: `sk-…`, `ghp_…`, `github_pat_…`,
  `xox[bp]-…`, `AKIA…`, `Bearer …`, long hex, long random-looking base64, and whole
  `password: …` lines become `[redacted]`. Text is capped at 4000 characters per event.

## Control from outside

The MCP server (`../mcp`) starts and stops recording by writing
`~/.context-capsule/control`:

```json
{"command": "start", "at": 1792404063000}
```

Context Capsule checks it twice a second and acts once per new `at`. A command written
before the app launched is ignored. A start opens the picker (brought to the front);
the user still has to pick. The app keeps `~/.context-capsule/state.json` current:

```json
{"recording": true, "session_id": "20261019-100103", "started_at": "2026-10-19T10:01:03Z", "pid": 4242,
 "picking": false, "last_error": null}
```

`picking` is true while the picker waits. If the user closes it, `recording` stays
false and `last_error` is `"cancelled"`.

## Tests

```sh
./tests/run.sh      # redaction, exclusion, dedupe, file formats, picked-content membership,
                    # OCR crop, AX text assembly + secure-field skipping, AX-vs-OCR choice
"./build/Context Capsule.app/Contents/MacOS/ContextCapsule" --selftest   # OCR (whole + cropped) on a generated image
```

## Files

| file | does |
|---|---|
| `Sources/Core.swift` | pure logic: paths, JSON, times, redaction, exclusion, event log, picked-content membership, AX text assembly, AX-vs-OCR choice |
| `Sources/Picker.swift` | the system picker (`SCContentSharingPicker`) and the one `SCStream` of the picked content |
| `Sources/Screen.swift` | front app + window, Accessibility text, Vision OCR |
| `Sources/Recorder.swift` | sessions, 3 s sampling, state.json, control file |
| `Sources/App.swift` | menu-bar icon and menu, ⌥⇧C hotkey, Accessibility menu item |
| `Sources/SelfTest.swift` | `--selftest` |
| `Sources/main.swift` | entry point |
