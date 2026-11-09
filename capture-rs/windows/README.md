# Context Capsule — the Windows recorder

A small tray app for Windows 10 (2004+) and 11. It drives the shared engine in
[`../core`](../core) and writes exactly the same files as the macOS app (see
[`../../SPEC.md`](../../SPEC.md)), so the MCP server works with it unchanged.

Press **Alt+C**: the Windows capture picker opens. Pick a window or a screen and
recording starts; close the picker to cancel. Press Alt+C again to stop.

**All my screens** (Settings → Every day → When you start, `"share_screens": "all"` in config.json):
Alt+C shares every monitor at once (up to 4), with no picker, through Windows.Graphics.Capture
(`IGraphicsCaptureItemInterop::CreateForMonitor`, Windows 10 1903+; no permission is asked). It is the
same recording as picking each screen with Add a screen: the yellow border goes around each screen, the
window you type in is read on whichever screen holds it, and "Add a screen" is off while every screen is
shared. A monitor that can't be captured that way is left out; if none can, the picker opens instead,
and `app.log` says why (`all screens: … the picker instead`). The default is `"pick"` (the picker).

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

Tray icon: the Karya panther = idle (white on a dark taskbar, black on a light one), **red** filled circle = recording (the running time is in its
tooltip and the menu's first line: the tray can't show text), **red** microphone with bars that
move with your voice = listening. Nothing appears on screen: soft sounds mark start and stop, and
the mic opening and closing (only problems are shown as text) (`"sounds": false` in config.json turns them off). The menu shows
the time, the event count and, when a sample is skipped, why. If a recording ends with nothing
written, a message says why. Menu: Start / Stop recording, Add / Finish voice note, Add another
screen… (while recording: the picker again, up to 4 screens or windows; each sample uses the
one holding the window you type in; picking one that's already shared says "That screen is
already shared. Pick a different one."), Project (Auto-detect, your projects, New project from this
window… — works without a recording too), Open Context Capsule, Make today's capsules (with Your AI; stops
contextualizing first unless a voice note is listening), Save from clipboard (only when Your AI is a website or any
other AI: the result shows in a small box), Check for updates, Quit.

Each sample saves only the lines not already saved for that window in this recording, so long
documents are kept whole as you scroll (no size limit; a 100,000-character safety net).

Evening reminder: at 18:00 (`"remind_at": "19:30"`, or `false` to turn it off), if something was
recorded today and no capsule exists for today, a box asks "Make today's capsules?" once a day.
**Make** makes today's capsules with Your AI (see *Any AI*): Claude opens on a new chat with the request typed
in (press Enter), another app gets the request on the clipboard, a website or any other AI gets Copy my day. Raw recordings older than 30 days are deleted (`"keep_recordings_days"`, 0 = keep);
capsules are never deleted.

## Install, uninstall, updates

The download is one file, `Context-Capsule-<version>-windows.exe`. No admin, no installer.

- **Install**: run it from anywhere (Downloads…) and it asks *Install Context Capsule?* —
  **Install** copies it to `%LOCALAPPDATA%\Programs\Context Capsule\context-capsule.exe`, adds a
  Start menu shortcut *Karya Context Capsule* and an entry in Settings → Apps (under HKCU: the name
  *Karya Context Capsule*, publisher Karya, the version, *Uninstall* runs
  `context-capsule.exe --uninstall`), points open at login at the installed copy,
  then starts it. A copy that is already running is asked to quit first. **Just run it** runs it
  where it is (it asks again the next time you open that file, never at login).
- The Claude extension (`context-capsule.mcpb`) is inside the exe and written next to it when
  missing or changed, so Connect for Claude works from a single download.
- The any-AI program (`context-capsule-mcp.exe`, about 87 MB; see *Any AI*) is inside the exe too,
  zstd-compressed (about 33 MB of the download). At every start a thread writes it next to the exe
  when it's missing or different (checked by size and SHA-256; a running copy, which a connected AI
  app may have open, is moved aside to `.old` first, as updates do with the main exe). The AI apps'
  settings point at that path (`%LOCALAPPDATA%\Programs\Context Capsule\context-capsule-mcp.exe` when
  installed), so it stays the same across updates.
