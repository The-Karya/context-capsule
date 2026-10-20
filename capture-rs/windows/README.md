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
window… — works without a recording too), Open Context Capsule, Make today's capsules, Check for updates, Quit.

Each sample saves only the lines not already saved for that window in this recording, so long
documents are kept whole as you scroll (no size limit; a 100,000-character safety net).

Evening reminder: at 18:00 (`"remind_at": "19:30"`, or `false` to turn it off), if something was
recorded today and no capsule exists for today, a box asks "Make today's capsules?" once a day.
**Make** puts the request on the clipboard and opens the Claude app on a new chat; press Ctrl+V,
then Enter. Raw recordings older than 30 days are deleted (`"keep_recordings_days"`, 0 = keep);
capsules are never deleted.

## Install, uninstall, updates

The download is one file, `Context-Capsule-<version>-windows.exe`. No admin, no installer.

- **Install**: run it from anywhere (Downloads…) and it asks *Install Context Capsule?* —
  **Install** copies it to `%LOCALAPPDATA%\Programs\Context Capsule\context-capsule.exe`, adds a
  Start menu shortcut and an entry in Settings → Apps (under HKCU: publisher Karya, the version,
  *Uninstall* runs `context-capsule.exe --uninstall`), points open at login at the installed copy,
  then starts it. A copy that is already running is asked to quit first. **Just run it** runs it
  where it is (it asks again the next time you open that file, never at login).
- The Claude extension (`context-capsule.mcpb`) is inside the exe and written next to it when
  missing or changed, so Connect Claude works from a single download.
- **Uninstall** (Settings → Apps → Context Capsule → Uninstall): the running copy quits; the
  shortcut, the Settings → Apps entry, open at login, our registry key and the program folder go.
  Your recordings and capsules stay unless you answer **Yes** to *Also delete your recordings and
  capsules?* (No is the default).
- **Updates**: a minute after launch and then once a day (never while contextualizing or during a
  voice note), it fetches `latest.json` and its Ed25519 signature from the release feed
  (`"update_feed"` in config.json overrides it; `"auto_update": false` turns automatic checks off).
  Signature first, then the version (only newer), then the new exe is downloaded next to the running
  one as `<exe>.new` and checked (size and SHA-256). Then: *Context Capsule 0.2.0 is ready. Restart
  to update.* **Restart now** renames the running exe to `.old` (Windows allows that), moves the new
  one in, starts it and quits; the new one waits for the old one to end and deletes `.old`. Any
  failure keeps the current version and is only logged (shown only after **Check for updates**,
  in the tray menu and in Settings → Updates). No network = no update, silently. Nothing about you is sent.
- Settings → Updates: *Update automatically* (`auto_update`), the version, and **Check for updates**.

## The Context Capsule window

Everything in one place, like the Mac's window: tray menu → **Open Context Capsule**. The first
launch opens it by itself on **Settings**. A sidebar in two groups, *Your work* (Home, Capsules,
Projects) and *Context Capsule* (Settings, Help); the page title and a small status pill
("● 0:12" while contextualizing, else "Today 2 m") at the top.

- **Home**: "Good evening, Saachi." and the date; one panel with Today (time contextualized),
  This week (time, capsules made) and Streak (days in a row); the status card (Start
  contextualizing / Stop, Speak a note, Add a screen); Today's time per project as bars;
  Recent capsules (the 3 newest, See all). Beside it when wide (below when narrow): Shortcuts
  (Alt+C, Alt+V), Working on (Find by itself, your projects, New project), and Make today's
  capsules.
- **Capsules**: by project, the growing *Whole project* capsule first, then days, newest first.
  Open, and ⋯ → Copy file (paste it into a chat, an email or a folder), Copy text, Save a copy…,
  Show in folder.
- **Projects**: rename, change the words they're found by, delete (asks first).
- **Settings**: your name (config.json `author`; it goes on your capsules), Set up (Voice notes:
  microphone allowed? **Allow** opens the Windows microphone settings; Connect Claude: opens
  `context-capsule.mcpb`, written next to the exe from inside it, with the Claude app, which asks to install
  it, or offers the download page; Open at login: a `Run` entry under HKCU — each row shows a
  check mark when done), Every day (evening reminder and its time, when to delete old
  recordings, sounds), Voice notes language, Appearance (System / Light / Dark, `appearance`),
  Never record (pick an open app, or find its .exe), Updates (Update automatically, the version,
  Check for updates). Every change is saved at once.
- **Help**: shortcuts, how it works, privacy.

Raw recordings are never shown in the window: only time per project, counts and capsules.
Time per project follows the same rules as the Mac and Claude (core `WorkStats`). Connected
means the extension is in Claude's `Claude Extensions` folder or `claude_desktop_config.json`
has a `context-capsule` server.

Look: drawn in code with Windows 11's colors and the system accent, Segoe UI Variable (Segoe
UI on Windows 10), light or dark following Windows (or the Appearance setting), a dark title
bar and dark menus in dark mode. Text boxes are real edit controls; choices are standard
popup menus. Keyboard: Tab / Shift+Tab, Enter or Space, arrows and Page Up / Down scroll.

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

The exe carries `mcp/dist-ext/context-capsule.mcpb` inside it (build it first with
`mcp/extension/build.sh`; `build.rs` embeds it), and both build scripts also copy it next to the
exe. On a Mac, `./cross-build.sh` also makes the release file
`target-xwin/Context-Capsule-<version>-windows.exe` and prints its size and SHA-256 (for
`latest.json`). Started from a terminal, it prints its log lines there; the
same lines go to `app.log` in the data folder (counts, reasons and error codes only, capped at
256 KB). Data folder: `CONTEXT_CAPSULE_HOME`
if set, else `%USERPROFILE%\.context-capsule` (the same rule as the MCP server).

## Tests

The pure rules (which window or screen was picked, UI Automation roles, window classes for
toasts and overviews, 16 kHz resampling, model checks, menu text, toast placement, the window's
layout at any size) run on any OS:

```sh
cd capture-rs && cargo test -p context-capsule-windows
```

## Files

| file | does |
|---|---|
| `src/main.rs` | entry point (install / uninstall / finish an update, then the app) |
| `src/install.rs` | install without admin, uninstall, the embedded extension, the exe swap for updates |
| `src/update.rs` | update checks, "ready, restart to update", restart into the new exe |
| `src/net.rs` | https downloads through WinINet (the speech model, updates) |
| `src/app.rs` | the UI thread: hidden owner window, tray menu, cues, alerts; starts the engine thread |
| `src/main_window.rs` | the Context Capsule window: messages, scrolling, keyboard, actions, text boxes |
| `src/layout.rs` | what goes where in the window (pure, tested on any OS) |
| `src/paint.rs` | drawing: Windows 11 colors, fonts, GDI text and GDI+ shapes |
| `src/setup.rs` | setting up: voice notes, Connect Claude, open at login (the Settings rows) |
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
