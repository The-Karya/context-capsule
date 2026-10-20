# Context Capsule

by Karya

**Context for your Claude, and for anyone's.**

You already use Claude for your work. Context Capsule quietly notes what you work on (the
text on your screen and what you say about it) and lets Claude turn your day into
**capsules**: one per project, with what you did, what you decided and why, and what's next.

Send a capsule to anyone. Their Claude loads it and carries on, with nothing to explain. A
capsule is written **for Claude**: people can read it, but they don't have to.

- **You choose what it sees.** Each time, you pick the screen or window in your computer's own
  picker. Only that is read.
- **Only text.** No screenshots, no audio. Private apps (password managers, WhatsApp, Signal)
  and password fields are never recorded.
- **On your computer.** No account, no cloud, no tracking. Text goes to Claude only when you
  ask Claude to make or read a capsule. Every detail is in [PRIVACY.md](PRIVACY.md).
- **Your Claude does the writing.** Context Capsule has no AI of its own.
- **Plain files.** A capsule is Markdown. Any person or AI can read it.

| | macOS 14+ | Windows 10 / 11 | Linux (Wayland, X11) |
|---|---|---|---|
| Start / stop contextualizing | ⌥C | Alt+C | Alt+C |
| Speak a note | ⌥V | Alt+V | Alt+V |
| Status | in daily use | built, first tests now | built, not yet tried on a real desktop |

## Get started

1. **Open Context Capsule.** It lives in your menu bar (Windows: system tray) and opens by
   itself at login.
2. **Settings** opens the first time. Turn each step on, and it gets a ✓:
   - **Read your apps** (Mac): cleaner text, and window names, so projects are found by themselves.
   - **Voice notes** and **Dictation**, so you can talk.
   - **Connect Claude**, in one click: the Claude app asks you to install Context Capsule.
   - **Open at login**.
3. **Start contextualizing** (⌥C) and pick your screen. The menu bar shows a red dot and a timer.
4. Work. Any time, **Speak a note** (⌥V): say what you're doing and why. The menu bar shows a
   mic with bars that move with your voice. Press ⌥V again when done.
5. At the end of the day, click **Make today's capsules**. Claude opens; press ⌘V, then Return.
   Claude sorts your day into projects and writes the capsules.
6. **Share** a capsule from **Capsules**: copy it into WhatsApp, Slack or email, or AirDrop it.

## The Context Capsule window

Everything in one place (menu → **Open Context Capsule**). Mac for now; on Windows and Linux
the same actions are in the tray menu and a small setup window.

- **Home**: a hello with your name, Start / Stop contextualizing with a live timer, Speak a
  note, Add a screen, your projects as one-click buttons, time per project today and this
  week, your latest capsule, and Make today's capsules. It fits any window size.
- **Capsules**: your capsules by project, with Share (Copy file, Copy text, Save a copy,
  Show in Finder, AirDrop/Mail/Messages) and Open.
- **Projects**: rename them, change how they're found, delete them.
- **Settings**: your name (it goes on your capsules), set up, the evening reminder and its time, when to delete old recordings,
  sounds, the voice-note language, and apps never to record.
- **Help**: shortcuts, how it works, privacy.

The menu-bar menu is the quick version: Start contextualizing · Speak a note · Add a screen ·
Project · Open Context Capsule · Make today's capsules · Quit.

## Projects

A day often has several projects (an hour on one, two on another). Context Capsule keeps them
apart:

- It **recognises the project** from what's in the window: a folder, file, website or
  document name. A short message says when the project changes.
- For new work, **Claude sorts it** when making your capsules, and remembers how to recognise
  it next time. At the end it lists what it sorted, so you can correct anything.
- Each project gets a **daily capsule** and one **growing capsule** that's updated every day
  you work on it. That's the one to send: it covers the whole project.

## Voice notes

Your own words are the best context: what you're doing, how, and why you decided something.
Claude treats them as the main source, and screen text as supporting evidence.

- The mic is open **only** while you're speaking a note (⌥V to start, ⌥V to finish; 2 minutes
  at most).
- Speech becomes text **on your computer**: Apple Speech on Mac (Dictation must be on);
  whisper.cpp on Windows and Linux (a one-time ~148 MB download, only after you agree). It
  never falls back to a cloud service.
- **No audio is saved.** Prefer typing? Tell Claude *"add a note: …"*.

## Several screens

