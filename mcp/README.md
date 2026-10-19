# context-capsule-mcp

The MCP server for **Context Capsule**. It lets your own Claude (Desktop or Code) turn a
recorded work session into a **capsule**: one Markdown file with what you worked on,
the decisions you made and why. Send the file to anyone; their Claude loads it and
knows the project.

The Context Capsule menu-bar app (in `../capture`) does the recording. This server reads what
it wrote, starts and stops it, and saves, imports and exports capsules. Everything stays
on your Mac. No account, no cloud, no API key.

## Install

Needs Node 18 or newer.

```sh
cd mcp
bun install && bun run build     # or: npm install && npm run build
```

This makes `mcp/dist/index.js`. Find its absolute path with `realpath dist/index.js`,
and your node with `which node`.

## Connect

### Claude Desktop

Open Claude Desktop → Settings → Developer → Edit Config, and add:

```json
{
  "mcpServers": {
    "context-capsule": {
      "command": "/opt/homebrew/bin/node",
      "args": ["/Users/you/context-capsule/mcp/dist/index.js"]
    }
  }
}
```

Use your own paths (`which node`, `realpath dist/index.js`). Restart Claude Desktop.

### Claude Code

```sh
claude mcp add context-capsule -- node /Users/you/context-capsule/mcp/dist/index.js
```

### Settings

- `CONTEXT_CAPSULE_HOME`: where data lives. Default `~/.context-capsule`.
- `~/.context-capsule/config.json` → `{"author": "Your Name"}` sets the capsule author.
  Without it, your macOS full name is used.

## Use it

1. Open the Context Capsule app (`open -a "Context Capsule"`). A circle shows in the menu bar.
2. Say "start recording" to Claude (or press ⌥C). The macOS picker opens: choose the
   window or screen to record. Work as usual.
   While recording, press ⌥V to say what you're doing and why (a voice note), or
   tell Claude "add a note: …".
3. Say "stop recording", then use the **make_capsule** prompt (or ask "make a capsule
   from my last session").
4. Read the draft, ask for changes, then let Claude save it and export it to your Desktop.
5. Got a capsule from someone? "Import ~/Downloads/their-plan.capsule.md".

## Tools

| tool | does |
|---|---|
| `start_recording` | asks the app to start. The app opens the macOS picker; you choose the window or screen. Waits up to 6 s and says "picker is open", "recording: session id", or why it didn't start (e.g. you closed the picker). Tells you how to open the app if it isn't running |
| `stop_recording` | asks the app to stop; returns the session id and event count, or says why nothing was recorded |
| `recording_status` | is the app running, is the picker open, is it recording, events so far, why samples are skipped, a voice note in progress, why the last start failed |
| `add_note` `{text}` | adds a typed note to the session being recorded (written by the app as `"kind":"note"`) |
| `list_sessions` | sessions, newest first: start, end, duration, text source (Accessibility / OCR), notes, top apps |
| `get_session_activity` `{session_id, max_chars?}` | a short timeline of a session for Claude to read (`"latest"` works) |
| `save_capsule` `{title, markdown, session_ids?}` | saves `capsules/<slug>.capsule.md` with front matter; never overwrites |
| `list_capsules` | title, author, created, path |
| `get_capsule` `{name}` | the full text (by name, file name or title) |
| `import_capsule` `{path}` | copies a received capsule in, after checking it |
| `export_capsule` `{name, to_path?}` | copies a capsule to the Desktop (or a path) to send |

Also:

- Prompt **`make_capsule`** `{session_id}`: how to write a good capsule.
- Resources **`capsule://<slug>`**: every capsule, so Claude can read them directly.

### The timeline

`get_session_activity` turns thousands of screen samples into something short:

- one segment per app and window, with local times (`## 10:01–10:14 · Google Chrome — Pricing`);
- each text line shown once, where it first appeared; spaces squashed;
- menus and sidebars that repeat across many windows, numbers-only lines, clocks and
  very short lines are dropped;
- voice and typed notes sit in the timeline where they happened:
  `>> You said (10:14): "…"` / `>> You typed (10:20): "…"`;
- the result fits `max_chars` (default 60000). Notes are budgeted first and never cut
  before screen text; then each segment gets a fair share, and long ones are cut with `…`.

## Voice notes

While recording, press **⌥V** in the app, talk, press it again. Speech becomes text on
the Mac (Apple Speech, on-device only, no audio kept) and goes into the session as
`"kind":"voice"`. Or type: `add_note`. The `make_capsule` prompt treats notes as the
user's own words, the primary source for decisions, reasons and intent; screen text is
supporting evidence.

## Capsule format

A `.capsule.md` file is plain Markdown with a small header:

```markdown
---
capsule: 1
title: Pricing page redesign
author: Saachi
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

Any person or AI can read it. `save_capsule` adds the header for you and warns (but still
saves) if a section is missing. A second capsule with the same title is saved as
`-2`, `-3`, never over the first.

## Privacy

- All data stays in `~/.context-capsule` on your Mac. This server makes no network calls.
- The app keeps text only, never images, and skips password managers and private chat
  apps (plus any app you list in `config.json` → `exclude_apps`).
- Keys, tokens and `password:` lines are redacted by the app, and again by `save_capsule`.
- Claude only sees a session when you ask it to read one. What Claude does with it
  follows your Claude settings.
- Before you send a capsule, read it. It is your work; share what you want to share.

## Develop

```sh
bun run dev     # run from source
bun run test    # build, then all tests (uses a temp CONTEXT_CAPSULE_HOME)
```

MIT licensed.
