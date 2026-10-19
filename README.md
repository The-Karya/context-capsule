# Context Capsule

by Karya

**Turn what you worked on into context your AI already knows, and share it.**

Press a hotkey, pick the window or screen to record in the macOS picker, work as
usual, press it again. Context Capsule reads the text of what you picked while you
work. Your own Claude turns that into a **capsule**: one Markdown file with what you
worked on, the decisions you made and why, open questions and next steps.

Load the capsule into Claude and it knows your project. Email it to a teammate and
their Claude knows it too.

- **Local.** Screen text stays on your Mac. No account, no cloud, no telemetry.
- **You choose what it sees.** Each recording starts with the macOS window/screen
  picker. Context Capsule reads only what you picked, and needs no Screen Recording permission.
- **No screenshots kept.** Only the text on screen is saved; images are thrown away.
- **Private apps skipped.** Password managers, WhatsApp, Signal and any app you list
  are never recorded. Anything that looks like a key or password is redacted.
- **Your AI does the writing.** Context Capsule has no AI key of its own; it hands a short
  activity timeline to your Claude through MCP.
- **Open format.** A capsule is plain Markdown with a little front matter. Any person
  or AI can read it.

> v1 is macOS 14+ and Claude (Desktop or Code). Windows and ChatGPT are planned.

## How it works

```
 ⌥⇧C  ──►  Context Capsule.app  ──►  ~/.context-capsule/sessions/<id>/events.jsonl
                                              │  (app, window, on-screen text, times)
                                              ▼
 Claude  ◄──►  context-capsule-mcp  ──►  ~/.context-capsule/capsules/<name>.capsule.md
                                              │
                                              ▼
                                   export to Desktop → email → import
```

## Install

You need macOS 14+, Node 18+ and [bun](https://bun.sh) (or npm).

```sh
git clone <this repo> context-capsule && cd context-capsule

# 1. the recorder
capture/build.sh              # builds "capture/build/Context Capsule.app"
open "capture/build/Context Capsule.app"

# 2. the MCP server
cd mcp && bun install && bun run build
```

Each time you start a recording, macOS shows its window/screen picker; pick a window
or a display and recording begins. There is no Screen Recording permission to grant.
Optional: menu → **Read text directly (Accessibility)…** lets Context Capsule read
text straight from apps (cleaner than OCR, and gives window titles). Without it, it
uses OCR on the picked content.

### Connect to Claude

**Claude Code**

```sh
claude mcp add context-capsule -- node /absolute/path/to/context-capsule/mcp/dist/index.js
```

**Claude Desktop**: add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "context-capsule": {
      "command": "node",
      "args": ["/absolute/path/to/context-capsule/mcp/dist/index.js"]
    }
  }
}
```

Restart Claude Desktop.

## Use it

1. Press **⌥⇧C** (or ask Claude: *"start recording"*). Pick a window or screen in the
   macOS picker. The menu-bar circle fills in. Close the picker to cancel.
2. Work. Any time, press **⌥⇧V** and say what you're doing and why; press ⌥⇧V again
   to finish (see *Voice notes* below).
3. Press **⌥⇧C** again (or *"stop recording"*).
4. Ask Claude: *"Make a capsule from my last session."* Claude reads the timeline,
   writes the capsule and saves it. Read it; edit anything you don't want to share.
5. *"Export the capsule"* puts it on your Desktop, ready to email.
6. Someone sent you one? *"Import ~/Downloads/pricing-redesign.capsule.md"* and ask
   away.

## Voice notes

While recording, explain your work out loud: what you're doing, how, and the decisions
you make and why. Your words become the main source for the capsule; the screen text
backs them up.

- Press **⌥⇧V** (or menu → *Add voice note*), talk, press **⌥⇧V** again. The menu-bar
  icon shows a microphone while it listens. A note stops by itself after 2 minutes.
- The microphone is open **only** between the two presses.
- Speech is turned into text **on your Mac** (Apple Speech, on-device only). If your
  language has no on-device model, Context Capsule tells you and does nothing; it never
  falls back to Apple's servers. Set another language with `"voice_locale": "en-GB"` in
  `~/.context-capsule/config.json`.
- **No audio is saved.** Only the text goes into the session, redacted like screen text.
  Silent clips are skipped ("Didn't catch that").
- Prefer typing? Tell Claude *"add a note: we chose Postgres because the team knows it"*
  (the `add_note` tool).

macOS asks once for Microphone and Speech Recognition. If you said no, turn them on in
System Settings → Privacy & Security.

## The capsule format

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
- **Use annual pricing by default** — 70% of trials pick it. *(seen in Linear, 10:14)*
## Open questions
## Next steps
## Sources
```

## MCP tools

`start_recording`, `stop_recording`, `recording_status`, `add_note`, `list_sessions`,
`get_session_activity`, `save_capsule`, `list_capsules`, `get_capsule`,
`import_capsule`, `export_capsule`, the `make_capsule` prompt, and every capsule as
a `capsule://<name>` resource. Details in [mcp/README.md](mcp/README.md).

## Privacy

Everything is in `~/.context-capsule/`. Delete a session folder and it is gone.
Add apps to never record in `~/.context-capsule/config.json`:

```json
{ "author": "Your Name", "exclude_apps": ["com.example.secretapp"] }
```

Always read a capsule before you share it: it is your work, in your words, and you
decide what leaves your laptop.

## Develop

- `capture/tests/run.sh`: recorder logic tests (Swift)
- `cd mcp && bun run test`: server tests
- [SPEC.md](SPEC.md): the contract between the recorder and the server

## License

MIT © Karya