- **The name and icon**: people see *Karya Context Capsule* (window title, taskbar, Start menu,
  Settings → Apps, dialog titles, the tray's tooltip, Task Manager) and the Karya panther (`icon.ico`,
  made by `../icons/make-icons.mjs`; build.rs puts it and the version info in the exe through
  `src/res.rs`). In sentences it is *Context Capsule*. The exe, install folder, registry keys, data
  folder and release files keep their names, so updates find everything where it was. Up to 0.2.0
  the shortcut was *Context Capsule.lnk*: the updated app renames it at its first start (a new one,
  then the old one goes), so the Start menu never shows both; the Settings → Apps entry is the same
  key, renamed.
- **Uninstall** (Settings → Apps → Karya Context Capsule → Uninstall): the running copy quits; the
  shortcut (either name), the Settings → Apps entry, open at login, our registry key and the
  program folder go.
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
- **Open with**: Install (and every start of the installed copy, when something changed) adds
  Context Capsule to the *Open with* list of `.md` files, never as the default app: the ProgID
  `HKCU\Software\Classes\ContextCapsule.Capsule` (`shell\open\command` = `"<exe>" --open "%1"`),
  listed under `.md\OpenWithProgids`, and `Applications\context-capsule.exe` (name, `.md`).
  Uninstall removes them.

## Opening a capsule someone sent

Right-click a capsule file → **Open with** → **Context Capsule** (or **Open a capsule…** on the
Capsules page). It checks the file looks like a capsule (front matter with `capsule:` and
`title:`), then your AI carries on from it: with Claude (also when Your AI is a website or any other
AI, as on the Mac), Claude opens on a new chat with *Use Context Capsule to continue from the capsule
at <full path>.* typed in (no Claude link registered: on the clipboard, and the app opens); with
another connected app, that sentence goes on the clipboard (*Copied. Open Cursor and paste it in a new
chat.*); an app not connected yet asks to connect it first. Not a capsule: *Not a capsule*, nothing opened. When Context Capsule already runs, the new copy hands
the path to it (`WM_COPYDATA` to the tray window) and quits; else it starts as usual and opens it.

## The Context Capsule window

Everything in one place, like the Mac's window: tray menu → **Open Context Capsule**. The first
launch opens it by itself on **Settings**. It is the same page as the Mac's and Linux's (`ui/` at the
repo root: Home, Timeline, Capsules, Projects, Settings, Help), shown in WebView2.

- **WebView2.** The page is shown in WebView2 (through `wry`), served from inside the exe at
  `https://capsule.localhost/` (no network, no files on disk); its bridge is `core::bridge` (ui/BRIDGE.md), answered
  off the UI thread (`web_window.rs`). WebView2's own files go to `%LOCALAPPDATA%\Context Capsule\WebView2`.
- **No WebView2 runtime** (possible on Windows 10 without a recent Edge), or WebView2 failing: a plain message says the
  window needs Microsoft Edge WebView2, with Microsoft's download link and a **Download** button (app.log says why).
  Recording and the tray menu work without it. Windows 11 always has it.
- **Glass.** On Windows 11 22H2 and later the window has Mica behind a transparent page, and the page is told
  `material: "glass"`; with Settings → Personalization → Colors → Transparency effects off it is told `"none"` (and
  again live when that changes). Windows 10: flat.
- Light or dark follows Windows (or the Appearance setting); popup menus follow it too.
- The title bar and its buttons, the tray, hotkeys, the picker and the dialogs stay Windows'.
- For testing, `--show <page> [YYYY-MM-DD] [<recording id>]` opens the window at launch on that page (the Timeline
  on that day, with that recording open).

## Any AI

Context Capsule works with any AI (SPEC *Any AI*), through `context-capsule-mcp.exe` (inside the
exe, written next to it; see *Install*), run by `src/ai.rs` off the UI thread with a time limit and
no console window (core `anyai`):

