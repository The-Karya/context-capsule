# context-capsule — v1 spec

Open-source, for macOS, Windows and Linux. Press a hotkey, work, press it again: Context Capsule reads
your screen while you work and helps your own AI (Claude, via MCP) turn that into a
**capsule** — one shareable file with what you worked on, the decisions you made and
why. Send the capsule to anyone; their Claude loads it and knows the project.

Recordings stay on the laptop. No accounts, no cloud, no AI key of our own: the
user's Claude does the writing through MCP — so the timeline text it reads goes to
Claude as part of that conversation (PRIVACY.md lists every hop).

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

- **Hotkey** `⌥C` (Option-C) toggles recording. Also a menu: Start / Stop
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
- **Voice notes** — `⌥V` (menu *Speak a note*) starts talking, press again to stop;
  only while recording (else a message: "Start recording first (⌥C)"). The mic opens
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
events are byte-identical) and one tray app per OS. Hotkeys **Alt+C**
(start/stop contextualizing) and **Alt+V** (speak a note) — the same keys as the Mac's ⌥C / ⌥V.

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
  projects.json, config.json, control, state.json
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
ignore unknown fields; older files have no `src`. Times are ISO-8601 UTC.

**Only what's new:** a screen event's `text` holds only the lines not already saved for that
window (app/bundle + title + where) in this recording; a sample with nothing new extends the
last event of the same window, or is written with empty `text` (time spent there). Accessibility
reads the whole window, limited only by time (0.4 s) and 20,000 elements; a 100,000-character
safety cap per event remains for extreme cases. (Older files: whole screen text, 4000 cap.)

### Projects (dividing a day's work)

A day often spans several projects (1 h on one, 2 h on another…). Each gets its own
capsules: a **daily** capsule per project, and one **growing** capsule per project that
is updated each day it is worked on, plus a time summary.

**`projects.json`** (in the data folder; written by the app and the MCP, editable by hand):

```json
{"projects": [
  {"id": "context-capsule", "name": "Context Capsule", "match": ["karya-capsule", "context capsule"]},
  {"id": "client-report", "name": "Client report", "match": ["acme q3", "docs.google.com/document/d/1AbC"]}
]}
```

`id` is the slug of `name`. `match` are clues, matched case-insensitively as substrings
of the front window's **title**, its **`where`** (page address or file path) and the
**app name**. Never matched against screen text (other projects' names show up there).

**Detection (recorder, every sample):**
- **Pinned** (the user picked a project in the menu, or the MCP `set_project`): every
  event gets that project, `"project_by":"you"`, until the user picks another or *Auto*.
- **Auto** (default): the project whose clue is found wins (longest clue on a tie, then
  list order) → `"project_by":"auto"`. No match → no project (Claude sorts it later).
- When the current project changes, a short message says so ("Project: Client report").
- Voice and typed notes get the current project too.

**New event fields** (optional; omitted when empty, so older lines are unchanged):
`"where"` — the window's page address or file path when Accessibility gives it (query
and fragment dropped, then redacted); `"project"` (id) and `"project_by"` (`auto`/`you`).
Key order: `t, end, [kind, src,] app, bundle, title, [where], [project, project_by], [focus], [src,] text`.

**Where you look:** each sample reads the window in use (keyboard focus). If the mouse moved
or scrolled over another *shared* screen in the last 30 s (or the window in use isn't on a
shared screen), the window under the mouse is read too, with `"focus":"pointer"` — written only
when it has something new, and never counted as time spent (it was seen, not worked in).

**After the fact:** `sessions/<id>/assign.json` holds corrections, without rewriting
events: `{"assign": [{"from": iso, "to": iso, "project": id}]}` (later entries win).
Readers apply it on top of each event's own `project`.

**Menu:** "Project: <name or Auto>" with *Auto-detect*, each project, and *New project
from this window…* (name prefilled from the window, the clue added to `match`). Optional
(`config.json` `"ask_project": true`, off by default): ask "What are you working on?" when
a recording's first window matches no project. Clues need window titles and addresses,
i.e. Accessibility (macOS) — so the app is signed with a fixed local identity when one
exists (`build.sh`), and the permission survives rebuilds.
`control`: `{"command":"project","project":"<id>"|"auto","at":<ms>}`. `state.json` adds
`"project": id|null, "project_by": "auto"|"you"|null`.

