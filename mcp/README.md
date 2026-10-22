# context-capsule-mcp

The MCP server for **Context Capsule**. It lets your own AI (Claude, ChatGPT, Codex, Cursor,
VS Code, Windsurf, LM Studio, Gemini CLI, or any MCP client) turn a recorded work session into
a **capsule**: one Markdown file with what you worked on, the decisions you made and why.
Send the file to anyone; their AI loads it and knows the project. An AI without tools works
too: paste your day into any chat and paste its answer back (`day` / `save` below).

The Context Capsule menu-bar app (in `../capture`) does the recording. This server reads what
it wrote, starts and stops it, and saves, imports and exports capsules. No account, no
cloud, no API key: files stay on your computer, and text reaches your AI only when it
reads it through these tools, or when you paste it yourself (see ../PRIVACY.md).

## Install

Needs Node 18 or newer.

```sh
cd mcp
bun install && bun run build     # or: npm install && npm run build
```

This makes `mcp/dist/index.js`. Find its absolute path with `realpath dist/index.js`,
and your node with `which node`.

The apps ship it as one self-contained program, `context-capsule-mcp` (no Node needed). Build it
with `./build-binary.sh` (bun): `dist-bin/context-capsule-mcp-macos` (universal),
`-windows-x64.exe`, `-linux-x64`, `-linux-arm64`. Or run it from npm: `npx context-capsule-mcp`.

## Commands

| command | does |
|---|---|
| `serve` (or nothing) | the MCP server on stdio |
| `serve --http [--port N]` | the MCP server over Streamable HTTP at `http://127.0.0.1:3740/mcp` (another free port if 3740 is taken; the address is printed on stderr). Only this computer can reach it; every request needs `Authorization: Bearer <token>`, the token being in `~/.context-capsule/mcp-token` (made on first use, readable by you only). No CORS: requests from web pages are refused |
| `clients` | JSON: `[{"id","name","installed","connected","how"}]` for every AI app it knows |
| `connect <id>` / `disconnect <id>` | adds / removes Context Capsule in that app's own MCP settings file; merges (other servers, settings and comments stay), keeps a `.bak`, never touches a file it can't read. `{"ok","message"}` |
| `day [--date YYYY-MM-DD]` | JSON `{"text","chars"}`: one message (up to ~60,000 characters) to paste into any AI chat: the capsule guide, your day (screen text wrapped as information) and how to answer: each capsule between `=== CAPSULE ===` and `=== END ===` lines |
| `save [--date YYYY-MM-DD]` | reads that AI's answer on stdin, checks each capsule like `save_capsule` (every section, citations that match real notes, secrets removed) and saves the good ones. JSON `{"saved":[{"title","path"}],"refused":[{"title","why"}]}` |

### The apps `connect` knows

| id | app | settings file (Mac / Linux; Windows under `%USERPROFILE%` unless noted) |
|---|---|---|
| `claude` | Claude desktop | none: Claude uses the extension (`extension/`, `context-capsule.mcpb`) |
| `chatgpt`, `codex` | ChatGPT desktop app, Codex (they share one file) | `~/.codex/config.toml` (`$CODEX_HOME` if set), `[mcp_servers.context-capsule]` |
| `cursor` | Cursor | `~/.cursor/mcp.json`, `mcpServers` |
| `vscode` | VS Code | Mac `~/Library/Application Support/Code/User/mcp.json`, Linux `~/.config/Code/User/mcp.json`, Windows `%APPDATA%\Code\User\mcp.json`; `servers`, `"type": "stdio"` |
| `windsurf` | Windsurf | `~/.codeium/windsurf/mcp_config.json`, `mcpServers` |
| `lmstudio` | LM Studio | `~/.lmstudio/mcp.json`, `mcpServers` |
| `gemini` | Gemini CLI | `~/.gemini/settings.json`, `mcpServers` |

The entry runs the program itself with `serve` (from source: `node <abs>/dist/index.js serve`).
"Connected" means the entry is there and points at a program that exists.

## Use it with any MCP client

Over stdio (most clients). With the app installed, point at the program inside it; from npm, use `npx`:

```json
{
  "mcpServers": {
    "context-capsule": {
      "command": "npx",
      "args": ["-y", "context-capsule-mcp", "serve"]
    }
  }
}
```

Over HTTP, for clients that only take a URL. Start `context-capsule-mcp serve --http`, then:

```json
{
  "mcpServers": {
    "context-capsule": {
      "url": "http://127.0.0.1:3740/mcp",
      "headers": { "Authorization": "Bearer <the text in ~/.context-capsule/mcp-token>" }
    }
  }
}
```

