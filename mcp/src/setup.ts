// context-capsule — mcp: `setup` and `remove`, the one-line install: connect every AI app found on this computer, or
// take Context Capsule out of them again. Words for a person in a terminal, unlike the JSON commands the apps use.
import fs from "node:fs";
import path from "node:path";
import { CLIENTS, connect, defaultCtx, disconnect, listClients, selfCommand, type Ctx, type SelfCommand } from "./clients.js";

export interface SetupResult {
  ok: boolean;
  lines: string[];
}

const claudeCodeHere = (c: Ctx) => fs.existsSync(path.join(c.home, ".claude")) || fs.existsSync(path.join(c.home, ".claude.json"));
const quoted = (s: string) => (/\s/.test(s) ? `"${s}"` : s);

export const RECORD_NOTE = "To record your work you also need the Context Capsule app: https://github.com/The-Karya/context-capsule/releases/latest";
export const CLAUDE_STEPS = "Claude app: in the Context Capsule app, open Settings → Connect your AI and click Connect next to Claude.";

/** Connect the apps named, or every app found. Claude's two kinds can't be connected from here, so say how. */
export function setup(o: { ids?: string[]; ctx?: Ctx; self?: SelfCommand } = {}): SetupResult {
  const c = o.ctx ?? defaultCtx();
  const self = o.self ?? selfCommand();
  const found = listClients(c);
  const named = !!o.ids?.length;
  const ids = named ? o.ids! : found.filter((a) => a.installed && a.how === "config").map((a) => a.id);
  const lines: string[] = [];
  let ok = true;
  for (const id of ids) {
    const r = connect(id, { ctx: c, self: o.self });
    if (r.how === "extension") continue; // the steps below
    ok &&= r.ok;
    lines.push(`${r.ok ? "✓" : "✗"} ${r.message}`);
  }
  const wantsClaude = named ? ids.some((id) => id.trim().toLowerCase() === "claude") : found.some((a) => a.id === "claude" && a.installed && !a.connected);
  if (wantsClaude) lines.push(`→ ${CLAUDE_STEPS}`);
  if (!named && claudeCodeHere(c)) lines.push(`→ Claude Code: run  claude mcp add --scope user context-capsule -- ${[self.command, ...self.args].map(quoted).join(" ")}`);
  if (!lines.length) {
    ok = false;
    lines.push(`No AI apps found on this computer. Known: ${CLIENTS.map((d) => d.id).join(", ")}.`);
  }
  lines.push("", RECORD_NOTE);
  return { ok, lines };
}

/** Take Context Capsule out of the apps named, or out of all of them; other settings stay. */
export function remove(o: { ids?: string[]; ctx?: Ctx } = {}): SetupResult {
  const c = o.ctx ?? defaultCtx();
  const named = !!o.ids?.length;
  const ids = named ? o.ids! : CLIENTS.filter((d) => d.how === "config").map((d) => d.id);
  const lines: string[] = [];
  let ok = true;
  for (const id of ids) {
    const r = disconnect(id, { ctx: c });
    if (!r.ok) ok = r.how === "extension" ? ok : false;
    if (r.how === "extension") lines.push(`→ ${r.message}`);
    else if (!r.ok) lines.push(`✗ ${r.message}`);
    else if (named || !r.message.includes("wasn't connected")) lines.push(`✓ ${r.message}`);
  }
  if (!named) {
    if (listClients(c).find((a) => a.id === "claude")?.connected) lines.push("→ Claude app: remove it in Settings → Extensions.");
    if (claudeCodeHere(c)) lines.push("→ Claude Code, if you added it: run  claude mcp remove context-capsule");
  }
  if (!lines.length) lines.push("Context Capsule wasn't connected to any app.");
  return { ok, lines };
}
