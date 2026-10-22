# Privacy: what leaves your computer, and when

Context Capsule records on your computer. It has no account, no server, no cloud and no
analytics. Its only network use is the update check below (and the one-time speech model
download on Windows/Linux). Here is every place your data goes, and the code that makes it so.

| What | Where it goes | Who sees it | When | Where to check |
|---|---|---|---|---|
| Screen text (from apps or OCR) | `~/.context-capsule/sessions/` on your computer | Only you | While recording | `capture/Sources/Recorder.swift`, `capture-rs/core/src/engine.rs` |
| Screenshots | Nowhere: the latest frame is kept in memory only, then dropped | Nobody | — | `capture/Sources/Picker.swift` (`latestFrame`), never written |
| Your voice | Nowhere: the mic feeds on-device speech recognition in memory; only the text is saved | Nobody | While you hold a voice note | `capture/Sources/Voice.swift` (`requiresOnDeviceRecognition`) |
| Voice-note text, typed notes | Your session files | Only you | When saved | `events.jsonl` (`"kind":"voice"` / `"note"`) |
| When something wasn't read, and why (times and a reason only, no text) | Your session files | Only you | While recording | `gaps.jsonl` |
| A timeline or capsule your AI reads | **Your AI conversation**, so to that AI's company (Anthropic for Claude, OpenAI for ChatGPT…), under your own plan and its terms | Your AI | Only when you ask your AI to make, read or continue a capsule | `mcp/src/server.ts` (the tools return text to your AI) |
| Connecting an AI app (Settings → Connect your AI, `context-capsule-mcp connect`) | That AI app's own settings file on this computer (a `.bak` of it is kept next to it); nothing is sent | — | Only when you click Connect | `mcp/src/clients.ts` |
| Copy my day / Save from clipboard | Your clipboard; then only into the AI chat *you* paste it in, under that AI's terms | That AI | Only when you paste it | `mcp/src/anyai.ts` |
| `serve --http` (for AI apps that only take an address) | Nowhere outside this computer: it listens on 127.0.0.1 only and needs the token in `mcp-token` | Apps on this computer that have the token | Only when started | `mcp/src/http.ts` |
| A capsule you export | The folder you choose (Desktop by default); then wherever *you* send it | Whoever you send it to | Only when you ask | `mcp/src/capsules.ts` (`exportCapsule`) |
| Update check | Nothing is sent about you. The app downloads `latest.json` and its signature from the Context Capsule releases on github.com, at launch and once a day, and, when there is a new version, that version's file. GitHub sees a normal web request (your IP address), as with any download. For staged updates the app keeps a random number 0–99 in `.update-bucket`; it never leaves your computer | — | Daily, or when you click Check for Updates; turn off "Update automatically" in Settings | `capture/Sources/Updater.swift`, `capture-rs/` (Windows, Linux) |
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