Every tool says what it may do (`readOnlyHint`, `destructiveHint`, `idempotentHint`; nothing reaches the
internet, so `openWorldHint` is false), and `list_capsules`, `list_sessions` and `recording_status` also
return structured data (`outputSchema`). Check it with the MCP Inspector:
`npx @modelcontextprotocol/inspector --cli node dist/index.js serve --method tools/list`.
`server.json` describes the npm package for the MCP Registry.

## Connect by hand

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
| `list_projects` | projects, their clues, time today, and their capsules |
| `create_project` `{name, match?}` | adds a project (or clues to one); clues also sort past recordings |
| `set_project` `{project}` | the recorder sticks to this project (`"auto"`: detect again) |
| `assign_project` `{session_id, project, from?, to?}` | corrects which project a stretch of work belongs to |
| `prepare_day` `{date?}` | start of "make my capsules": checklist + everything needed in one call |
| `finish_day` `{date?}` | end: checks every project got its capsules, lists what's missing |
| `get_day_activity` `{date?, project?, max_chars?}` | a whole day split by project, with time per project |
| `list_sessions` | sessions, newest first: start, end, duration, text source (Accessibility / OCR), notes, top apps |
| `get_session_activity` `{session_id, max_chars?}` | a short timeline of a session for Claude to read (`"latest"` works) |
| `save_capsule` `{title, markdown, session_ids?}` | saves `capsules/<slug>.capsule.md` with front matter; never overwrites |
| `list_capsules` | title, author, created, path |
| `get_capsule` `{name}` | the full text (by name, file name or title) |
| `search_capsules` `{query, limit?=8}` | searches every saved capsule (received ones too) section by section, by the question's words; matches in Decisions, Where things stand and Next steps and newer capsules rank higher. Returns the best sections, each to cite like *(Pricing page, 19 Oct, Decisions)*, or says plainly that nothing matches. Read-only, no network |
| `import_capsule` `{path}` | copies a received capsule in, after checking it. Any file name works (`.capsule.md`, `.md`…) as long as it is a capsule; otherwise a clear "isn't a Context Capsule" and nothing is imported |
| `export_capsule` `{name, to_path?}` | copies a capsule to the Desktop (or a path) to send |

Also:

- Prompt **`make_capsule`** `{session_id}`: how to write a good capsule.
- Prompt **`continue_from_capsule`** `{capsule}`: load a capsule (yours or received) and carry on the work.
  The apps open Claude with `Use Context Capsule to continue from the capsule at <path>.`; Claude imports it, then follows this.
- Prompt **`ask_capsules`** `{question}`: answer a question about past work from your capsules
  (`search_capsules`, then `get_capsule` if needed), short, with citations, or say they don't say.
- Prompt **`make_day_capsules`** `{date?}`: split a day by project, sort unsorted work with
  you, write a daily capsule per project and update each project's growing capsule.
- Resources **`capsule://<slug>`**: every capsule, so Claude can read them directly.

### The timeline

`get_session_activity` turns thousands of screen samples into something short:

- one segment per app and window, with local times (`## 10:01–10:14 · Google Chrome — Pricing`);
- each text line shown once, where it first appeared; spaces squashed;
- menus and sidebars that repeat across many windows, numbers-only lines, clocks and
  very short lines are dropped;
- voice and typed notes sit in the timeline where they happened:
  `>> You said (10:14): "…"` / `>> You typed (10:20): "…"`;
- stretches the recorder didn't read (from the session's `gaps.jsonl`, when there is one:
  `{"t","end","why"}`, `why` = `private_app`, `not_shared`, `no_text`, `system` or `locked`) are one short
  line each, where they happened: `(Not read 10:40–10:55: a private app was in front)`. `get_day_activity`
  lists the day's near the top, `get_moment` those in its stretch; many are folded into one line;
- a moment deleted in the app's Timeline is simply gone (its line is removed from `events.jsonl`);
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
playbook: 2026-10-22
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

Any person or AI can read it ("For Claude" is the section for whichever AI loads it).
`save_capsule` adds the header for you and refuses, with a numbered list of fixes, a capsule
with a missing section or a citation that matches no note. A second capsule with the same
title is saved as `-2`, `-3`, never over the first.

## Privacy

- All data stays in `~/.context-capsule` on your computer. This server makes no network calls;
  `serve --http` listens on 127.0.0.1 only, behind a token. `connect` only edits the AI app's
  own settings file on this computer.
- The app keeps text only, never images, and skips password managers and private chat
  apps (plus any app you list in `config.json` → `exclude_apps`).
- Keys, tokens and `password:` lines are redacted by the app, and again by `save_capsule`.
- Your AI only sees a session when you ask it to read one (or paste it yourself). What it
  does with it follows that AI's own settings.
- Before you send a capsule, read it. It is your work; share what you want to share.

## Develop

```sh
bun run dev     # run from source
bun run test    # build, then all tests (uses a temp CONTEXT_CAPSULE_HOME)
```

MIT licensed.
