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

**The Context Capsule window** (menu → *Open Context Capsule*, ⌘O; opens on Settings the first
time) has everything in one place, with a sidebar:
- **Home** — Start / Stop contextualizing with a live timer, Speak a note, Add a screen, the
  current project (change it or make a new one), Make today's capsules, See today's timeline.
- **Timeline** — one day, short enough to read at a glance. "‹ Thursday 23 October ›" moves a day back or
  forward; the date opens a month calendar (a dot on days with recordings); Today, when another day shows. Reads are
  grouped into stretches: the same app and title in a row (a note, a "Not read" gap or a pause over a minute
  ends one). The header says the date, the time recorded and how many recordings; a day bar shows each
  stretch in its project's colour (click one to see it); **Where your time went** lists time per project
  ("Not sorted" for the rest; per app when nothing has a project). Each recording is one line ("10:00 –
  11:40 · 1 h 40 min · Website, Client report") with **Make a capsule** and a menu with Delete; the one still
  recording is open. Opened: stretches (app · title, time and length, page or file, a peek at the text,
  "24 reads"), voice and typed notes as speech bubbles, and "Not read 10:40 – 10:55: a private app was in
  front" (from `gaps.jsonl`, below). **Show what was read** lists a stretch's reads: time, page, "Looked at"
  (read under the mouse on another screen), the first 3 lines with **Show all** (the whole text is read from
  the file only then). **Search this day** keeps the stretches and notes that match. Delete a read, a stretch
  or a recording ("It can't be undone."); not while it's still recording ("Stop first"). Make a capsule
  opens your AI with `Use Context Capsule to make a capsule from my session <id>.` (a website or any other AI
  gets Copy my day for that day). While contextualizing, Today updates every few seconds.
- **Capsules** — by project, with Share (Copy file — paste into WhatsApp, Slack, email…; Copy
  text; Save a copy…; Show in Finder; More… for AirDrop/Mail/Messages) and Open. A search field
  filters by title and "In short" as you type (Return only filters); **Ask your AI** opens your AI
  with "Use Context Capsule to search my capsules: …". **Open a capsule…** picks a capsule someone
  sent you (below). **Copy my day** and **Save from clipboard** are always here, for any AI (below).

**Check before sharing.** Every share (Copy file, Copy text, Save a copy, More…) first lists what
might be private — People, Emails, Phone numbers, Private links — with a check box each (all
ticked), in a "Before you share" box. **Share** sends a copy, same file name in a temporary folder, with the checked items replaced
by `[removed]`; the saved capsule never changes. Nothing found: no box, it just shares. Plain rules
in `Core.swift` (`ShareCheck`), tested against `../spec/share-check.json`, the same cases as Windows/Linux.

**Opening a received capsule.** Context Capsule is in Finder's *Open With* for `.md` files (never
the default app), and takes capsule files dropped on its window. If the file is a capsule (front
matter with `capsule:` and `title:`), Claude opens with "Use Context Capsule to continue from the
capsule at <path>." (in your AI; connecting it first if needed); otherwise: "This isn't a capsule".
- **Projects** — rename, edit how they're found, delete.
- **Settings** — set up (read your apps, voice notes, Dictation, connect your AI, open at login), Your AI,
  the evening reminder and its time, when to delete old recordings, sounds, the voice-note
  language, apps never to record, and updates (Update automatically, Check for Updates).
- **Help** — shortcuts, how it works, privacy.

The menu-bar menu is the quick version: Start contextualizing (⌥C) · Speak a note (⌥V) · Add a
screen · Project · Open Context Capsule · Make today's capsules · Check for Updates · Quit.

Menu-bar icon: hollow circle = idle; **a red dot and the time** (● 12:34) while recording; **a
red mic with bars that move with your voice** while listening. Nothing appears on the screen
itself; soft sounds mark start and stop (`"sounds": false` to turn them off). No text pops up for
starting, stopping, a saved voice note, a project change or an added screen: only problems are shown. While recording the
menu shows how many events were written and, when a sample is skipped, why ("the front
window is not in what you picked", "Mission Control is open"…). If a recording ends
with nothing written, an alert says why.
Menu: Start / Stop recording, Read text directly (Accessibility)…, Open sessions
folder, Quit. While macOS shares the picked content you also see its own sharing
icon in the menu bar; stopping there stops the recording.

## Any AI

The app runs `context-capsule-mcp` (in `Contents/Resources`; built from `../mcp`, see SPEC.md "Any AI") with
`clients`, `connect`, `disconnect`, `day` and `save`, off the main thread with a time limit (`Sources/AIBridge.swift`).

- **Settings → Set up → Connect your AI** lists the AI apps on this Mac: Connect, or a ✓ (Disconnect in its ⋯
  menu); apps that aren't installed are under More. Claude installs the `.mcpb` extension, as before. The ✓ on
  the row means at least one AI is connected.
- **Your AI** (config.json `"ai"`: an id from `clients`, `chatgpt-web`, `gemini-web` or `any`; default: Claude
  if connected, else the first connected app, else `any`) decides what **Make today's capsules** does: Claude
  opens with the request typed in; another app opens with the request on the clipboard ("Paste it in a new
  chat"); a website or any other AI gets **Copy my day**.
- **Copy my day** copies one message with today's work (from `day`) and opens the website if one is chosen.
  Paste it into your AI, copy its whole reply, then **Save from clipboard**: `save` checks every capsule in
  it, and a small box says what was saved and what wasn't, and why.

## What wasn't read (gaps.jsonl)

Next to `events.jsonl`, each recording gets a `gaps.jsonl` once something wasn't read for 5 seconds or more:
`{"t":"…","end":"…","why":"private_app"}`. Samples skipped for the same reason one after another make one
line whose `end` moves, like events. The menu's skip reasons map to: a private app or a private window on
top → `private_app`; the front window isn't in what you shared (or the shared screen can't be found) →
`not_shared`; no window, or waiting for the screen to update → `no_text`; Mission Control → `system`; the
lock screen, the screen saver or a locked screen → `locked`. Only times and the reason, never any text.

## Several screens

When more than one screen is connected, **Add a screen** (in the menu while recording)
opens the picker again; up to 4 screens or windows can be shared, each chosen by you. Each
sample reads only the shared screen holding the window you're typing in (keyboard focus, not
the mouse pointer).