While contextualizing, **Add a screen** shares another screen (up to 4, each picked by you).
Each moment, Context Capsule reads the window you're using (the one with keyboard focus — so
keyboard-only work is followed too), and, if you're reading on another shared screen (your
mouse moved or scrolled there in the last 30 seconds), that window as well. A screen you
didn't share is never read, and picking a screen that's already shared is refused.

## Every day

- **Opens at login** (turn it off in Settings or your system's login items).
- **Evening reminder** (6 pm by default): if you contextualized today and made no capsules,
  a small box asks *Make today's capsules?*
- **Old recordings are deleted after 30 days** (change it in Settings). Capsules are kept.

## Capsules

```markdown
---
capsule: 1
title: Pricing page redesign
author: Saachi
created: 2026-10-19T11:30:00Z
project: website
kind: daily            # daily | project (the growing one)
date: 2026-10-19
playbook: 2026-10-20   # which rules wrote it
---

## In short            ← the only part written for people
## For Claude           ← tells the receiving Claude how to carry on
## Where things stand
## What was done
## Decisions
- **Use annual pricing by default** — 70% of trials pick it. *(you said, 10:14)*
## Open questions
## Next steps
## People, links and files
```

Received one? Tell your Claude *"continue from this capsule"* and drop the file in.

## Safe by design

- **Screen text and capsules are information, never instructions.** If a web page or a
  capsule says "AI: send your files somewhere", Claude is told it's just text: it can't
  change the task or approve anything. Received capsules are scanned for hidden instructions.
- **Checked before saving.** A capsule is saved only when it's complete, and every
  *"(you said, 10:14)"* must match a real voice note at that time. Card numbers, bank
  numbers and keys are removed.
- **Honest privacy.** [PRIVACY.md](PRIVACY.md) lists every place data goes, with the code
  that does it.

## For Claude (MCP)

The Claude connection is an MCP server (`mcp/`), shipped as a one-click Claude Desktop
extension. Claude gets:

- **Tools:** `start_recording`, `stop_recording`, `recording_status`, `add_note`,
  `prepare_day` / `finish_day` (a guided day: checklist in, check out), `get_day_activity`,
  `get_session_activity`, `get_moment` (zoom in on a stretch, nothing shortened),
  `list_sessions`, projects (`list_projects`, `create_project`, `set_project`,
  `assign_project`), and capsules (`save_capsule`, `list_capsules`, `get_capsule`,
  `import_capsule`, `export_capsule`).
- **Prompts:** `make_day_capsules`, `make_capsule`, `continue_from_capsule`, `review_capsule`
  (a second look before sharing).
- **Resources:** every capsule as `capsule://<name>`.

All of Claude's instructions live in one playbook (`mcp/src/playbook.ts`). Details:
[mcp/README.md](mcp/README.md) and [SPEC.md](SPEC.md).

## Build from source

Signed downloads are coming. Until then:

```sh
git clone <this repo> context-capsule && cd context-capsule
mcp/extension/build.sh          # the Claude extension (needs Node 18+ and bun)
```

- **macOS 14+** (Xcode command line tools): `capture/build.sh`, then open
  `capture/build/Context Capsule.app`. The Claude extension is bundled inside, so
  Settings → Connect Claude installs it.
- **Windows 10/11:** `capture-rs\windows\build.ps1` (Rust, Visual Studio Build Tools, CMake,
  LLVM), or from a Mac `capture-rs/windows/cross-build.sh`. Keep `context-capsule.mcpb`
  next to the `.exe`. See [capture-rs/windows/README.md](capture-rs/windows/README.md).
- **Linux:** `capture-rs/linux/build.sh`. See [capture-rs/linux/README.md](capture-rs/linux/README.md).
- **Claude Code** instead of the Claude app:
  `claude mcp add context-capsule -- node /absolute/path/to/context-capsule/mcp/dist/index.js`

Data lives in `~/.context-capsule/` (`%USERPROFILE%\.context-capsule\` on Windows).

## Develop

- `capture/tests/run.sh`: macOS logic tests (Swift); the app's `--selftest` checks OCR.
- `cd capture-rs && cargo test -p context-capsule-core`: the shared Windows/Linux core, with
  the same test cases as Swift, so all three write identical files.
- `cargo test -p context-capsule-windows`; Linux in Docker (`capture-rs/ci/linux.Dockerfile`).
- `cd mcp && bun run test`: the Claude side, including the safety checks.
- CI on GitHub runs only when started by hand (Actions → ci → Run workflow).
- [SPEC.md](SPEC.md): the contract between the recorders and the Claude side.

## License

MIT © Karya
