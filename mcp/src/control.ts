// karya-capsule — mcp: talk to the Capsule menu-bar app (control file out, state.json in)
import { controlFile, stateFile } from "./paths.js";
import { readJson, writeFileAtomic } from "./fsutil.js";

export interface RecorderState {
  recording: boolean;
  session_id: string | null;
  started_at: string | null;
  pid: number | null;
}

export type Command = "start" | "stop";

export const APP_NOT_RUNNING =
  "The Capsule app is not running, so recording can't start yet.\n" +
  "Open it first: run `open -a Capsule` in Terminal, or double-click Capsule.app " +
  "(after building it with `capture/build.sh` it is in `capture/build/Capsule.app`; " +
  "you can drag it to /Applications). A circle icon appears in the menu bar when it's ready. " +
  "Then ask me again, or press ⌥⇧C (Option-Shift-C) to start recording yourself.";

/** Read state.json; null if the app never wrote one. */
export function readState(): RecorderState | null {
  const s = readJson<Partial<RecorderState>>(stateFile());
  if (!s || typeof s !== "object") return null;
  return {
    recording: s.recording === true,
    session_id: typeof s.session_id === "string" ? s.session_id : null,
    started_at: typeof s.started_at === "string" ? s.started_at : null,
    pid: typeof s.pid === "number" && Number.isInteger(s.pid) ? s.pid : null,
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
