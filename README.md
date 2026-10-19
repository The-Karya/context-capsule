# Karya Capsule

**Turn what you worked on into context your AI already knows, and share it.**

Press a hotkey, work as usual, press it again. Capsule reads your screen while you
work. Your own Claude turns that into a **capsule**: one Markdown file with what you
worked on, the decisions you made and why, open questions and next steps.

Load the capsule into Claude and it knows your project. Email it to a teammate and
their Claude knows it too.

- **Local.** Screen text stays on your Mac. No account, no cloud, no telemetry.
- **No screenshots kept.** Only the text on screen is saved; images are thrown away.
- **Private apps skipped.** Password managers, WhatsApp, Signal and any app you list
  are never recorded. Anything that looks like a key or password is redacted.
- **Your AI does the writing.** Capsule has no AI key of its own; it hands a short
  activity timeline to your Claude through MCP.
- **Open format.** A capsule is plain Markdown with a little front matter. Any person
  or AI can read it.

> v1 is macOS 13+ and Claude (Desktop or Code). Windows and ChatGPT are planned.

## How it works

```
 ⌥⇧C  ──►  Capsule.app (menu bar)  ──►  ~/.karya-capsule/sessions/<id>/events.jsonl
                                              │  (app, window, on-screen text, times)
                                              ▼
 Claude  ◄──►  karya-capsule-mcp  ──►  ~/.karya-capsule/capsules/<name>.capsule.md
                                              │
                                              ▼
                                   export to Desktop → email → import
```

## Install

You need macOS 13+, Node 18+ and [bun](https://bun.sh) (or npm).

```sh
git clone <this repo> karya-capsule && cd karya-capsule

# 1. the recorder
capture/build.sh              # builds capture/build/Capsule.app
open capture/build/Capsule.app

# 2. the MCP server
cd mcp && bun install && bun run build
```

The first time you start a recording, macOS asks for **Screen Recording**
permission (System Settings → Privacy & Security → Screen Recording → Capsule).
Grant it, then quit and reopen Capsule. Accessibility permission is optional and
gives better window titles.

### Connect to Claude

**Claude Code**

```sh
claude mcp add karya-capsule -- node /absolute/path/to/karya-capsule/mcp/dist/index.js
```

**Claude Desktop**: add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "karya-capsule": {
      "command": "node",
      "args": ["/absolute/path/to/karya-capsule/mcp/dist/index.js"]
    }
  }
}
```

Restart Claude Desktop.

## Use it

1. Press **⌥⇧C** (or ask Claude: *"start recording"*). The menu-bar circle fills in.
2. Work.
3. Press **⌥⇧C** again (or *"stop recording"*).
4. Ask Claude: *"Make a capsule from my last session."* Claude reads the timeline,
   writes the capsule and saves it. Read it; edit anything you don't want to share.
5. *"Export the capsule"* puts it on your Desktop, ready to email.
6. Someone sent you one? *"Import ~/Downloads/pricing-redesign.capsule.md"* and ask
   away.

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

`start_recording`, `stop_recording`, `recording_status`, `list_sessions`,
`get_session_activity`, `save_capsule`, `list_capsules`, `get_capsule`,
`import_capsule`, `export_capsule`, the `make_capsule` prompt, and every capsule as
a `capsule://<name>` resource. Details in [mcp/README.md](mcp/README.md).

## Privacy

Everything is in `~/.karya-capsule/`. Delete a session folder and it is gone.
Add apps to never record in `~/.karya-capsule/config.json`:

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
