# Context Capsule — the Windows recorder

A small tray app for Windows 10 (2004+) and 11. It drives the shared engine in
[`../core`](../core) and writes exactly the same files as the macOS app (see
[`../../SPEC.md`](../../SPEC.md)), so the MCP server works with it unchanged.

Press **Alt+C**: the Windows capture picker opens. Pick a window or a screen and
recording starts; close the picker to cancel. Press Alt+C again to stop.

While recording, every 3 seconds it notes the front app and, **only if the front window is
part of what you picked**, its title and text. Text comes from UI Automation (password fields
are never read); when that gives too little (canvas apps, video, games), from Windows OCR of
the picked content, cropped to the front window, with private app windows and notification
toasts painted out. OCR never uses a frame from before the last app or desktop switch, and
nothing is read while Task View or Alt-Tab is open.

It also reads where you look: when the mouse moved or scrolled over a window on **another**
shared screen in the last 30 seconds, that window is read too (only its new lines; never a
private app). Those events say `"focus":"pointer"`: seen, not counted as time spent. The mouse
is watched only while recording (a mouse hook that notes the time of moves and scrolls, nothing
else). Only text is saved, in
`%USERPROFILE%\.context-capsule\sessions\`. Frames stay in memory and are never written to disk.

The taskbar, Start, Search, the notification and action centers, Quick Settings, Alt-Tab and
Task View are never "the app you work in": when one is in front, the top window of a real app
is used instead (a private app on top still means nothing is read).

Tray icon: hollow circle = idle, **red** filled circle = recording (the running time is in its
tooltip and the menu's first line: the tray can't show text), **red** microphone with bars that
move with your voice = listening. Nothing else stays on screen. A short message and a soft sound
mark "● Recording started…" / "■ Recording stopped: N events saved."; the mic opening and
closing has a soft sound too (`"sounds": false` in config.json turns them off). The menu shows
the time, the event count and, when a sample is skipped, why. If a recording ends with nothing
written, a message says why. Menu: Start / Stop recording, Add / Finish voice note, Add another
screen… (while recording: the picker again, up to 4 screens or windows; each sample uses the
one holding the window you type in; picking one that's already shared says "That screen is
already shared. Pick a different one."), Project (Auto-detect, your projects, New project from this
window… — works without a recording too), Settings, Quit.

Each sample saves only the lines not already saved for that window in this recording, so long
documents are kept whole as you scroll (no size limit; a 100,000-character safety net).

Evening reminder: at 18:00 (`"remind_at": "19:30"`, or `false` to turn it off), if something was
recorded today and no capsule exists for today, a box asks "Make today's capsules?" once a day.
**Make** puts the request on the clipboard and opens the Claude app on a new chat; press Ctrl+V,
then Enter. Raw recordings older than 30 days are deleted (`"keep_recordings_days"`, 0 = keep);
capsules are never deleted.

## Setup window

Shown on first launch and from the tray menu (**Settings**): Voice notes
(microphone allowed? **Allow** opens the Windows microphone settings), Connect Claude (opens
`context-capsule.mcpb`, shipped next to the exe, with the Claude app, which asks to install it;
without the Claude app it offers the download page), Open at login (a `Run` entry under
HKCU). Each row shows a check mark when done. Connected means the extension is in Claude's
`Claude Extensions` folder or `claude_desktop_config.json` has a `context-capsule` server.

## Hotkeys

| keys | does |
|---|---|
| Alt+C | start (opens the picker) / cancel the picker / stop recording (waits while a voice note listens) |
| Alt+V | start / finish a voice note (only while recording) |

The same keys as the Mac's ⌥C / ⌥V. If another app already uses one of them, Context Capsule
logs it, says so once at launch, and the tray menu still works.

## The yellow border

While Windows shares a window or screen with an app, it draws a **yellow border** around it.
That is Windows telling you what Context Capsule can see; it goes away when you stop
recording. Context Capsule leaves it on.

## Voice notes

While recording, press **Alt+V**, talk, press it again.

- Recognition runs on this PC with [whisper.cpp](https://github.com/ggerganov/whisper.cpp)
  (the multilingual "base" model). The first time, a dialog asks before downloading the model
  (about 148 MB, once) into `%USERPROFILE%\.context-capsule\models\`. It is checked by size
  and SHA-256 before it is used. Until then, voice notes say how to get it. You can also put
  `ggml-base.bin` there yourself.
- The microphone is open only between the two presses; the tray icon is a microphone while it
  listens. No audio is written anywhere and nothing is sent to any server.
- Language: your Windows locale (region format), or `"voice_locale"` in
  `%USERPROFILE%\.context-capsule\config.json` (e.g. `"de-DE"`).
- Silent or very short clips are dropped ("Didn't catch that"). A real fault says what it is
  (microphone blocked and where to turn it on, no sound from the device, the model missing).
  Logs say only counts, durations and error codes, never the words.

## Permissions

- **Screen capture** — none needed. The picker is the consent: only what you pick is captured.
- **UI Automation** — none needed.
- **Microphone** — only for voice notes. If it is off, the message says where to turn it on:
  Settings → Privacy & security → Microphone (also "Let desktop apps access your microphone").

## Build

Needs Rust (MSVC toolchain). For voice notes also CMake, the Visual Studio C++ build tools and
LLVM (whisper.cpp is compiled from source: `winget install Kitware.CMake LLVM.LLVM`).

```powershell
cd capture-rs\windows
.\build.ps1            # with voice notes
.\build.ps1 -NoVoice   # without (no C++ toolchain needed)
..\target\release\context-capsule.exe
```

Both build scripts copy `mcp/dist-ext/context-capsule.mcpb` next to the exe (build it first
with `mcp/extension/build.sh`). Started from a terminal, it prints its log lines there; the
same lines go to `app.log` in the data folder (counts, reasons and error codes only, capped at
256 KB). Data folder: `CONTEXT_CAPSULE_HOME`
if set, else `%USERPROFILE%\.context-capsule` (the same rule as the MCP server).

## Tests

The pure rules (which window or screen was picked, UI Automation roles, window classes for
toasts and overviews, 16 kHz resampling, model checks, menu text, toast placement) run on any OS:

```sh
cd capture-rs && cargo test -p context-capsule-windows
```

## Files

| file | does |
|---|---|
| `src/main.rs` | entry point |
| `src/app.rs` | the UI thread: hidden owner window, tray menu, cues, alerts; starts the engine thread |
| `src/setup.rs` | the setup window (voice notes, Connect Claude, open at login) |
| `src/connect.rs` | Connect Claude: the extension next to the exe, connected or not |
| `src/platform.rs` | the core's `Platform` for Windows |
| `src/picker.rs` | `GraphicsCapturePicker`, which item was picked, the capture and its latest frame |
| `src/windows_list.rs` | front app, on-screen windows (EnumWindows + DWM), monitors, toast / overview rules |
| `src/uia.rs` | UI Automation text |
| `src/ocr.rs` | Windows.Media.Ocr |
| `src/voice.rs` | microphone (cpal) + whisper.cpp voice notes |
| `src/model.rs` | the speech model: path, SHA-256 check, consented download |
| `src/tray.rs` | tray icon (drawn in code) and menu text |
| `src/toast.rs` | fading messages near the tray |
| `src/hotkeys.rs` | global hotkeys |
| `src/pointer.rs` | the mouse while recording: position and last move or scroll |
| `src/changes.rs` | when the front app or virtual desktop last changed |
| `src/win.rs` | small Win32 helpers |
