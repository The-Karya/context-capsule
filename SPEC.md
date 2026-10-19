# context-capsule — v1 spec

Open-source, for macOS, Windows and Linux. Press a hotkey, work, press it again: Context Capsule reads
your screen while you work and helps your own AI (Claude, via MCP) turn that into a
**capsule** — one shareable file with what you worked on, the decisions you made and
why. Send the capsule to anyone; their Claude loads it and knows the project.

Everything stays on the laptop. No accounts, no cloud, no AI key of our own: the
user's Claude does the writing through MCP.

## Parts

```
context-capsule/
  capture/            Swift menu-bar app "Context Capsule" (macOS 14+)
  capture-rs/         Rust recorders: core/ (shared logic), windows/, linux/
  mcp/                TypeScript MCP server (stdio), Node >= 18
  SPEC.md             this file — the contract between the two
  README.md, LICENSE (MIT)
```

### 1. capture/ (Swift)

A small menu-bar app, built with `swiftc` (no Xcode project needed), one or a few
files. Build script `capture/build.sh` → `capture/build/Context Capsule.app`
(executable `ContextCapsule`, bundle id `com.thekarya.contextcapsule`).

- **Hotkey** `⌥⇧C` (Option-Shift-C) toggles recording. Also a menu: Start / Stop
  recording, Read text directly (Accessibility)…, Open sessions folder, Quit. Icon:
  hollow circle idle, filled circle while recording.
- **Start = the system picker.** Hotkey, menu or the `control` start command present
  `SCContentSharingPicker` (single window, single display; several windows on macOS
  15.2+). Recording starts only after the user picks. Closing the picker = no session:
  `recording` stays false and `state.json` gets `"last_error":"cancelled"`. Pressing the
  hotkey while the picker is open cancels it. A `control` start while the picker is
  open brings it to the front.
- Capture uses **only the picked filter**: one `SCStream`, `minimumFrameInterval` 1 s (frames only arrive when the content changes),
  frames kept in memory (latest one only). No `CGPreflightScreenCaptureAccess` /
  `CGRequestScreenCaptureAccess`, no `CGWindowListCreateImage` / `CGDisplayCreateImage`,
  no `SCShareableContent` — so macOS shows no Screen Recording permission alert.
- **Stop** stops the stream and sets `SCContentSharingPicker.shared.isActive = false`.
  If the user stops sharing from the macOS menu-bar sharing icon, the session ends too.
- While recording, every **3 s**:
  - frontmost app name + bundle id + frontmost window (`NSWorkspace`, `CGWindowList`
    ids/layers/frames). The front window is the app's first layer-0 window that is visible
    (alpha > 0) and at least 40×40 pt; a strip inside a bigger window of the same app
    (Chrome/Zen/Electron tab bars) gives way to that window;
  - **Mission Control / Exposé** (Dock windows below layer 20): skip the sample;
  - **is that window in the picked content?** Picked display: the window's centre is on
    it (the display's global frame: `includedDisplays` on 15.2+, else matched by size against
    the active displays; unknown → skip). Picked window(s): it is one of them (by window id on 15.2+, else by matching
    frame). If not, write nothing for this sample;
  - **text, Accessibility first:** if Accessibility is allowed, walk the focused
    window's AX tree (text roles `AXStaticText`, `AXTextArea`, `AXTextField`,
    `AXComboBox`, `AXHeading`, `AXLink`, `AXCell`; ≤ 2000 nodes, ≤ 400 ms, depth ≤ 60;
    secure text fields and everything inside them are never read). If that gives
    < 80 letters/digits, or Accessibility is not allowed, **OCR** the latest frame of
    the picked content with Apple Vision (`VNRecognizeTextRequest`, accurate, language
    correction on), cropped to the front window when a whole display was picked. OCR only
    uses a frame that arrived after the last app switch, space switch or Mission Control
    (else skip: the frame may still show the previous app). On a picked display, any
    never-record app window or notification banner **above** the front window is painted black before OCR; if
    such windows hide more than half of it, skip the sample;
  - window title: Accessibility if allowed, else the picked window's title (15.2+),
    else the `CGWindowList` name when macOS gives it out (never prompted for), else empty;
  - if the app, title and text are the same as the previous sample, skip it
    (only extend the previous event's `end`).
- **Voice notes** — `⌥⇧V` (menu *Add voice note*) starts talking, press again to stop;
  only while recording (else a message: "Start recording first (⌥⇧C)"). The mic opens
  only between the presses; the menu-bar icon shows a mic while listening and resets on
  every way out (saved, empty, denied, error, 2-min limit, recording stopped).
  `AVAudioEngine` input tap → `SFSpeechRecognizer` with `requiresOnDeviceRecognition`;
  no on-device model for the locale → tell the user, never use Apple's servers. Locale:
  the user's, or `"voice_locale"` in config.json. No audio on disk. Silent clips (peak
  < 0.01, or < 0.35 s, or no words) are skipped: "Didn't catch that". Logs: counts and
  durations only. Info.plist has `NSMicrophoneUsageDescription` and
  `NSSpeechRecognitionUsageDescription`.
