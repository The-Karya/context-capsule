# Context Capsule

by Karya

**Turn what you worked on into context your AI already knows, and share it.**

Press a hotkey, pick the window or screen to record in the system picker, work as
usual, press it again. Context Capsule reads the text of what you picked while you
work. Your own Claude turns that into a **capsule**: one Markdown file with what you
worked on, the decisions you made and why, open questions and next steps.

Load the capsule into Claude and it knows your project. Email it to a teammate and
their Claude knows it too.

- **Local.** Screen text stays on your computer. No account, no cloud, no telemetry.
- **You choose what it sees.** Each recording starts with the system window/screen
  picker (macOS, Windows, or the Linux screen-sharing portal). Context Capsule reads
  only what you picked; on macOS it needs no Screen Recording permission.
- **No screenshots kept.** Only the text on screen is saved; images are thrown away.
- **Private apps skipped.** Password managers, WhatsApp, Signal and any app you list
  are never recorded. Anything that looks like a key or password is redacted.
- **Your AI does the writing.** Context Capsule has no AI key of its own; it hands a short
  activity timeline to your Claude through MCP.
- **Open format.** A capsule is plain Markdown with a little front matter. Any person
  or AI can read it.

> v1: macOS 14+, Windows 10 (1903+) / 11, and Linux (Wayland or X11), with Claude
> (Desktop or Code). ChatGPT is planned.
>
> | | macOS | Windows | Linux |
> |---|---|---|---|
> | Recorder | `capture/` (Swift) | `capture-rs/windows/` (Rust) | `capture-rs/linux/` (Rust) |
> | Record / voice note | ⌥C / ⌥V | Ctrl+Alt+Shift+C / V | Ctrl+Alt+Shift+C / V |
> | Status | tested | built, being tested | built, not yet tried on a real desktop |

## How it works

```
 ⌥C  ──►  Context Capsule.app  ──►  ~/.context-capsule/sessions/<id>/events.jsonl
                                              │  (app, window, on-screen text, times)
                                              ▼
 Claude  ◄──►  context-capsule-mcp  ──►  ~/.context-capsule/capsules/<name>.capsule.md
                                              │
                                              ▼
                                   export to Desktop → email → import
```

## Install