- **The list** (`clients`): asked for at start, when Settings opens and every 10 seconds while it's
  open (never two at once). Claude stays as it was: its row uses the app's own detection (the ✓
  shows as soon as the extension is installed) and **Connect** opens the `.mcpb` (see *Finding
  Claude*). Other apps (Cursor, VS Code, Windsurf, Codex / ChatGPT, LM Studio, Gemini CLI…):
  **Connect** runs `connect <id>`, which adds a `context-capsule` entry (`command` = the path of
  `context-capsule-mcp.exe`, `args` = `["serve"]`) to that app's MCP settings; restart the app to use
  it. The first app connected becomes Your AI when none was chosen. **Disconnect** runs `disconnect
  <id>` (Claude: *Remove it in Claude*, Settings → Extensions). Without the program the list is just
  Claude, as before, and the rest says *Part of Context Capsule is missing. Please download Context
  Capsule again.*
- **Your AI** (config.json `ai`): what Make today's capsules uses — Claude (the request typed in),
  another app (*Connect Cursor first* when it isn't; else the request on the clipboard: *Copied. Open
  Cursor and paste it in a new chat.*), or a website / any other AI: **Copy my day**.
- **Copy my day** runs `day`, copies one paste-ready message (≤ 60,000 characters), opens the website
  when Your AI is ChatGPT (website) or Gemini (website), and says *Paste it into your AI. When it
  answers, copy its whole reply, then click Save from clipboard.* (Home and Capsules keep that line
  until a save works or Your AI changes). Nothing recorded today: *Nothing recorded today yet.*
- **Save from clipboard** reads the clipboard's text (empty: *The clipboard is empty*) and runs
  `save`: a box says *Saved 2 capsules* with a ✓ line each, and *Not saved:* with the reason for any
  refused; the Capsules page and Home's numbers update.
- `app.log` says `connect: <id>`, `copy my day: N characters copied`, `save from clipboard: N saved,
  M refused`, `your AI: <id>`, never the day's text or the reply.

## Hotkeys

| keys | does |
|---|---|
| Alt+C | start (opens the picker, or shares all your screens) / cancel the picker / stop recording (waits while a voice note listens) |
| Alt+V | start / finish a voice note (only while recording) |

The same keys as the Mac's ⌥C / ⌥V, unless you change them. If another app already uses one at
launch, Context Capsule logs it, says so once, and the tray menu still works.

**Shortcuts** (Settings → Shortcuts, config.json `"hotkeys": {"record": "Alt+C", "voice": "Alt+V"}`,
core `shortcuts`): "Start or stop" and "Speak a note", each a button showing its keys. Click it
(it says *Press keys…*; the shortcuts in use are off meanwhile), press the new keys: Ctrl / Alt /
Shift and a letter, a digit or F1–F12 (Esc cancels; clicking elsewhere or leaving the window
too). It's checked, registered with RegisterHotKey and saved at once. Refused, with a line under
the row, and the old one stays: the other shortcut's keys (*Already used for Speak a note*), no
Ctrl or Alt (*Add Ctrl or Alt*; F-keys may stand alone), or keys another app has registered
(*Another app uses these keys*). **Reset** puts back Alt+C and Alt+V. The Win key is not offered
(Windows keeps it). Every place that names a shortcut (Home, Help, the tray menu and tooltip,
messages) shows the current one.

## Finding Claude

Claude may be a classic install, a packaged (MSIX / Store) app, or only reachable through its
`claude:` link or app alias, so Connect for Claude, and Make today's capsules, Ask Claude and opening a
received capsule when they go to Claude, look for every sign of it (`connect::find`), and write what they found to
`app.log` (a line starting with `claude app:`):

- `.mcpb` files open with Claude (the file association: exe, app name, ProgID or packaged app id);
- the `claude:` link is registered (HKCU or HKLM `Software\Classes\claude`);
- `claude.exe`: `%LOCALAPPDATA%\AnthropicClaude`, `%LOCALAPPDATA%\Programs\Claude`, `%ProgramFiles%\Claude`,
  App Paths, or an *Add or remove programs* entry by Anthropic (its icon or folder);
- the app alias `%LOCALAPPDATA%\Microsoft\WindowsApps\claude.exe`;
- a packaged app named like Claude (HKCU `…\AppModel\Repository\Packages`, or a folder in
  `%LOCALAPPDATA%\Packages`), started through `shell:AppsFolder\<family>!<app>`;
