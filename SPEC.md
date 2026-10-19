# karya-capsule — v1 spec

Open-source, macOS-first. Press a hotkey, work, press it again: Karya Capsule reads
your screen while you work and helps your own AI (Claude, via MCP) turn that into a
**capsule** — one shareable file with what you worked on, the decisions you made and
why. Send the capsule to anyone; their Claude loads it and knows the project.

Everything stays on the laptop. No accounts, no cloud, no AI key of our own: the
user's Claude does the writing through MCP.

## Parts

```
karya-capsule/
  capture/            Swift menu-bar app "Capsule" (macOS 13+)
  mcp/                TypeScript MCP server (stdio), Node >= 18
  SPEC.md             this file — the contract between the two
  README.md, LICENSE (MIT)
```

### 1. capture/ (Swift)

A small menu-bar app, built with `swiftc` (no Xcode project needed), one or a few
files. Build script `capture/build.sh` → `capture/build/Capsule.app`.

- **Hotkey** `⌥⇧C` (Option-Shift-C) toggles recording. Also a menu: Start / Stop
  recording, Open sessions folder, Quit. Icon: hollow circle idle, filled circle
  while recording.
- While recording, every **3 s**:
  - frontmost app name + bundle id, focused window title (Accessibility if granted,
    else CGWindowList owner/name);
  - screenshot of the display under the frontmost window, OCR with Apple Vision
    (`VNRecognizeTextRequest`, accurate, language correction on);
  - if the app, title and text are the same as the previous sample, skip it
    (only extend the previous event's `end`).
- **Never record** (skip the sample entirely, write nothing): password managers and
  private apps — bundle ids containing `1password`, `bitwarden`, `keychain`,
  `lastpass`, `dashlane`, `signal`, `whatsapp`, plus a user list in
  `~/.karya-capsule/config.json` → `{"exclude_apps": ["com.example.app"]}`.
- **Images are not kept.** Only text is written.
- Redact before writing: strings that look like API keys / tokens
  (`sk-…`, `ghp_…`, `xox[bp]-…`, `AKIA…`, long hex/base64 > 32 chars), and
  `password: …` lines → `[redacted]`.
- Control from outside (the MCP server uses this): the app watches
  `~/.karya-capsule/control` — a JSON file `{"command":"start"|"stop","at":<ms>}`.
  It also writes `~/.karya-capsule/state.json`:
  `{"recording": bool, "session_id": string|null, "started_at": iso|null, "pid": int}`.
- Permissions: Screen Recording (required), Accessibility (optional, better titles).
  On first start, if screen capture is denied, show one alert explaining why and how
  to grant it.

### Session files (the contract)

```
~/.karya-capsule/
  sessions/<session_id>/          session_id = YYYYMMDD-HHMMSS
    session.json                  {"id","started_at","ended_at"|null,"host","version":1}
    events.jsonl                  one JSON object per line, see below
  capsules/<slug>.capsule.md      saved capsules
  config.json, control, state.json
```

`events.jsonl` line:

```json
{"t":"2026-10-19T10:01:03Z","end":"2026-10-19T10:01:42Z","app":"Google Chrome",
 "bundle":"com.google.Chrome","title":"Pricing — Linear","text":"…OCR text…"}
```

`text` is capped at 4000 chars per event (keep the start). Times are ISO-8601 UTC.

### 2. mcp/ (TypeScript)

`@modelcontextprotocol/sdk`, stdio transport. `bun` for dev; the built output runs
with plain `node` so `npx`/Claude Desktop can launch it. Binary name
`karya-capsule-mcp`.

Tools:

| tool | does |
|---|---|
| `start_recording` | writes `control` start; returns state (errors clearly if the app isn't running, with how to start it) |
| `stop_recording` | writes `control` stop; returns the session id |
| `recording_status` | reads `state.json` |
| `list_sessions` | id, start, end, duration, top apps |
| `get_session_activity` `{session_id, max_chars?=60000}` | a condensed timeline for the AI: grouped by app/window, time spans, de-duplicated text (drop lines seen before, collapse whitespace), trimmed to `max_chars` keeping a fair share per segment |
| `save_capsule` `{title, markdown, session_ids?}` | validates + writes `capsules/<slug>.capsule.md` with front matter; returns path |
| `list_capsules` | title, author, created, path |
| `get_capsule` `{name}` | full text |
| `import_capsule` `{path}` | copies a received `.capsule.md` into `capsules/` (validates format) |
| `export_capsule` `{name, to_path?}` | copies to Desktop (default) or a path; returns the path, ready to email |

Prompt `make_capsule` `{session_id}`: tells the AI how to write a capsule from
`get_session_activity` — sections below, decisions must cite the moment/app they came
from, mark guesses as guesses, never include secrets, keep it under ~1500 words.

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

Windows/Linux, ChatGPT remote connector (needs hosting), images, cloud sync, teams.

## Rules

- No secrets, no Karya-internal code or URLs. MIT. Clean, small, readable.
- Every source file starts with a short header comment naming the project.