You need Node 18+ and [bun](https://bun.sh) (or npm) for the MCP server, plus the
recorder for your system.

```sh
git clone <this repo> context-capsule && cd context-capsule

# the MCP server (all systems)
cd mcp && bun install && bun run build && cd ..
```

**macOS 14+**: Xcode command line tools (`xcode-select --install`).

```sh
capture/build.sh              # builds "capture/build/Context Capsule.app"
open "capture/build/Context Capsule.app"
```

**Windows 10/11**: [Rust](https://rustup.rs), Visual Studio Build Tools (C++), CMake and
LLVM (for voice notes). In PowerShell: `capture-rs\windows\build.ps1`, then run
`capture-rs\target\release\context-capsule.exe`. A circle appears in the system tray.
Details: [capture-rs/windows/README.md](capture-rs/windows/README.md). From a Mac you can
also cross-build the `.exe`: `capture-rs/windows/cross-build.sh`.

**Linux**: Rust, plus PipeWire, D-Bus and (optionally) Tesseract dev packages; see
[capture-rs/linux/README.md](capture-rs/linux/README.md). Then `capture-rs/linux/build.sh`
and run `context-capsule &`.

Data lives in `~/.context-capsule/` (`%USERPROFILE%\.context-capsule\` on Windows;
`$XDG_DATA_HOME/context-capsule` on Linux when that is set).

Each time you start a recording, the system shows its window/screen picker; pick a
window or a display and recording begins. Text comes straight from apps where the
system allows it (macOS Accessibility — optional, menu → **Read text directly
(Accessibility)…**; Windows UI Automation; Linux AT-SPI), and from OCR of the picked
content otherwise.

### Connect to Claude

**Claude Code**

```sh
claude mcp add context-capsule -- node /absolute/path/to/context-capsule/mcp/dist/index.js
```

**Claude Desktop**: add to its config file (macOS
`~/Library/Application Support/Claude/claude_desktop_config.json`, Windows
`%APPDATA%\Claude\claude_desktop_config.json`):

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

1. Press **⌥C** (Windows/Linux: **Ctrl+Alt+Shift+C**), or ask Claude: *"start
   recording"*. Pick a window or screen in the picker. The menu-bar circle fills in. Close the picker to cancel.
2. Work. Any time, press **⌥V** (Ctrl+Alt+Shift+V) and say what you're doing and why; press it again
   to finish (see *Voice notes* below).
3. Press **⌥C** again (or *"stop recording"*).
4. Ask Claude: *"Make a capsule from my last session."* Claude reads the timeline,
   writes the capsule and saves it. Read it; edit anything you don't want to share.
5. *"Export the capsule"* puts it on your Desktop, ready to email.
6. Someone sent you one? *"Import ~/Downloads/pricing-redesign.capsule.md"* and ask
   away.

## Voice notes

While recording, explain your work out loud: what you're doing, how, and the decisions
you make and why. Your words become the main source for the capsule; the screen text
backs them up.

- Press **⌥V** (or menu → *Add voice note*), talk, press **⌥V** again. The menu-bar
  icon shows a microphone while it listens. A note stops by itself after 2 minutes.
- The microphone is open **only** between the two presses.
- Speech is turned into text **on your computer**: Apple Speech (on-device only) on
  macOS; whisper.cpp on Windows and Linux (a one-time ~148 MB model download, only
  after you agree). If your language has no on-device model, Context Capsule tells you
  and does nothing; it never falls back to a cloud service. Set another language with `"voice_locale": "en-GB"` in
  `~/.context-capsule/config.json`.
- **No audio is saved.** Only the text goes into the session, redacted like screen text.
  Silent clips are skipped ("Didn't catch that").
- Prefer typing? Tell Claude *"add a note: we chose Postgres because the team knows it"*
  (the `add_note` tool).

macOS asks once for Microphone and Speech Recognition. If you said no, turn them on in
System Settings → Privacy & Security. On macOS, **Dictation must be on** (System Settings →
Keyboard → Dictation): Apple allows speech recognition only then, even on-device.

## Projects

A day often has several projects (1 h on one, 2 h on another…). Context Capsule keeps
them apart:

- **It detects the project** from clues in the window: a folder, file, website or document
  name. The menu shows the current project (**Project: …**); pick one there to stick to
  it, or **New project from this window…** to make one (it remembers the window's clue).
- **Claude sorts the rest by itself.** Ask: *"make my capsules for today"* (the
  `make_day_capsules` prompt). Claude groups new work into projects (and remembers clues so
  it's automatic next time), then writes:
  - a **daily capsule** per project (`website-2026-10-19.capsule.md`), and
  - one **growing capsule** per project (`website.capsule.md`), updated every day you work
    on it. Old versions are kept in `capsules/.history/`.
- At the end it lists what it sorted, so you can correct anything.
- Projects live in `~/.context-capsule/projects.json` (name + clues); edit it by hand
  if you like.
- On macOS, turn on **Accessibility** (menu → *Read text directly (Accessibility)…*):
  window titles and web addresses are what projects are recognised by.

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

- `capture/tests/run.sh`: macOS recorder logic tests (Swift)
- `cd capture-rs && cargo test -p context-capsule-core`: shared Windows/Linux core (same test vectors as Swift)
- `cargo test -p context-capsule-windows` / Linux in Docker: see `capture-rs/ci/linux.Dockerfile`
- `cd mcp && bun run test`: server tests
- [SPEC.md](SPEC.md): the contract between the recorder and the server

## License

MIT © Karya
