// context-capsule — mcp: talk to the Context Capsule menu-bar app (control file out, state.json in)
import fs from "node:fs";
import { controlFile, stateFile } from "./paths.js";
import { readJson, writeFileAtomic } from "./fsutil.js";
import { words } from "./platform.js";

export interface RecorderState {
  recording: boolean;
  session_id: string | null;
  started_at: string | null;
  pid: number | null;
  /** The system picker is open, waiting for the user to choose what to record. */
  picking: boolean;
  /** Why the last start did not happen ("cancelled", "picker: …", "capture: …"); null after a good start. */
  last_error: string | null;
  /** Events written in the current (or just-ended) session; null if the app is older. */
  events: number | null;
  /** Why the latest sample wrote nothing (e.g. "not_picked"); null if it wrote. */
  skip: string | null;
  /** A voice note is being spoken right now. */
  listening: boolean;
  /** `at` of the last typed note the app wrote (confirms add_note). */
  note_at: number | null;
}

const SKIP_REASONS: Record<string, string> = {
  private_app: "a private app (password manager, WhatsApp, Signal…) is in front, so nothing is read",
  no_window: "the front app has no window open",
  not_picked: "the window in front is not part of the window or screen you picked",
  overlay: "Mission Control is open",
  private_window: "a private app's window covers the window in front",
  waiting_frame: "waiting for the screen to update after switching apps",
  unknown_display: "the app can't tell where the picked screen is",
};

/** Plain words for a `skip` from state.json. */
export function describeSkip(skip: string): string {
  return SKIP_REASONS[skip] ?? skip;
}

export type Command = "start" | "stop" | "note";

export function appNotRunning(): string {
  const w = words();
  return (
    "The Context Capsule app is not running, so recording can't start yet.\n" +
    `Open it first: ${w.openApp} ` +
    `Then ask me again, or press ${w.recordKey} to start recording yourself.`
  );
}

export function pickerOpen(): string {
  return `${cap(words().picker)} is open — choose the window or screen to record, then I'll be recording.`;
}

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

/** A clear sentence for a `last_error` from state.json. */
export function describeStartError(err: string): string {
  if (err === "cancelled")
    return "Recording did not start: the picker was closed without choosing a window or screen. Ask me again when you're ready.";
  return `Recording did not start: ${err}. Try again, or start it from the Context Capsule icon in the ${words().tray} (${words().recordKeyShort}).`;
}

/** Read state.json; null if the app never wrote one. */
export function readState(): RecorderState | null {
  const s = readJson<Partial<RecorderState>>(stateFile());
  if (!s || typeof s !== "object") return null;
  return {
    recording: s.recording === true,
    session_id: typeof s.session_id === "string" ? s.session_id : null,
    started_at: typeof s.started_at === "string" ? s.started_at : null,
    pid: typeof s.pid === "number" && Number.isInteger(s.pid) ? s.pid : null,
    picking: s.picking === true,
    last_error: typeof s.last_error === "string" && s.last_error ? s.last_error : null,
    events: typeof s.events === "number" && Number.isInteger(s.events) ? s.events : null,
    skip: typeof s.skip === "string" && s.skip ? s.skip : null,
    listening: s.listening === true,
    note_at: typeof s.note_at === "number" ? s.note_at : null,
  };
}

/** Is a process with this pid alive? EPERM still means it exists. */
export function pidAlive(pid: number | null): boolean {
  if (!pid || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    return (e as NodeJS.ErrnoException).code === "EPERM";
  }
}

export function appRunning(state: RecorderState | null = readState()): boolean {
  return !!state && pidAlive(state.pid);
}

/** `text` is only for "note": a typed note for the current session. */
export function writeControl(command: Command, at = Date.now(), text?: string): string {
  const file = controlFile();
  writeFileAtomic(file, JSON.stringify(text === undefined ? { command, at } : { command, text, at }) + "\n");
  return file;
}

/** Poll state.json until `done(state)` or timeout. Returns the last state seen. */
export async function waitForState(
  done: (s: RecorderState) => boolean,
  timeoutMs: number,
  intervalMs = 200,
): Promise<{ state: RecorderState | null; confirmed: boolean }> {
  const until = Date.now() + timeoutMs;
  let state = readState();
  while (true) {
    if (state && done(state)) return { state, confirmed: true };
    if (Date.now() >= until) return { state, confirmed: false };
    await new Promise((r) => setTimeout(r, intervalMs));
    state = readState();
  }
}

export type StartOutcome =
  | { kind: "recording"; state: RecorderState }
  | { kind: "picking"; state: RecorderState }
  | { kind: "error"; state: RecorderState; error: string }
  | { kind: "timeout"; state: RecorderState | null };

function stateStamp(): string {
  try {
    const st = fs.statSync(stateFile());
    return `${st.mtimeMs}:${st.size}:${fs.readFileSync(stateFile(), "utf8")}`;
  } catch {
    return "";
  }
}

/**
 * Write the start command, then wait for the app: the picker opened, recording began, or it
 * reported an error. A `last_error` left over from before the command is ignored until
 * state.json changes (the app clears it on every start).
 */
export async function startAndWait(timeoutMs: number, intervalMs = 200): Promise<StartOutcome> {
  const before = stateStamp();
  writeControl("start");
  let fresh = false;
  const { state, confirmed } = await waitForState(
    (s) => {
      if (!fresh && stateStamp() !== before) fresh = true;
      return s.recording || s.picking || (fresh && !!s.last_error);
    },
    timeoutMs,
    intervalMs,
  );
  if (!confirmed || !state) return { kind: "timeout", state };
  if (state.recording) return { kind: "recording", state };
  if (state.picking) return { kind: "picking", state };
  return { kind: "error", state, error: state.last_error! };
}
