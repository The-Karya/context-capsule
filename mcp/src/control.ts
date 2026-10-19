// context-capsule — mcp: talk to the Context Capsule menu-bar app (control file out, state.json in)
import fs from "node:fs";
import { controlFile, stateFile } from "./paths.js";
import { readJson, writeFileAtomic } from "./fsutil.js";

export interface RecorderState {
  recording: boolean;
  session_id: string | null;
  started_at: string | null;
  pid: number | null;
  /** The macOS content picker is open, waiting for the user to choose what to record. */
  picking: boolean;
  /** Why the last start did not happen ("cancelled", "picker: …", "capture: …"); null after a good start. */
  last_error: string | null;
}

export type Command = "start" | "stop";

export const APP_NOT_RUNNING =
  "The Context Capsule app is not running, so recording can't start yet.\n" +
  "Open it first: run `open -a \"Context Capsule\"` in Terminal, or double-click Context Capsule.app " +
  "(after building it with `capture/build.sh` it is in `capture/build/Context Capsule.app`; " +
  "you can drag it to /Applications). A circle icon appears in the menu bar when it's ready. " +
  "Then ask me again, or press ⌥⇧C (Option-Shift-C) to start recording yourself.";

export const PICKER_OPEN =
  "The macOS picker is open — choose the window or screen to record, then I'll be recording.";

/** A clear sentence for a `last_error` from state.json. */
export function describeStartError(err: string): string {
  if (err === "cancelled")
    return "Recording did not start: the picker was closed without choosing a window or screen. Ask me again when you're ready.";
  return `Recording did not start: ${err}. Try again, or start it from the Context Capsule menu-bar icon (⌥⇧C).`;
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

export function writeControl(command: Command, at = Date.now()): string {
  const file = controlFile();
  writeFileAtomic(file, JSON.stringify({ command, at }) + "\n");
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