- Claude's data folder (it has run here).

**Connect**: `.mcpb` opens with Claude → open it (Claude asks to install). Else `claude.exe` found →
run it with the file. Else Claude found another way → open Claude (link, alias or package), show
`context-capsule.mcpb` selected in File Explorer, and say: *In Claude: Settings → Extensions →
Advanced settings → Install Extension…, then pick context-capsule.mcpb* (Claude Desktop's own menu
path). Nothing found → it doesn't assume Claude is missing: *Is the Claude app installed?* Yes → the
same steps; No → the download page. The other requests open Claude on a new chat with the request
typed in (the link), else start the app with the request on the clipboard; nothing found → the same
question (Yes: the request is copied, paste it in Claude).

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

- **Screen capture** — none needed. The picker is the consent: only what you pick is captured. With
  **All my screens**, choosing that setting is the consent: every monitor is captured, with the yellow
  border around each.
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
exe. It also carries `mcp/dist-bin/context-capsule-mcp-windows-x64.exe` (run
`mcp/build-binary.sh windows-x64` first): `build.rs` compresses it with zstd (level 19 in release,
about 30 s, only when it changed) and embeds it with its SHA-256 and size. A release build stops with
an error when either file is missing (the build scripts check first). Debug builds and tests embed
nothing: developing, the app finds the repo's `mcp/dist-bin` binary, or `mcp/dist/index.js` with `node`. On a Mac, `./cross-build.sh` also makes the release file
`target-xwin/Context-Capsule-<version>-windows.exe` and prints its size and SHA-256 (for
`latest.json`). Started from a terminal, it prints its log lines there; the
same lines go to `app.log` in the data folder (counts, reasons and error codes only, capped at
256 KB). Data folder: `CONTEXT_CAPSULE_HOME`
if set, else `%USERPROFILE%\.context-capsule` (the same rule as the MCP server).

## Tests

The pure rules (which window or screen was picked, UI Automation roles, window classes for
toasts and overviews, 16 kHz resampling, model checks, menu text, toast placement, the window's
small rules (glass, region settings, `--show`, which pages it may open), finding Claude, the any-AI state, writing out
the any-AI program, shortcut keys and the Open with
registry values) run on any OS. The Claude links are tested in core (`cargo test -p
context-capsule-core`):

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
| `src/web_window.rs` | the Context Capsule window: `ui/` in WebView2, its bridge host, Mica, light / dark, the WebView2 message |
| `src/setup.rs` | setting up: voice notes, Connect your AI, open at login (Settings → Set up) |
| `src/connect.rs` | finding the Claude app, Connect for Claude: the extension next to the exe, connected or not |
| `src/ai.rs` | any AI: the AI apps, Your AI, Connect, Make today's capsules, Copy my day, Save from clipboard |
| `src/program.rs` | the any-AI program: inside the exe (zstd), written next to it, found (or the repo's, developing) |
| `src/platform.rs` | the core's `Platform` for Windows |
| `src/picker.rs` | `GraphicsCapturePicker`, which item was picked, every monitor at once (All my screens), the captures and their latest frames |
| `src/windows_list.rs` | front app, on-screen windows (EnumWindows + DWM), monitors, toast / overview rules |
| `src/uia.rs` | UI Automation text |
| `src/ocr.rs` | Windows.Media.Ocr |
| `src/voice.rs` | microphone (cpal) + whisper.cpp voice notes |
| `src/model.rs` | the speech model: path, SHA-256 check, consented download |
| `src/tray.rs` | tray icon (drawn in code: at 16–24 px the panther is a blur, and the tray shows the state) and menu text |
| `src/res.rs` | the exe's icon and version info, as a .res build.rs writes (no resource compiler) |
| `src/toast.rs` | fading messages near the tray |
| `src/hotkeys.rs` | global shortcuts: registered, changed live, off while new keys are recorded |
| `src/pointer.rs` | the mouse while recording: position and last move or scroll |
| `src/changes.rs` | when the front app or virtual desktop last changed |
| `src/win.rs` | small Win32 helpers (registry, dialogs, opening things, the clipboard) |