- **Never record** (skip the sample entirely, write nothing): password managers and
  private apps — bundle ids containing `1password`, `bitwarden`, `keychain`,
  `lastpass`, `dashlane`, `signal`, `whatsapp`, plus a user list in
  `~/.context-capsule/config.json` → `{"exclude_apps": ["com.example.app"]}`.
- **Images are not kept.** Only text is written.
- Redact before writing: strings that look like API keys / tokens
  (`sk-…`, `ghp_…`, `xox[bp]-…`, `AKIA…`, long hex/base64 > 32 chars), and
  `password: …` lines → `[redacted]`.
- Control from outside (the MCP server uses this): the app watches
  `~/.context-capsule/control` — a JSON file `{"command":"start"|"stop","at":<ms>}`.
  A start opens the picker; the user still has to pick.
  `{"command":"note","text":"…","at":<ms>}` adds a typed note to the current session.
  It also writes `~/.context-capsule/state.json`:
  `{"recording": bool, "session_id": string|null, "started_at": iso|null, "pid": int,
  "picking": bool, "last_error": string|null, "events": int, "skip": string|null, "listening": bool, "note_at": int|null}` —
  `picking` is true while the picker waits for the user; `last_error` says why the last
  start did not happen (`"cancelled"`, `"picker: …"`, `"capture: …"`) and is cleared by
  the next start. `events` = events written this session; `skip` = why the latest sample
  wrote nothing (`private_app`, `no_window`, `not_picked`, `overlay`, `private_window`,
  `waiting_frame`, `unknown_display`) or null. After a stop both describe the ended
  session. The menu shows the event count and the skip reason; a session that ends with
  0 events shows an alert saying why. `listening` is true while a voice note is open;
  `note_at` is the `at` of the last typed note written (the MCP waits for it).
- Permissions: none required (the picker is the consent). Accessibility is optional:
  the menu item calls `AXIsProcessTrustedWithOptions` with the prompt and shows a check
  mark once allowed. Recording never waits for it.

### 1b. capture-rs/ (Rust: Windows and Linux)

Same behaviour and the same files as `capture/`, through a shared crate
(`capture-rs/core`: a port of the Swift logic, tested against the same vectors so
events are byte-identical) and one tray app per OS. Hotkeys **Ctrl+Alt+Shift+C**
(record) and **Ctrl+Alt+Shift+V** (voice note).

| | Windows 10 1903+ / 11 | Linux (Wayland, X11) |
|---|---|---|
| Picker = consent | `GraphicsCapturePicker` | xdg-desktop-portal ScreenCast (PipeWire) |
| Text first | UI Automation (password fields never read) | AT-SPI (password text never read) |
| OCR fallback | Windows.Media.Ocr | Tesseract CLI, image piped in memory |
| Overlay skipped | Task View, Alt-Tab | GNOME Activities, KDE Overview |
| Masked | notification toasts, never-record windows | X11 notifications, never-record windows |
| Voice notes | whisper.cpp + cpal | whisper.cpp + cpal |

- If the picked item can't be identified for sure, or (Wayland) where the front window
  sits on a picked screen is unknown, OCR fails closed: nothing is read from pixels.
- Voice notes on Windows/Linux use whisper.cpp ("base" multilingual model, ~148 MB,
  verified by SHA-256), downloaded once into `<data>/models/` only after the user agrees.
- Data folder: `CONTEXT_CAPSULE_HOME`; else on Linux `$XDG_DATA_HOME/context-capsule`
  when set; else `~/.context-capsule` (`%USERPROFILE%\.context-capsule` on Windows).
  The MCP server uses the same rule.

