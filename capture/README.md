# Context Capsule — the capture app

A small macOS menu-bar app (macOS 14+). Press **⌥C** (Option-C): the macOS
window/screen picker opens. Pick a window or a display and recording starts; close
the picker to cancel. Press ⌥C again to stop.

While recording, every 3 seconds it notes the front app and, **only if the front
window is part of what you picked**, its title and text. If you pick one screen, windows
on your other screens are not read. Text comes from the
Accessibility API when you allow it (exact, and secure fields are never read);
otherwise, or when an app shows too little text that way (Figma, video, canvas apps),
from Apple Vision OCR of the picked content, cropped to the front window. OCR never uses
a frame from before the last app switch, never runs while Mission Control is open, and
paints out any private app window (below) or notification banner that sits on top of the front window. Each event says which: `"src":"ax"` or
`"src":"ocr"`. Only text is saved, on this Mac, in `~/.context-capsule/sessions/`.
Frames stay in memory and are never written to disk. The file format is in
[`../SPEC.md`](../SPEC.md).

Menu: **Start recording** (⌥C) · **Add voice note** (⌥V) · **Project** · **Make today's
capsules** (opens a new Claude chat with the request ready to paste (⌘V, Return); stops a running
recording first) · **My capsules…** (capsules by project with Share / Open, and a Projects tab to
rename, edit or delete projects) · **Set up Context Capsule…** · Quit.

Menu-bar icon: hollow circle = idle, **red** filled circle = recording, **red** mic = listening
to a voice note. A short message says "● Recording started" / "■ Recording stopped: N events
saved", and "🎙 Listening… press ⌥V to finish" stays on screen for as long as the mic is on. While recording the
menu shows how many events were written and, when a sample is skipped, why ("the front
window is not in what you picked", "Mission Control is open"…). If a recording ends
with nothing written, an alert says why.
Menu: Start / Stop recording, Read text directly (Accessibility)…, Open sessions
folder, Quit. While macOS shares the picked content you also see its own sharing
icon in the menu bar; stopping there stops the recording.

## Evening reminder and clean-up

- At 18:00, if you recorded today and no capsule has been made for today, a small box asks
  **Make today's capsules?** — *Make* opens the Claude app with the request typed in; *Not now*
  closes it (once a day). Change the time with `"remind_at": "19:30"`, or `false` to turn it off.
- Raw recordings older than 30 days are deleted (`"keep_recordings_days"`, `0` = keep
  forever). Capsules are never deleted. Both settings live in `~/.context-capsule/config.json`.

## Projects

Menu → **Project: …** shows the project of the window in front, detected from the clues in
`~/.context-capsule/projects.json` (matched in the window title, its page address or file
path, and the app name). Pick a project to stick to it until you pick **Auto-detect**
again; **New project from this window…** makes one and remembers the window's clue. A short
message says when the project changes. Each event gets `"project"` and `"project_by"`
(`auto` / `you`), and `"where"` (the page address or file path, without `?query` or
`#fragment`) when Accessibility gives it.

## Voice notes

While recording, press **⌥V** (or menu → *Add voice note*), talk, press ⌥V again.

- The mic opens only between the two presses; the menu-bar icon is a microphone while it
  listens, and goes back to the filled circle on every way out (saved, nothing heard,
  permission denied, error, the 2-minute limit, recording stopped).
- `AVAudioEngine` feeds the mic straight into `SFSpeechRecognizer` with
  `requiresOnDeviceRecognition = true`. If the language has no on-device model, you are
  told and nothing happens; it never uses Apple's servers. Language: your locale, or
  `"voice_locale"` in `~/.context-capsule/config.json`.
- No audio is written anywhere. Silent or very short clips are dropped ("Didn't catch
  that", naming the input device). Logs say only counts and durations, never the words.
- A note is written as `{"t","end","kind":"voice","src":"voice","app","bundle","title","text"}`
  (the front app when you started talking; empty for a private app), with the same
  redaction and 4000-character cap. Stopping the recording mid-note finishes and saves it.
- Not recording? A small message says "Start recording first (⌥C)".
- The MCP's `add_note` sends a typed note the same way (`control` command `note`);
  it is written as `"kind":"note","src":"typed"`.

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
  titles come straight from apps; without it, text comes from OCR and titles come from
  the picked window (macOS 15.2+) or the window list when macOS gives them out, else
  they are empty. Recording never waits for it. Clicking the item a second time opens
  System Settings and explains how to re-add the app after a rebuild; the check mark
  updates on its own once allowed.

- **Microphone + Speech Recognition** — only for voice notes, asked the first time you
  press ⌥V. **Dictation** must also be on (System Settings → Keyboard → Dictation);
  without it Apple refuses speech recognition (kLSRErrorDomain 201) and the app says so. If denied, the message says where to turn them on (System Settings →
  Privacy & Security → Microphone / Speech Recognition).

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
{"command": "note", "text": "We chose Postgres because the team knows it", "at": 1792404063000}
```

Context Capsule checks it twice a second and acts once per new `at`. A command written
before the app launched is ignored. A start opens the picker (brought to the front);
the user still has to pick. The app keeps `~/.context-capsule/state.json` current:

```json
{"recording": true, "session_id": "20261019-100103", "started_at": "2026-10-19T10:01:03Z", "pid": 4242,
 "picking": false, "last_error": null, "events": 12, "skip": null, "listening": false, "note_at": null}
```

`picking` is true while the picker waits. If the user closes it, `recording` stays
false and `last_error` is `"cancelled"`.

## Tests

```sh
./tests/run.sh      # redaction, exclusion, dedupe, file formats, picked-content membership,
                    # front-window choice, Mission Control, frame freshness, OCR crop + masks, displays, AX text assembly + secure-field skipping, AX-vs-OCR choice
"./build/Context Capsule.app/Contents/MacOS/ContextCapsule" --selftest   # OCR (whole, cropped, masked) on a generated image
```

## Files

| file | does |
|---|---|
| `Sources/Core.swift` | pure logic: paths, JSON, times, redaction, exclusion, event log, picked-content membership, AX text assembly, AX-vs-OCR choice, voice notes (silence, events, the press-to-talk state machine) |
| `Sources/Picker.swift` | the system picker (`SCContentSharingPicker`) and the one `SCStream` of the picked content |
| `Sources/Screen.swift` | front app + window, Accessibility text, Vision OCR |
| `Sources/Recorder.swift` | sessions, 3 s sampling, notes into the session, state.json, control file |
| `Sources/Voice.swift` | mic + on-device Apple Speech for voice notes |
| `Sources/App.swift` | menu-bar icon and menu, ⌥C / ⌥V hotkeys, messages, Accessibility menu item |
| `Sources/SelfTest.swift` | `--selftest` |
| `Sources/main.swift` | entry point |
