# Capsule — the capture app

A small macOS menu-bar app (macOS 13+). Press **⌥⇧C** (Option-Shift-C) to start
recording, press it again to stop. While recording, every 3 seconds it notes the
front app, the window title, and the text on screen (Apple Vision OCR). Only text is
saved, on this Mac, in `~/.karya-capsule/sessions/`. Screenshots stay in memory and
are never written to disk. The file format is in [`../SPEC.md`](../SPEC.md).

Menu-bar icon: hollow circle = idle, filled circle = recording.
Menu: Start / Stop recording, Open sessions folder, Quit.

## Build

Needs the Xcode command line tools (`xcode-select --install`). No Xcode project.

```sh
./build.sh                 # universal (arm64 + x86_64) → build/Capsule.app
ARCHS=arm64 ./build.sh     # faster, one arch
open build/Capsule.app
```

The app is **ad-hoc signed** (`codesign -s -`). macOS remembers permissions by
signature, and every rebuild gets a new one, so **macOS may ask for Screen
Recording again after each rebuild** (turn Capsule off and on again in System
Settings → Privacy & Security → Screen Recording, then reopen it).

## Permissions

- **Screen Recording** — required. On first launch Capsule shows one alert that
  explains why and opens System Settings. After granting it, quit and reopen Capsule.
- **Accessibility** — optional. If allowed, window titles come from the focused
  window (more exact); otherwise from the window list.

## Privacy

- Never records password managers and private apps: any bundle id or app name
  containing `1password`, `bitwarden`, `keychain`, `lastpass`, `dashlane`, `signal`,
  `whatsapp` (plus Apple Passwords and the lock screen). Those samples are skipped
  entirely, with no screenshot.
- Add your own in `~/.karya-capsule/config.json` (exact bundle id or app name):
  ```json
  {"exclude_apps": ["com.example.app"]}
  ```
- Before writing, text and titles are scrubbed: `sk-…`, `ghp_…`, `github_pat_…`,
  `xox[bp]-…`, `AKIA…`, `Bearer …`, long hex, long random-looking base64, and whole
  `password: …` lines become `[redacted]`. Text is capped at 4000 characters per event.

## Control from outside

The MCP server (`../mcp`) starts and stops recording by writing
`~/.karya-capsule/control`:

```json
{"command": "start", "at": 1792404063000}
```

Capsule checks it twice a second and acts once per new `at`. A command written
before Capsule launched is ignored. Capsule keeps `~/.karya-capsule/state.json` current:

```json
{"recording": true, "session_id": "20261019-100103", "started_at": "2026-10-19T10:01:03Z", "pid": 4242}
```

## Tests

```sh
./tests/run.sh                                     # redaction, exclusion, dedupe, file formats
./build/Capsule.app/Contents/MacOS/Capsule --selftest   # OCR on a generated image; no permission needed
```

## Files

| file | does |
|---|---|
| `Sources/Core.swift` | pure logic: paths, JSON, times, redaction, exclusion, event log |
| `Sources/Screen.swift` | front app + window title, screenshot, Vision OCR |
| `Sources/Recorder.swift` | sessions, 3 s sampling, state.json, control file |
| `Sources/App.swift` | menu-bar icon and menu, ⌥⇧C hotkey, permission alert |
| `Sources/SelfTest.swift` | `--selftest` |
| `Sources/main.swift` | entry point |
