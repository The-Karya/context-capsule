# Karya Context Capsule

<img src="site/icon.png" alt="" width="64" height="64">

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

**Download it** from the download page (`site/index.html`): Mac, Windows or Linux. On a Mac,
open the file and drag Karya Context Capsule to Applications. It updates itself: new versions are
signed and checked, and install when you click **Restart now**.

1. **Open Context Capsule.** It lives in your menu bar (Windows: system tray) and opens by
   itself at login.
2. **Settings** opens the first time. Turn each step on, and it gets a ✓:
   - **Read your apps** (Mac): cleaner text, and window names, so projects are found by themselves.
   - **Voice notes** and **Dictation**, so you can talk.
   - **Connect your AI**: click Connect next to Claude, Cursor, VS Code… (Claude asks you to install
     Context Capsule). Any other AI, like ChatGPT or Gemini on the web, needs nothing: use **Copy my day**.
   - **Open at login**.
3. **Start contextualizing** (⌥C) and pick your screen. The menu bar shows a red dot and a timer.
4. Work. Any time, **Speak a note** (⌥V): say what you're doing and why. The menu bar shows a
   mic with bars that move with your voice. Press ⌥V again when done.
5. At the end of the day, click **Make today's capsules**. Your AI opens with the request (Claude: typed in,
   press Return; other apps: paste it in a new chat). Your AI sorts your day into projects and writes the capsules.
   With a website or any other AI: **Copy my day**, paste it in, copy its whole reply, then **Save from clipboard**.
   Or let it happen by itself: Settings → **Make capsules** → **Automatically**, at a time you pick. That needs
   Claude Code, Codex or Gemini CLI on your computer; it runs in the background with only Context Capsule's tools
   (no files, no commands, no web), and a notification says when your capsules are ready.
6. **Share** a capsule from **Capsules**: copy it into WhatsApp, Slack or email, or AirDrop it.

## The Context Capsule window

Everything in one place (menu or tray → **Open Context Capsule**), the same on Mac, Windows and
Linux. Light, dark, or the same as your system.

- **Home**: a hello with your name, Start / Stop contextualizing with a live timer, Speak a
  note, Add a screen; today, this week and your streak; time per project today; your recent
  capsules; your shortcuts, the project you're on, and Make today's capsules. It fits any
  window size.
- **Timeline**: any day's recordings and where your time went; open a recording to see what
  was read, or delete it.
- **Capsules**: search your capsules, by day or by project. Open one to read it, continue in
  your AI, or **Share…**: the share page shows the copy as it will be sent, with private things
  (names, emails, phones, private links, money and more) struck through where they are, and a
  switch for each kind. Then Copy text, Copy file, Save a copy, or AirDrop / Mail / Messages.
- **Projects**: rename them, change how they're found, delete them.
- **Settings**: your name (it goes on your capsules), set up, appearance, updates, the evening reminder and its time, when to delete old recordings,
  sounds, the voice-note language, apps never to record, and updates.
- **Help**: shortcuts, how it works, privacy.

The menu-bar menu is the quick version: Start contextualizing · Speak a note · Add a screen ·
Project · Open Context Capsule · Make today's capsules · Check for Updates · Quit.

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

Want every screen? **Settings → Screens → When you start → All my screens**: starting shares all of them
at once, with no picker (the Mac asks once for Screen Recording; on Linux you confirm in its dialog).
Or, while contextualizing, **Add a screen** shares another screen (up to 4, each picked by you).
Each moment, Context Capsule reads the window you're using (the one with keyboard focus — so
keyboard-only work is followed too), and, if you're reading on another shared screen (your
mouse moved or scrolled there in the last 30 seconds), that window as well. A screen you
didn't share is never read, and picking a screen that's already shared is refused.

## Every day

- **Opens at login** (turn it off in Settings or your system's login items).
- **Capsules made automatically** if you choose (6 pm by default): if the computer was asleep or off then, it
  happens when it wakes, the same day. Once a day at most, and only on days you contextualized.
- **Evening reminder** otherwise (6 pm by default): if you contextualized today and made no capsules,
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

No AI at hand? Your Claude can save any capsule as a web page (`export_capsule` with
`format: "html"`, or `context-capsule-mcp view <file>`) that opens in any browser and prints
cleanly. Got back a newer version of a capsule you sent? Claude compares the two
(`compare_capsules`): what changed, section by section, and one merged capsule, with
decisions that differ set apart for you to settle.

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
  `import_capsule`, `export_capsule` (as a capsule or a web page), `compare_capsules` (two
  versions: what changed, and the merge), `search_capsules` (answers "when did I decide…"
  from every capsule, with citations)).
- **Prompts:** `make_day_capsules`, `make_capsule`, `continue_from_capsule`, `review_capsule`
  (a second look before sharing), `ask_capsules`.
- **Resources:** every capsule as `capsule://<name>`.

All of Claude's instructions live in one playbook (`mcp/src/playbook.ts`). Details:
[mcp/README.md](mcp/README.md) and [SPEC.md](SPEC.md).

## Build from source

Most people should use the download page instead. To build it yourself:

```sh
git clone <this repo> context-capsule && cd context-capsule
mcp/extension/build.sh          # the Claude extension (needs Node 18+ and bun)
mcp/build-binary.sh macos-arm64 # Mac only: the helper behind Settings → Connect your AI (macos-x64 on Intel)
```

- **macOS 14+** (Xcode command line tools): `capture/build.sh`, then open
  `capture/build/Context Capsule.app`. The Claude extension and `context-capsule-mcp` (from
  `mcp/dist-bin/`, when built) are bundled inside, so Settings → Connect your AI works.
  `capture/package.sh` makes the disk image and zip.
- **Windows 10/11:** `capture-rs\windows\build.ps1` (Rust, Visual Studio Build Tools, CMake,
  LLVM), or from a Mac `capture-rs/windows/cross-build.sh`. Keep `context-capsule.mcpb`
  next to the `.exe`. See [capture-rs/windows/README.md](capture-rs/windows/README.md).
- **Linux:** `capture-rs/linux/build.sh`. See [capture-rs/linux/README.md](capture-rs/linux/README.md).
- **Claude Code** instead of the Claude app:
  `claude mcp add context-capsule -- node /absolute/path/to/context-capsule/mcp/dist/index.js`

Data lives in `~/.context-capsule/` (`%USERPROFILE%\.context-capsule\` on Windows).
Making a release (version bump, builds, the signed update feed): [release/README.md](release/README.md).

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