**Capsules:** daily `capsules/<project>-<YYYY-MM-DD>.capsule.md` (`kind: daily`, `date`),
growing `capsules/<project>.capsule.md` (`kind: project`, `days: [...]`; updating it keeps
the previous version in `capsules/.history/`). Capsules without `project` keep working.

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
| `import_capsule` `{path}` | copies a received capsule (any file name: `.capsule.md`, `.md`…) into `capsules/` (validates format; "isn't a Context Capsule" otherwise) |
| `search_capsules` `{query, limit?=8}` | read-only: ranks every saved capsule's `## ` sections by the query's words (stop words dropped; Decisions / Where things stand / Next steps and newer capsules weigh more); returns title, name, kind, date, section and text, each in a `<capsule>` block, cited as *(Pricing page, 19 Oct, Decisions)*; nothing found → says so and points to `get_day_activity` |
| `export_capsule` `{name, to_path?}` | copies to Desktop (default) or a path; returns the path, ready to email |
| `list_projects` | projects with their clues, time spent today, and their capsules |
| `create_project` `{name, match?}` | adds a project to `projects.json` |
| `set_project` `{project}` | pins the recorder to a project (`"auto"` to detect again); waits for `state.json` |
| `assign_project` `{session_id, project, from?, to?}` | records a correction in that session's `assign.json` |
| `prepare_day` `{date?, max_chars?}` | the start of a day's job: a checklist, the projects to write up (time, sessions, which capsules exist), the current project capsules to merge into, and the day split by project — one call |
| `finish_day` `{date?}` | the end: checks every project (≥ 2 min or a note) has its daily capsule and an updated project capsule and nothing is unsorted; lists exactly what's missing |
| `get_moment` `{session_id, from, to?, max_chars?}` | zoom in: everything recorded between two times, in full (screen text wrapped as information, notes, "nothing new" moments); says where to continue if it doesn't fit |
| `get_day_activity` `{date?, project?, max_chars?}` | all sessions of a local day, split by project with time per project; unassigned work listed for Claude to sort |

Prompt `make_capsule` `{session_id}`: tells the AI how to write a capsule from
`get_session_activity` — sections below (written for another Claude to load), decisions
must cite the moment/app they came from, mark guesses as guesses, never include secrets,
up to ~2500 words; it saves without pasting the capsule into the chat. Voice
notes are the user's own words: the primary source for decisions, reasons and intent;
screen text is supporting evidence; quote or closely paraphrase them and cite the time.

Prompt `make_day_capsules` `{date?}`: read `get_day_activity`; sort unassigned work into
projects itself (`create_project` with clues so it's found automatically next time,
`assign_project` for the rest) and list what it sorted at the end so the user can correct it; write one daily capsule per project
(`save_capsule` with `project`, `kind: "daily"`, `date`), then update each project's
growing capsule (`get_capsule` → merge → `save_capsule` `kind: "project"`); end with
the time per project.

