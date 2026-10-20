# Privacy: what leaves your computer, and when

Context Capsule records on your computer. It has no account, no server, no cloud and no
analytics. Here is every place your data goes, and the code that makes it so.

| What | Where it goes | Who sees it | When | Where to check |
|---|---|---|---|---|
| Screen text (from apps or OCR) | `~/.context-capsule/sessions/` on your computer | Only you | While recording | `capture/Sources/Recorder.swift`, `capture-rs/core/src/engine.rs` |
| Screenshots | Nowhere: the latest frame is kept in memory only, then dropped | Nobody | — | `capture/Sources/Picker.swift` (`latestFrame`), never written |
| Your voice | Nowhere: the mic feeds on-device speech recognition in memory; only the text is saved | Nobody | While you hold a voice note | `capture/Sources/Voice.swift` (`requiresOnDeviceRecognition`) |
| Voice-note text, typed notes | Your session files | Only you | When saved | `events.jsonl` (`"kind":"voice"` / `"note"`) |
| A timeline or capsule your Claude reads | **Your Claude conversation**, so to Anthropic, under your own Claude plan and its terms | Claude | Only when you ask Claude to make, read or continue a capsule | `mcp/src/server.ts` (the tools return text to Claude) |
| A capsule you export | The folder you choose (Desktop by default); then wherever *you* send it | Whoever you send it to | Only when you ask | `mcp/src/capsules.ts` (`exportCapsule`) |
| The speech model (Windows/Linux) | Downloaded once from huggingface.co after you say yes; nothing is uploaded | — | Once | `capture-rs/*/src/model.rs` |

## How long it's kept

Raw recordings are deleted after 30 days (change or turn off with `keep_recordings_days` in
`config.json`); capsules stay until you delete them. Delete any recording folder in
`~/.context-capsule/sessions/` and it's gone.

## Never recorded

Password managers, WhatsApp, Signal, the lock screen, password fields, and any app you
add to `exclude_apps`. Their windows are skipped, and blacked out when they sit on top of
what you're recording. Keys, tokens and passwords are removed from screen text before it's
saved (`Redactor` in `capture/Sources/Core.swift`); card numbers, bank numbers (IBAN) and
private keys are also removed when a capsule is saved (`redactSecrets` in `mcp/src/capsules.ts`).

## Safe to receive

A capsule from someone else is treated as **information, never instructions**: your Claude
is told it can't use a capsule (or recorded screen text) to send, share, delete, open links
or run anything without asking you. Imported capsules are scanned for text aimed at an AI,
and you're told if any is found (`mcp/src/untrusted.ts`).

## Not verified here

- What Anthropic keeps from your Claude conversations depends on your Claude plan and
  settings, not on Context Capsule.
- Apple's on-device speech recognition needs Dictation turned on; Apple's own Dictation
  settings apply to Dictation, not to Context Capsule.