## Evening reminder and clean-up

- At 18:00, if you recorded today and no capsule has been made for today, a small box asks
  **Make today's capsules?** — *Make* does it with your AI (as above); *Not now*
  closes it (once a day). Change the time with `"remind_at": "19:30"`, or `false` to turn it off.
- Raw recordings older than 30 days are deleted (`"keep_recordings_days"`, `0` = keep
  forever). Capsules are never deleted. Both settings live in `~/.context-capsule/config.json`.

## Projects

Menu → **Project: …** shows the project of the window in front, detected from the clues in
`~/.context-capsule/projects.json` (matched in the window title, its page address or file
path, and the app name). Pick a project to stick to it until you pick **Auto-detect**
again; **New project from this window** makes one and remembers the window's clue. A short
message says when the project changes. Each event gets `"project"` and `"project_by"`
(`auto` / `you`), and `"where"` (the page address or file path, without `?query` or
`#fragment`) when Accessibility gives it.

## Voice notes

While recording, press **⌥V** (or menu → *Speak a note*), talk, press ⌥V again.

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

For people: `./package.sh` builds and makes `dist/Context-Capsule-<ver>-mac.dmg` (drag to
Applications) and `-mac.zip` (for the updater). With `DEVELOPER_ID` and `NOTARY_PROFILE` set it
signs with the hardened runtime, notarizes and staples; see [../release/README.md](../release/README.md).

Without a "Context Capsule Dev" certificate the app is **ad-hoc signed** (`codesign -s -`). macOS remembers permissions by
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

## Updates

Once a day (and a minute after launch) the app fetches `latest.json` and `latest.json.sig` from
the GitHub releases (`"update_feed"` in config.json changes the base URL). In order: the Ed25519
signature must match the key built into the app; the version must be newer; the Mac zip is
downloaded and its size and SHA-256 must match; it's unzipped next to the app; the new app must
be Context Capsule, say that version, and be validly signed by the same signer as the running app
(same Developer ID team, or the same local certificate). Then one small box says
"Context Capsule 0.2.0 is ready. Restart to update." (Later / Restart now). Restart moves the old
app aside, moves the new one in, quits, and a tiny shell script reopens it. Never while
contextualizing or during a voice note. Any failure keeps the current version and is logged
in app.log; you're told only if you clicked Check for Updates. `"auto_update": false` (Settings →
Update automatically) stops the daily check. A 404 (no release yet) counts as "up to date".

Run from the disk image or Downloads, the app asks once: **Move Context Capsule to
Applications?** (Move / Not now). Move copies it there, puts the old copy in the Trash (not on a
disk image), and reopens it.

`--test-update latest.json latest.json.sig <zip>` runs every check offline and installs nothing.

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
                    # front-window choice, Mission Control, frame freshness, OCR crop + masks, displays, AX text assembly + secure-field skipping, AX-vs-OCR choice,
                    # check before sharing (every case in ../spec/share-check.json), is-it-a-capsule, search,
                    # any AI (reading context-capsule-mcp's answers, Your AI, finding the program), gaps.jsonl, the timeline
"./build/Context Capsule.app/Contents/MacOS/ContextCapsule" --selftest   # OCR (whole, cropped, masked) on a generated image
```

## Files

| file | does |
|---|---|
| `Sources/Core.swift` | pure logic: paths, JSON, times, redaction, exclusion, event log, picked-content membership, AX text assembly, AX-vs-OCR choice, voice notes (silence, events, the press-to-talk state machine), check before sharing, is-it-a-capsule, search, gaps (gaps.jsonl), the timeline (a day read into recordings and stretches, time per project, fold, search, delete) |
| `Sources/Library.swift` | Share (with the "Check before sharing" box), opening Claude with a request, opening a received capsule |
| `Sources/AIBridge.swift` | any AI: runs context-capsule-mcp, Connect your AI, Your AI, Copy my day, Save from clipboard |
| `Sources/Picker.swift` | the system picker (`SCContentSharingPicker`) and the one `SCStream` of the picked content |
| `Sources/Screen.swift` | front app + window, Accessibility text, Vision OCR |
| `Sources/Recorder.swift` | sessions, 3 s sampling, notes into the session, gaps.jsonl, state.json, control file |
| `Sources/TimelineView.swift` | the Timeline page (a lazy list; files read off the main thread) |
| `Sources/Voice.swift` | mic + on-device Apple Speech for voice notes |
| `Sources/App.swift` | menu-bar icon and menu, ⌥C / ⌥V hotkeys, messages, Accessibility menu item, files opened with the app |
| `Sources/UpdateCore.swift` | pure update logic: the feed, its Ed25519 signature, versions, when to check |
| `Sources/Updater.swift` | updates (download, checks, same-signer check, swap and restart) and "Move to Applications?" |
| `Sources/SelfTest.swift` | `--selftest` |
| `Sources/main.swift` | entry point |