### Session files (the contract)

```
~/.context-capsule/
  sessions/<session_id>/          session_id = YYYYMMDD-HHMMSS
    session.json                  {"id","started_at","ended_at"|null,"host","version":1}
    events.jsonl                  one JSON object per line, see below
  capsules/<slug>.capsule.md      saved capsules
  config.json, control, state.json
```

`events.jsonl` line:

```json
{"t":"2026-10-19T10:01:03Z","end":"2026-10-19T10:01:42Z","app":"Google Chrome",
 "bundle":"com.google.Chrome","title":"Pricing — Linear","src":"ax","text":"…screen text…"}
```

`src` is where `text` came from: `"ax"` (Accessibility), `"ocr"`, `"voice"` (a voice
note) or `"typed"` (a typed note). Optional `kind`: `"screen"` (default when missing),
`"voice"` or `"note"` — the user's own words:

```json
{"t":"2026-10-19T10:14:02Z","end":"2026-10-19T10:14:19Z","kind":"voice","src":"voice",
 "app":"Figma","bundle":"com.figma.Desktop","title":"Pricing v2","text":"We keep annual…"}
```

`app`/`bundle`/`title` of a note are the front window when it started (empty for a
private app or a typed note). Notes are never merged with other events. Readers must
ignore unknown fields; older files have no `src`. `text` is capped at 4000 chars per event
(keep the start). Times are ISO-8601 UTC.

### 2. mcp/ (TypeScript)

`@modelcontextprotocol/sdk`, stdio transport. `bun` for dev; the built output runs
with plain `node` so `npx`/Claude Desktop can launch it. Binary name
`context-capsule-mcp`.

Tools:

| tool | does |
|---|---|
| `start_recording` | writes `control` start, waits up to ~6 s for `picking` (picker open), `recording` (session id) or `last_error` (e.g. cancelled); errors clearly if the app isn't running, with how to open it |
| `stop_recording` | writes `control` stop; returns the session id |
| `recording_status` | reads `state.json`: running, picking, recording, events, skip, listening, last_error |
| `add_note` `{text}` | writes `control` note; waits for `note_at`; errors if not recording |
| `list_sessions` | id, start, end, duration, text source mix (ax / ocr), note count, top apps |
| `get_session_activity` `{session_id, max_chars?=60000}` | a condensed timeline for the AI: grouped by app/window, time spans, de-duplicated text (drop lines seen before, collapse whitespace), notes inline as `>> You said (HH:MM): "…"` / `>> You typed (HH:MM): "…"`, trimmed to `max_chars` — notes budgeted first and never cut before screen text, then a fair share per segment |
| `save_capsule` `{title, markdown, session_ids?}` | validates + writes `capsules/<slug>.capsule.md` with front matter; returns path |
| `list_capsules` | title, author, created, path |
| `get_capsule` `{name}` | full text |
| `import_capsule` `{path}` | copies a received `.capsule.md` into `capsules/` (validates format) |
| `export_capsule` `{name, to_path?}` | copies to Desktop (default) or a path; returns the path, ready to email |

Prompt `make_capsule` `{session_id}`: tells the AI how to write a capsule from
`get_session_activity` — sections below, decisions must cite the moment/app they came
from, mark guesses as guesses, never include secrets, keep it under ~1500 words. Voice
notes are the user's own words: the primary source for decisions, reasons and intent;
screen text is supporting evidence; quote or closely paraphrase them and cite the time.

Resource: each capsule as `capsule://<slug>` so Claude can read them directly.

### Capsule format (`*.capsule.md`)

```markdown
---
capsule: 1
title: Pricing page redesign
author: Saachi            # from config.json "author", else OS user full name
created: 2026-10-19T11:30:00Z
sessions: [20261019-100103]
apps: [Figma, Google Chrome, Linear]
---

## Summary
## What I worked on
## Decisions
- **Decision** — why. *(seen in Linear, 10:14)*
## Open questions
## Next steps
## Sources
```

Plain Markdown, readable by a human, by Claude, ChatGPT or anything else.

## Out of scope for v1

ChatGPT remote connector (needs hosting), images, cloud sync, teams.

## Rules

- No secrets, no Karya-internal code or URLs. MIT. Clean, small, readable.
- Every source file starts with a short header comment naming the project.