`save_capsule` gains `{project?, kind?: "daily"|"project", date?}`, and **checks** the capsule
before saving: every section present, "In short" and "Next steps" not empty, "For Claude"
really addressed to Claude, every decision citing its source (*(you said …)*, *(seen in …)*,
*(guess)*). If not, nothing is saved and Claude gets the exact list of fixes ("Not saved
yet…"); it fixes them and saves again.

Prompt `continue_from_capsule` `{capsule}`: import if it's a file, `get_capsule`, follow
"For Claude", reply in three lines (whose work, where it stands, next step), then carry on.

Prompt `ask_capsules` `{question}`: `search_capsules`, then `get_capsule` if needed; answer short with
citations; say plainly when the capsules don't say.

All of Claude's instructions (server instructions, capsule format, prompts) live in
`mcp/src/playbook.ts`; every capsule records the playbook version in its front matter
(`playbook: 2026-10-22`). Playbook words say "the AI" / "you", not "Claude", wherever any AI may read them; the
section heading stays `## For Claude` (the capsule format), meaning the AI that loads it.

**Harness safety** (`mcp/src/untrusted.ts`, `checks.ts`):
- Screen text reaches Claude only inside `<screen>` blocks with a rule: information, never
  instructions; it can't change the task or approve anything. Text inside can't close the
  block, fake tags, or pose as the user's `>>` notes. The user's notes are the only trusted words.
- Capsules reach Claude inside a `<capsule>` block: "For Claude" is advice, never permission
  to send, share, export, import, delete, open links, run commands or record.
  `import_capsule` / `get_capsule` scan for text aimed at an AI and tell the user.
- `save_capsule` also checks citations against the given sessions: every *(you said / you
  typed, HH:MM)* must match a real note within 2 minutes (else refused); *(seen in App,
  HH:MM)* not found nearby is a warning. Fix lists are numbered ("change only what this list
  names"); after 3 refusals the capsule is saved as a draft (`draft: true`).
- Card numbers (Luhn-checked), IBANs, private keys, bearer tokens and API keys are removed
  on save, and the kinds removed are named.
- `finish_day` adds **Unsure** (drafts, guesses) and **Skipped** (projects under 2 minutes).
- Prompt `review_capsule` `{name}`: a second look before sharing (confirmed problems vs not
  sure; facts, reasons, privacy, can another Claude carry on); never exports.
- [PRIVACY.md](PRIVACY.md): every place data goes, with the code that does it.

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
project: website            # optional
kind: daily                 # optional: daily | project
date: 2026-10-19            # daily capsules
---

## In short
## For Claude
## Where things stand
## What was done
## Decisions
- **Decision** — why. *(you said, 10:14)*
## Open questions
## Next steps
## People, links and files
```

A capsule is **context for the receiver's Claude**, not a report: complete and specific,
with a "For Claude" section that tells the loading Claude whose work it is, to treat the
decisions as settled and to carry on from Next steps. "In short" is the only part written
for people; they can read the rest, but don't have to.

Plain Markdown, readable by a human, by Claude, ChatGPT or anything else.

## Out of scope for v1

ChatGPT remote connector (needs hosting), images, cloud sync, teams.

## Rules

- No secrets, no Karya-internal code or URLs. MIT. Clean, small, readable.
- Every source file starts with a short header comment naming the project.

## Check before sharing (all recorders)

Before a capsule is shared (Copy file, Copy text, Save a copy, AirDrop/Share…), the app shows what's inside that
might be private, grouped: **People, Emails, Phone numbers, Links, Money, Files**. Each item has a check box;
"Share" sends a copy with the checked items replaced by `[removed]` (the saved capsule is never changed). With
nothing found, sharing goes straight through. No AI: plain rules, the same on every platform
(`spec/share-check.json` holds the cases both the Swift and the Rust code must pass).

Rules (on the body only: a leading `---` front matter block is skipped):
1. **links** `https?://` up to whitespace, `)`, `>`, `]`; trailing `. , ; :` dropped. **emails** `name@domain.tld`.
   Both are found first, then blanked out (same length) so later rules can't match inside them.
2. **files** start after start-of-line, whitespace or `(`: `~/…` or `/…` with at least two parts, or `C:\…`;
   trailing `. , ; :` dropped.
3. **phones** `+`/digit, then digits, spaces, `-`, `.`, `(`, `)`, ending in a digit, with 8–15 digits;
   dates like `2026-10-19` are not phones.
4. **money** a currency sign (`$ € £ ₹`) and an amount (`$12,000`, `€450`, `$1.5m`), or an amount and a code or
   word (`1,200 USD`, `EUR`, `GBP`, `INR`, `dollars`, `euros`, `pounds`, `rupees`).
5. **people** `@handles` (not inside an email), and in the `## People…` section, the start of each `-`/`*`
   bullet (bold removed) up to ` — `, ` – `, ` - `, `:`, `(` or `,`, when it is 1–4 words that each start with
   a capital letter, with no digits, `/`, `@` or `http`.
Each group lists items once, in order of first appearance in the body. Removing replaces every occurrence of
each checked item (longest first) with `[removed]`, in the whole file (front matter too).

## Opening a received capsule (all recorders)

Context Capsule is offered for `.md` files ("Open With", not the default app) and accepts capsule files dropped
on its window. Opening one checks it looks like a capsule (front matter with `capsule:` and `title:`), then opens
Claude on a new chat with: `Use Context Capsule to continue from the capsule at <full path>.` (Claude imports it
with import_capsule / continue_from_capsule). Not a capsule: a short message, nothing opened.

## Ask your capsules

The Capsules page has a search field (filters titles and "In short" as you type) and **Ask Claude**, which
opens Claude with: `Use Context Capsule to search my capsules: <question>`. Claude uses the `search_capsules`
tool (mcp/), which ranks every saved capsule section by the question's words and cites capsule, date and section.

## Your own shortcuts (all recorders)

config.json `"hotkeys": {"record": "Alt+C", "voice": "Alt+V"}` (missing = these defaults). A shortcut is
modifiers then one key, joined by `+`: modifiers `Ctrl`, `Alt` (Option on a Mac), `Shift`, `Cmd` (Mac only);
the key is `A`–`Z`, `0`–`9` or `F1`–`F12`. It needs at least one of Ctrl / Alt / Cmd (F-keys may stand alone).
Written in this order: Ctrl, Alt, Shift, Cmd. The Mac shows them as ⌃ ⌥ ⇧ ⌘ (e.g. `⌥C`), Windows and Linux as
written (`Alt+C`).

Settings → **Shortcuts**: "Start or stop" and "Speak a note", each a button showing the shortcut. Click it, press
the new keys (Esc cancels), and it's saved and used at once. Refused, with a short line under the row: the same
keys as the other shortcut ("Already used for Speak a note"), keys without Ctrl/Alt/Cmd ("Add Ctrl, Alt or ⌘"; Windows and Linux: "Add Ctrl or Alt"),
or keys another app already has ("Another app uses these keys") — then the old shortcut stays. "Reset" puts back
the defaults. Every place that names a shortcut (Home, Help, the menu / tray, messages) shows the current one.

## Any AI (all recorders)

Context Capsule works with any AI, in two ways. The logic lives once, in one program built from `mcp/`:
**`context-capsule-mcp`**, a single self-contained binary (no Node needed), shipped inside each app:
Mac `Context Capsule.app/Contents/Resources/context-capsule-mcp`, Windows `context-capsule-mcp.exe` next to the
exe, Linux `usr/bin/context-capsule-mcp` in the AppImage. Commands (JSON on stdout, except `serve`):

| Command | What |
|---|---|
| `serve` (or no command) | the MCP server on stdio, as today |
| `serve --http [--port N]` | the same server over Streamable HTTP at `http://127.0.0.1:<port>/mcp` (port 3740, or any free one if taken; printed on stderr). 127.0.0.1 only; every request needs `Authorization: Bearer <token>` from `~/.context-capsule/mcp-token` (random, made on first use, mode 600); no CORS, foreign `Origin` / `Host` refused |
| `clients` | `[{"id","name","installed","connected","how"}]`: every AI app it knows on this computer; `how` is `"extension"` (Claude: the .mcpb) or `"config"` |
| `connect <id>` / `disconnect <id>` | adds / removes Context Capsule in that app's MCP settings (merges, never overwrites others; keeps a `.bak`); `{"ok","message"}`. For `claude` it answers `{"ok":false,"how":"extension"}` and the app installs the .mcpb as before |
| `day [--date YYYY-MM-DD]` | `{"text","chars"}`: one paste-ready message for any AI chat (≤ 60,000 characters): the capsule guide, the day's work (wrapped as untrusted, budgeted like `get_day_activity`), the current project capsules to merge into, and how to answer |
| `save [--date YYYY-MM-DD]` | reads a pasted AI reply on stdin; finds capsules between `=== CAPSULE ===` and `=== END ===` lines (or one bare capsule); checks each with the same rules as `save_capsule` (never saved as a draft: every refusal is reported); `{"saved":[{"title","path"}],"refused":[{"title","why"}]}`. `--date`: the day a capsule is about when it doesn't say (default today) |

Clients (`id`: settings file; Windows under `%USERPROFILE%` unless noted): `claude` (extension; detected by the app or
its folder: Mac `~/Library/Application Support/Claude`, Windows `%APPDATA%\Claude`, Linux `~/.config/Claude`);
`chatgpt` and `codex` (one shared file: `~/.codex/config.toml`, or `$CODEX_HOME`, `[mcp_servers.context-capsule]`);
`cursor` (`~/.cursor/mcp.json`); `vscode` (`Code/User/mcp.json`: Mac `~/Library/Application Support`, Windows
`%APPDATA%`, Linux `~/.config`; key `servers`, `"type": "stdio"`); `windsurf` (`~/.codeium/windsurf/mcp_config.json`);
`lmstudio` (`~/.lmstudio/mcp.json`); `gemini` (Gemini CLI, `~/.gemini/settings.json`). JSON files use `mcpServers`
unless noted. Antigravity is left out until its config path settles. The entry is `{"command": <the binary>,
"args": ["serve"]}` (from source `node <abs>/dist/index.js serve`); "connected" = our entry is there and its program exists.

In a chat AI's answer each capsule starts with a small front matter block (`title`, `project`, `kind: daily|project`,
`date`); `save` adds the rest (author, sessions, apps, playbook) as `save_capsule` does, checks citations against
that day's notes, and adds a project the AI started once its capsule is saved.

The apps (Settings → **Connect your AI**) list `clients`, with Connect / a ✓ per app. **Your AI** (a choice in
Settings) is what **Make today's capsules** opens: an MCP app with the request typed in where it can be, or —
for any other AI — **Copy my day** (runs `day`, copies `text`, opens the AI's website if chosen) and then
**Save from clipboard** (runs `save` on the clipboard). Words say "your AI", not "Claude", wherever any AI works.
Nothing goes to the internet except what the person pastes into their AI themselves.

## Timeline (all recorders)

A **Timeline** page (sidebar, under Your work) shows everything recorded for one day (Today, Yesterday,
or a date picker), short enough to read at a glance. Nothing leaves the computer.

The recorder writes a new event every few seconds when the screen text changes, so the page never lists events
one by one at first. It groups them into **stretches**.

**Stretches** (shared core: Swift `Timeline`, Rust `core::timeline`, same rules, parity tests):
- A stretch is a run of consecutive screen events in one session with the same `app` and the same `title`
  (an empty title matches only an empty title).
- A stretch also ends at a voice/typed note, at a gap from `gaps.jsonl`, and when the next event starts more
  than 60 s after the previous one's `end`.
- Each stretch shows: start–end (`t` of the first event, `end` of the last) and its length ("38 min";
  under one minute: "under 1 min"); **app · title** (app alone when the title is empty); the first non-empty
  `where`; a **peek**: the first line of 20 or more characters (trimmed) from the event with the most text,
  cut at 120 characters with "…" (none if no line is that long); and the number of reads ("24 reads").
- Its project is the most-used `project` among its events (after `assign.json`), or none.
- **Show what was read** opens the stretch's events (the reads): time, `where`, `focus:"pointer"` → "Looked at",
  and the text folded to 3 lines with **Show all**.

**The page, top to bottom:**
1. **Header:** "Today" (or the date), then "Thursday 23 October · 1 h 50 min recorded · 3 recordings", and
   **Search**. Recorded time = the sum of the stretches' lengths in the day.
2. **Day bar:** one row from the hour before the first stretch to the hour after the last, hour labels below;
   each stretch is a segment coloured by its project (none → grey). Clicking a segment opens its recording and
   scrolls to the stretch.
3. **Where your time went:** one row per project (name, bar, minutes), longest first; work with no project is
   "Not sorted". If no stretch has a project, the rows are per app instead. Project colours: a fixed order of
   plain system colours (blue, teal, orange, green, pink, brown), "Not sorted" grey. No purple.
4. **Recordings**, one per session that overlaps the day, folded to one line: "10:00 – 11:40 · 1 h 40 min ·
   Website, Client report" (projects by time, or the top 2 apps if none) and **Make a capsule**. Open by default:
   the one still recording, else none. Opened, it lists in time order: stretches; voice/typed notes as speech
   bubbles with the full words; gaps as one grey line "Not read 11:00–11:05: a private app was in front"
   ("this window wasn't shared", "nothing readable", "system screen", "the screen was locked").
   Time between recordings is not shown.

**Gaps:** recorders write `gaps.jsonl` in the session folder, one line per stretch that wasn't read:
`{"t":"…","end":"…","why":"private_app|not_shared|no_text|system|locked"}` (the same skip reasons the menu
shows; consecutive skips with the same reason extend one line, like events; under 5 s isn't written).

**Search** (the day): keeps only stretches with a read whose title, where, app or text contains the words (and
notes containing them); recordings with a match open, and **Show what was read** lists only matching reads.

**Delete**, after a confirm, permanent, not while that session is still recording ("Stop first"): a read
(removes that line from events.jsonl, matched by its `t` and exact line), a whole stretch (all its lines), or a
whole recording (the session folder).

**Make a capsule** (per recording): opens Your AI with
`Use Context Capsule to make a capsule from my session <session id>.` (the AI uses get_session_activity).

Today updates by itself every few seconds while recording, keeping what is open and the scroll position.

The Claude side (`get_day_activity`, `get_session_activity`) also lists gaps in one short line each, so the AI
knows what wasn't seen.
