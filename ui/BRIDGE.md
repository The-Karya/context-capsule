# The window's bridge: how the page talks to its host

The Context Capsule window is one web page (`ui/`) shown in each platform's system webview: WKWebView on the Mac,
WebView2 on Windows, WebKitGTK on Linux. The **host** (the native app) gives the page raw files and does what only
it can do (start recording, the clipboard, opening apps, native pickers). Everything people see — stretches,
durations, the day bar, search, dates and times, every word — is worked out once, in `ui/js/`.

The menu bar / tray, the screen picker, hotkeys, voice notes, file pickers and the update installer stay native.
Alerts and confirms *inside the window* (Delete…, Save from clipboard's result) are drawn by
the page (`role="alertdialog"`), so they look the same everywhere.

## Shape

```js
// page → host: one function, one JSON argument, a Promise of JSON.
const answer = await window.capsuleHost.call("data.read", { files: ["config.json"] });

// host → page: events.
window.capsuleHost.on("state", (state) => { … });
```

The host injects a tiny script before the page loads that defines `window.capsuleHost` with `call(method, args)`
and `on(event, fn)`, and calls `window.capsuleHost._emit(event, payload)` to send events. Transport per host:

| host | page → host | host → page |
|---|---|---|
| Mac (WKWebView) | `webkit.messageHandlers.capsule.postMessage({id, method, args})` (a `WKScriptMessageHandlerWithReply` can answer directly) | `evaluateJavaScript("capsuleHost._reply(id, json)")` / `_emit` |
| Windows (WebView2) | `chrome.webview.postMessage({id, method, args})` | `PostWebMessageAsJson({id, ok, value})` / `{event, payload}` |
| Linux (WebKitGTK) | `webkit.messageHandlers.capsule.postMessage(...)` (`webkit_user_content_manager_register_script_message_handler`) | `webkit_web_view_evaluate_javascript` |

A call answers `{ "ok": true, "value": <JSON> }` or `{ "ok": false, "error": "<plain words>" }`; the shim turns the
second into a rejected Promise. Methods never block the UI thread: file reads and program runs happen off it.

Pages are served from a local scheme (`capsule://app/index.html` on Mac/Linux, `https://capsule.local/` mapped to
the folder on WebView2) — never from the network. No CDN, no remote content; the page sets a strict CSP.

All paths in the bridge are **relative to the data folder** (`CONTEXT_CAPSULE_HOME`, else `~/.context-capsule`, else
`$XDG_DATA_HOME/context-capsule` on Linux). The host refuses absolute paths, `..`, and anything outside the
allow-list below.

## Methods

25 methods. "Has code" says where each host already does this today (Swift `capture/Sources`, Rust
`capture-rs/core` = shared crate, `windows/`, `linux/`).

### Starting up

**`host.hello`** `{}` →
```json
{ "platform": "mac" | "windows" | "linux",
  "version": "1.4.2",
  "locale": "en-GB",            // BCP 47, the system's
  "hour12": false,               // the system's 12/24-hour choice; null = the locale's default
  "firstWeekday": 2,             // 1 = Sunday … 7 = Saturday; null = from the locale
  "userName": "Saachi Badal",    // the account's full name (fallback for config "author")
  "dark": false,                 // the system appearance right now
  "material": "glass",           // "glass" | "none" (see below); missing = "none"
  "firstRun": true,              // open on Settings with nothing set up (the old welcome)
  "page": "home",                // the page to show first (tray "Open Settings…" → "settings")
  "voiceLanguages": [ {"id": "en-US", "name": "English (United States)"} ],
  "voiceLanguageDefault": "Same as my Mac" }
```
**`material`**: `"glass"` = the host shows the system material behind a transparent webview (Mac: an
`NSVisualEffectView` under a WKWebView that doesn't draw its background; Windows 11: Mica with a transparent WebView2
background). The page then sets `<html data-material="glass">` and paints the window, sidebar and cards as tints of
its usual colours over it (menus, alerts, fields and buttons stay solid; text keeps full contrast). `"none"` = flat
fills, exactly as without the field: Linux, Windows 10, and whenever the system asks to reduce transparency (Mac
`accessibilityDisplayShouldReduceTransparency`, Windows "Transparency effects" off). When that setting changes the
host sends the `material` event.

Has code: Swift `Updater.currentVersion`, `NSFullUserName`, `MainWindow.seen`, `Preferences.languages`; Rust
`settings::language_options`, `SAME_LANGUAGE`, `update::CURRENT`.

### Reading files (raw)

**`data.list`** `{ "dir": "sessions" | "capsules" }` → `["20261023-112957", …]` — the names in that folder
(sessions: folders; capsules: files, `.history` left out). Sorted.

**`data.read`** `{ "files": ["config.json", "sessions/20261023-112957/events.jsonl", …] }` →
`{ "<path>": "<file text>" | null }` (null = missing). Allowed: `config.json`, `projects.json`, `state.json`,
`sessions/<id>/events.jsonl|gaps.jsonl|assign.json|session.json`, `capsules/<name>.md`. Text is UTF-8 as on
disk, CRLF untouched. A day is usually well under 5 MB; the page asks only for the sessions it needs
(`timeline.candidates`, `stats.weekCandidates`).

Has code: trivially (read a file). Both hosts already read every one of these.

### Changing things

**`config.set`** `{ "key": "appearance", "value": "dark" }` → `{}` — merge one key into config.json (`null`
removes it), keeping other keys and their order; then apply it at once where the host must: `appearance` (window
look), `share_screens` (Mac: ask for Screen Recording), `auto_update`, `remind_at`, `keep_recordings_days`,
`sounds`, `voice_locale`, `exclude_apps`, `author`, `ai`, `make_capsules` (re-check Automatic capsules at once),
`capsules_view` (the Capsules page's By day / By project: `"project"`, or missing for By day; nothing to apply).
Has code: Swift `ConfigFile.set`, `Preferences.save*`; Rust `settings::ConfigFile::set`.

**`projects.save`** `{ "text": "<projects.json>" }` → `{}` — write projects.json (atomically) and tell the
recorder (detection uses it). The page makes the text (`projects.toJSON`).
Has code: Swift `Projects.save` + `onProjectsChanged`; Rust `projects::save`, `settings::save_project_edit`.

**`timeline.delete`** `{ "session": "20261023-112957", "lines": [ { "t": "2026-10-23T06:59:57Z", "line": "<the exact line>" } ] }`
→ `{}` or error. Without `lines`: delete the whole session folder. With `lines`: remove, for each, the first line of
events.jsonl that is exactly `line` and whose `t` is `t`; all or none, one atomic rewrite. Refuse while that session
is recording. Errors are `"recording"`, `"gone"` (a line or the folder isn't there) or plain words.
Has code: Swift `Timeline.deleteMoments`/`deleteSession`; Rust `timeline::delete_read`/`delete_stretch`/`delete_session`
(both match by byte offset today; matching by exact line is a small change).

### Recording

**`recorder`** `{ "action": "toggle" | "voice" | "add-screen" | "new-project" | "project", "id": "<project id>" | null }`
→ `{}` — the same as the tray/menu items: start (opens the picker) / stop / cancel the picker; speak a note;
add a screen; "New project from this window…"; pin a project (`id`) or back to Auto (`id: null`).
Has code: all hosts (`AppModel.toggleRecord`, `toggleVoice`, `addScreen`, `newProject`, `pickProject`; Rust tray actions).

### Shortcuts

**`shortcuts.set`** `{ "record": "Alt+C", "voice": "Alt+V" }` → `{}` or error `"taken"` (another app has the keys)
— register both and save `hotkeys` in config.json; on error keep the old ones. The page checks the rest
(`shortcuts.check`) first. Reset = set the defaults.

**`shortcuts.listen`** `{ "on": true }` → `{}` — while Settings waits for new keys, the global hotkeys are off.
Has code: Swift `AppModel.setShortcut`/`pauseHotKeys`/`resetShortcuts`; Rust `shortcuts.rs`, windows/linux `hotkeys.rs`.

### Set up

**`setup.status`** `{}` → `{ "steps": [ { "id": "read", "done": true }, … ], "screenRecording": true }` — the steps
this platform has, in order. Ids: `read` (Mac: Accessibility), `voice` (Mac: mic + speech; Win/Linux: the voice
model), `dictation` (Mac only), `ai` (any AI app connected), `login` (open at login). The words for each step live
in the page. `screenRecording`: Mac only (for "All my screens"). The page asks every second while Settings shows.

**`setup.do`** `{ "id": "read" | "voice" | "dictation" | "ai" | "login" | "screen-recording" }` → `{}` — the step's
button (ask for the permission, open the right system settings pane, turn on open-at-login, download the voice
model…). `ai` = connect Claude: install the Claude extension (.mcpb), or say how to get the Claude app when it
isn't there (Swift `ClaudeConnect.connect`, Rust `connect.rs`). Other AI apps connect through `mcp.run connect`.
Has code: Swift `SetupStatus`, `Setup.*`; Rust windows `setup.rs`, linux `autostart.rs`, `window.rs` rows.

### Your AI

**`mcp.run`** `{ "command": "clients" | "connect" | "disconnect" | "day" | "save", "id"?: "cursor", "date"?: "2026-10-23", "input"?: "<clipboard text>" }`
→ the program's stdout as a string (JSON; the page reads it with `yourai.parse*`), or error `"missing"` / `"timeout"` / plain words (the program's last stderr
line). Runs `context-capsule-mcp <command> [id] [--date D]` with `input` on stdin; time limits 10 s (clients),
60 s (day), 30 s (save), 20 s (others). The page reads the answers (`yourai.parse*`).
Has code: Swift `AIBridge.run`; Rust `anyai.rs` + windows/linux `program.rs`, `ai.rs`.

**`ai.open`** `{ "id": "claude" | "<client id>", "prompt": "Use Context Capsule to …" }` → `{ "opened": true }` —
put `prompt` on the clipboard and open that AI app on a new chat (Claude: `claude://claude.ai/new`). `opened:false`
when the app can't be found. The page decides first whether it is installed / connected and what to say.
Has code: Swift `Claude.open`, `AIState.open`, `appURL`; Rust windows/linux `link.rs`, `connect.rs`.

### Automatic capsules (SPEC "Automatic capsules")

**`auto.status`** `{}` →
```json
{ "mode": "auto" | "click",             // config make_capsules (missing = "click")
  "at": "18:00",                         // config remind_at, "18:00" when missing or false
  "agent": { "id": "claude-code", "name": "Claude Code" } | null,   // the tool a run would use; null = none here
  "running": false,                      // a run is going (this app's, or one in auto.json whose pid is alive)
  "autoDay": "2026-10-23" | null,        // auto.json auto_day
  "last": { "day", "by", "status", "ai", "started_at", "finished_at", "capsules", "reason" } | null }  // auto.json last
```
The host finds the tools with `context-capsule-mcp agents` (at launch, then at most every 5 minutes, and after
`config.set ai`). **`auto.run`** `{}` → `{ "started": true }`, or error `"none"` (no tool: the page uses Your AI as
before), `"nothing"` (nothing recorded today), `"running"` — starts `context-capsule-mcp make --by click` in the
background. The host also starts `make --by auto` by itself (SPEC "When"), shows the notification when a run ends,
and sends the **`auto`** event (the same object as `auto.status`) when a run starts or ends or the tools change, then
`data-changed` `capsules`.
Has code: Swift `AutoRun`, Rust `core::auto` + windows/linux `auto.rs`.

### Clipboard, links, files

**`clipboard.read`** `{}` → `{ "text": "…" }`  ·  **`clipboard.write`** `{ "text": "…" }` → `{}`

**`open.url`** `{ "url": "https://chatgpt.com" }` → `{}` — http(s) only, in the default browser.

**`capsule.open`** `{ "file": "capsules/website.capsule.md" }` → `{}` — open in a text editor, never this app (on the
Mac this app is the default for .capsule.md and would send it to your AI). The page doesn't use it now: a capsule row
opens the reading view (`#capsule`).

**`capsule.share`** `{ "file": "capsules/…", "how": "copy-file" | "copy-text" | "save-copy" | "reveal" | "more", "text"?: "<cleaned copy>" }`
→ `{ "done": true, "where"?: "Desktop" }` — with `text`, share a copy with that text and the same file name (made on the share page); without, the file itself. `save-copy` shows the native save panel
(`done:false` if cancelled); `reveal` = Show in Finder / Explorer / Files; `more` = the system share sheet (Mac) —
other hosts may leave it out (`hello.platform` tells the page).
Has code: Swift `Share.*`; Rust windows/linux share code in `main_window.rs` / `window/`.

**`capsule.pick`** `{}` → `{ "path": "/Users/…/Pricing.capsule.md", "text": "…" }` or `null` — the native open
panel for a received capsule (.md). The page checks `isCapsule` and shows it in the reading view, with "Continue in
<your AI>" (the continue prompt).

**`apps.pick`** `{}` → `{ "id": "com.example.app", "name": "Example" }` or `null` — choose an app never to record
(Mac: /Applications; Windows: an .exe; Linux: a .desktop app).
**`apps.names`** `{ "ids": ["com.tinyspeck.slackmacgap"] }` → `{ "<id>": "Slack" }` — names for the Never record list.
**`apps.icons`** `{ "apps": [{ "app": "Safari", "bundle": "com.apple.Safari" }] }` →
`{ "<bundle, else app>": "data:image/png;base64,…" | null }` — each app's own icon, 64 × 64 px (drawn at 32 pt @2×),
for the Timeline's stretches and the Your AI rows. `app` and `bundle` are as events store them (`bundle`: Mac bundle
id, Windows exe name like `chrome.exe`, Linux desktop id or exe name). null: not found — the page keeps its glyph in
the same box. At most 200 apps a call; the host looks each app up once and keeps the answer (found or not) while it
runs; the page asks for the apps it draws in one call and keeps answers too (`ui/js/core/appicons.js`).
Mac: NSWorkspace by bundle id, else a running app or `<app>.app` in the Applications folders. Windows: the exe of a
running process, else App Paths, else the Windows folders; the shell's icon. Linux: the .desktop file by desktop id,
StartupWMClass, Exec or name; its Icon= from the GTK icon theme.
Has code: Swift `AppIconPNG`; Rust `core::app_icons` (cache, PNG), `windows/app_icon.rs`, `linux/icon_match.rs` +
`window/web.rs`.

### Updates

**`updates`** `{ "action": "check" | "restart" }` → `{}` — progress comes in `state.update`.
Has code: Swift `Updater`; Rust `update.rs`.

## Events (host → page)

**`state`** — whenever it changes (and once after `host.hello`): state.json plus what the host knows.
```json
{ "recording": false, "picking": false, "listening": false,
  "started_at": "2026-10-23T06:59:57Z", "session_id": null, "events": 0,
  "project": null, "project_by": null,
  "screens": 1, "can_add_screen": false, "several_screens": true,
  "update": { "line": "", "ready": false, "working": false } }
```

**`appearance`** `{ "dark": true }` — the system appearance changed. (The page combines it with config
`appearance`: `light` / `dark` win over the system.)

**`data-changed`** `{ "what": ["sessions" | "capsules" | "projects" | "config" | "clients"] }` — files changed
(a capsule was saved, the MCP made a project, a session ended…). While recording, the host sends `sessions` at most
every 4 s; the page re-reads only what it shows.

**`material`** `{ "material": "glass" | "none" }` — reduce transparency was switched on or off (see `host.hello`).

**`auto`** — Automatic capsules changed (see `auto.status`).

**`navigate`** `{ "page": "home" | "timeline" | "capsules" | "projects" | "settings" | "help" }` — the tray asked
for a page.

**`open-capsule`** `{ "path": "…", "text": "…" }` — a file was dropped on the window or opened with the app; the
page checks it and shows it in the reading view; "Continue in <your AI>" there opens your AI (SPEC "Opening a
received capsule").

## What lives in the page (`ui/js/core`, tested in `ui/tests`)

| module | from |
|---|---|
| `timeline.js` — stretches, gaps, notes, blocks, search, day bar, where your time went, delete words | Swift `Timeline` |
| `stats.js` — Today / This week totals, streak, greeting, durations | Swift `WorkStats`, Rust `core::stats` |
| `dates.js` — long day, clock times, month calendar (Intl + host locale, 12/24 h) | Swift `DayWords` |
| `capsules.js` — front matter, In short, groups, row labels, prompts | Swift `CapsuleSummary`, Rust `core::capsules` |
| `projects.js` — projects.json, slug, detect, edit | Swift `Projects`, Rust `core::projects` |
| `shortcuts.js` — parse, show (⌥C / Alt+C), key events, checks | Swift `Shortcut`, Rust `core::shortcuts` |
| `yourai.js` — Your AI choice, clients, Copy my day / Save words | Swift `YourAI`/`AIAnswer`, Rust `core::anyai` |
| `sharecheck.js` — the share page: names, emails, phones, private links, money, companies, file paths, public links; where each is; the copy | only here (spec/share-check.json) |
| `markdown.js` — the reading view's Markdown (blocks, inline; no HTML) | — |

## Dev harness

`bun ui/dev/server.js [data folder]` serves the page at http://localhost:5174 with a fake host (`ui/dev/shim.js` +
the server) that answers every method above from a data folder (default `/tmp/mac-ref-data`); writes stay in memory.
URL options: `#home` … `#help`, `?capsule=capsules/<file>` (the reading view), `?share=capsules/<file>` (the share page), `?open=first` (open the first recording), `?dark=1`, `?firstRun=1`,
`?platform=windows|linux`, `?material=glass`, `?locale=en-GB`, `?recording=1`, `?now=2026-10-23T19:00:00` (the page's clock).

Renders: `ui/dev/render-all.sh` (WKWebView, `ui/dev/snap.swift`) writes every page in light and dark to
`/tmp/shared-ui/<page>-<mode>.png` (860 × 620 @2×, like the Mac's `--render-page`), plus `full/` (860 × 1700),
`narrow/` (640 × 620) and `large/` (1280 × 900); `ui/dev/compare.sh` puts each next to the Mac's render in
`/tmp/shared-ui/compare/`. `snap <url> <out> <w> <h> <light|dark> [script]` runs a script first (open a menu…).
The page sets `document.documentElement.dataset.ready = "1"` once its first view is drawn.

Tests: `cd ui && bun test`.
