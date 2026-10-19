// context-capsule — mcp: where the data lives (CONTEXT_CAPSULE_HOME, default ~/.context-capsule)
import os from "node:os";
import path from "node:path";

/** The user's home dir ($HOME first, so it can be overridden). */
export function homeDir(): string {
  return process.env.HOME?.trim() || os.homedir();
}

/** Root data dir. Read on every call so tests can switch it at runtime. */
export function capsuleHome(): string {
  const env = process.env.CONTEXT_CAPSULE_HOME;
  if (env && env.trim()) return path.resolve(expandHome(env.trim()));
  return path.join(homeDir(), ".context-capsule");
}

export const sessionsDir = () => path.join(capsuleHome(), "sessions");
export const capsulesDir = () => path.join(capsuleHome(), "capsules");
export const controlFile = () => path.join(capsuleHome(), "control");
export const stateFile = () => path.join(capsuleHome(), "state.json");
export const configFile = () => path.join(capsuleHome(), "config.json");

/** Turn a leading "~" into the home dir. */
export function expandHome(p: string): string {
  if (p === "~") return homeDir();
  if (p.startsWith("~/")) return path.join(homeDir(), p.slice(2));
  return p;
}

/** Session ids are folder names like 20261019-100103; refuse anything path-like. */
export function isSafeName(name: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(name) && !name.includes("..");
}
