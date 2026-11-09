// context-capsule — mcp: Automatic capsules (SPEC "Automatic capsules"). Finds a local AI command-line tool
// (Claude Code, Codex, Gemini CLI), runs it headless with ONLY Context Capsule's MCP tools (no shell, no file
// edits, no web), with the make_day_capsules request, a time limit and low priority; then writes the outcome to
// auto.json in the data folder (one source of truth for every app: what ran, when, and how it went).
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { localDay } from "./activity.js";
import { CAPSULE_EXT, logLine } from "./capsules.js";
import { selfCommand, type SelfCommand } from "./clients.js";
import { writeFileAtomic } from "./fsutil.js";
import { capsuleHome, capsulesDir, configFile, homeDir, sessionsDir } from "./paths.js";
import { currentOS, type OS } from "./platform.js";
import { makeDayCapsulesPrompt } from "./playbook.js";

export type AgentID = "claude-code" | "codex" | "gemini";

export interface Agent {
  id: AgentID;
  name: string;
  /** The program found on this computer. */
  path: string;
}

/** In order of preference (Your AI moves its own to the front). */
export const AGENTS: { id: AgentID; name: string; bin: string }[] = [
  { id: "claude-code", name: "Claude Code", bin: "claude" },
  { id: "codex", name: "Codex", bin: "codex" },
  { id: "gemini", name: "Gemini CLI", bin: "gemini" },
];

/** ISO time without milliseconds, like every other file of the app ("2026-10-23T17:00:04Z"). */
const isoSeconds = (d: Date) => d.toISOString().replace(/\.\d{3}Z$/, "Z");

/** A run may take this long, then it is stopped. */
export const TIME_LIMIT_MS = 10 * 60 * 1000;
/** A "running" entry older than this is a run that was cut off (the computer slept, the app quit…). */
export const STALE_MS = TIME_LIMIT_MS + 5 * 60 * 1000;
export const MCP_NAME = "context-capsule";

// ---------- finding the tools ----------

export interface FindCtx {
  os: OS;
  home: string;
  env: Record<string, string | undefined>;
}

export function defaultFindCtx(): FindCtx {
  return { os: currentOS(), home: homeDir(), env: process.env };
}

/** Where these tools usually live. Apps started from the Dock / Start menu don't get the shell's PATH, so the
 * usual install folders are searched too (the official installers, Homebrew, npm, bun, volta, nvm). */
export function searchDirs(c: FindCtx): string[] {
  const j = (...p: string[]) => path.join(c.home, ...p);
  const fromPath = (c.env.PATH ?? c.env.Path ?? "").split(c.os === "windows" ? ";" : ":").filter(Boolean);
  const dirs: string[] = [...fromPath];
  if (c.os === "windows") {
    const appData = c.env.APPDATA?.trim() || j("AppData", "Roaming");
    const local = c.env.LOCALAPPDATA?.trim() || j("AppData", "Local");
    dirs.push(j(".local", "bin"), path.join(appData, "npm"), j(".bun", "bin"), path.join(local, "Volta", "bin"), path.join(local, "Programs", "claude"));
  } else {
    dirs.push(j(".local", "bin"), j(".claude", "local"), "/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", j(".npm-global", "bin"), j(".bun", "bin"), j(".volta", "bin"), j("bin"));
    // nvm: every installed Node's bin, newest name last (globals live with the Node they were installed with)
    try {
      const nvm = j(".nvm", "versions", "node");
      for (const v of fs.readdirSync(nvm).sort()) dirs.push(path.join(nvm, v, "bin"));
    } catch {
      // no nvm
    }
  }
  return [...new Set(dirs)];
}

const isFile = (p: string) => {
  try {
    return fs.statSync(p).isFile();
  } catch {
    return false;
  }
};

/** The first `bin` in the search folders (Windows: .exe first, then .cmd). */
export function findProgram(bin: string, c: FindCtx = defaultFindCtx()): string | null {
  const exts = c.os === "windows" ? [".exe", ".cmd"] : [""];
  for (const dir of searchDirs(c)) {
    for (const ext of exts) {
      const p = path.join(dir, bin + ext);
      if (isFile(p)) return p;
    }
  }
  return null;
}

/** The tools installed here, in preference order. */
export function findAgents(c: FindCtx = defaultFindCtx()): Agent[] {
  return AGENTS.flatMap((a) => {
    const p = findProgram(a.bin, c);
    return p ? [{ id: a.id, name: a.name, path: p }] : [];
  });
}

/** Which one to use: the one matching Your AI (config "ai": codex / gemini / claude) if installed, else the first. */
export function pickAgent(agents: Agent[], yourAI?: unknown): Agent | null {
  const want: AgentID | null = yourAI === "codex" || yourAI === "chatgpt" ? "codex" : yourAI === "gemini" ? "gemini" : yourAI === "claude" ? "claude-code" : null;
  return agents.find((a) => a.id === want) ?? agents[0] ?? null;
}

// ---------- the restricted command ----------

export interface Command {
  command: string;
  args: string[];
  /** Files to write before running (settings for the tool), relative to nothing: absolute paths. */
  files: { path: string; text: string }[];
  /** Extra environment for the tool. */
  env: Record<string, string>;
}

export interface CommandOpts {
  prompt: string;
  /** How the tool starts our MCP server. */
  self: SelfCommand;
  /** The data folder (passed to our MCP server). */
  home: string;
  /** An empty folder the tool runs in (no project files, no CLAUDE.md / AGENTS.md / GEMINI.md). */
  work: string;
}

/** Claude Code's tool names for our server. */
export const CLAUDE_ALLOWED = `mcp__${MCP_NAME}__*`;

/** Codex features that give the model a way out of the MCP tools (shell, browser, computer use, apps, plugins…). */
export const CODEX_OFF = [
  "shell_tool",
  "unified_exec",
  "apps",
  "plugins",
  "browser_use",
  "browser_use_external",
  "computer_use",
  "in_app_browser",
  "image_generation",
  "multi_agent",
  "multi_agent_v2",
  "hooks",
  "view_image",
  "goals",
  "tool_suggest",
  "skill_search",
  "workspace_dependencies",
  "memories",
];

/** The command line for one tool, restricted to Context Capsule's MCP tools. */
export function agentCommand(agent: Agent, o: CommandOpts): Command {
  const server = { command: o.self.command, args: o.self.args, env: { CONTEXT_CAPSULE_HOME: o.home } };
  switch (agent.id) {
    case "claude-code": {
      const config = path.join(o.work, "mcp.json");
      return {
        command: agent.path,
        args: [
          "-p",
          o.prompt,
          // only our MCP server, none of the user's others
          "--strict-mcp-config",
          "--mcp-config",
          config,
          // no built-in tools at all: no Bash, no file reading or editing, no WebFetch / WebSearch
          "--tools",
          "",
          // our tools run without asking; anything else is refused, never asked about (nobody is there)
          "--allowedTools",
          CLAUDE_ALLOWED,
          "--permission-mode",
          "dontAsk",
          "--disable-slash-commands",
          "--no-session-persistence",
          "--output-format",
          "json",
        ],
        files: [{ path: config, text: JSON.stringify({ mcpServers: { [MCP_NAME]: { type: "stdio", ...server } } }, null, 2) }],
        env: {},
      };
    }
    case "codex": {
      // TOML values without quotes (a bare path isn't TOML, so Codex takes it as text): nothing for a shell to mangle
      return {
        command: agent.path,
        args: [
          "exec",
          // none of the user's settings: no other MCP servers, profiles or rules (sign-in still works)
          "--ignore-user-config",
          "--ignore-rules",
          "--skip-git-repo-check",
          "--ephemeral",
          // commands may not write or reach the network, and are never approved
          "--sandbox",
          "read-only",
          "-c",
          "approval_policy=never",
          "-c",
          "web_search=disabled",
          "-c",
          "include_apply_patch_tool=false",
          ...CODEX_OFF.flatMap((f) => ["--disable", f]),
          "-c",
          `mcp_servers.${MCP_NAME}.command=${server.command}`,
          "-c",
          `mcp_servers.${MCP_NAME}.args=[${server.args.map((a) => `'${a}'`).join(", ")}]`,
          "-c",
          `mcp_servers.${MCP_NAME}.env.CONTEXT_CAPSULE_HOME=${o.home}`,
          "-c",
          `mcp_servers.${MCP_NAME}.default_tools_approval_mode=approve`,
          "--cd",
          o.work,
          o.prompt,
        ],
        files: [],
        env: {},
      };
    }
    case "gemini": {
      // Settings of the folder it runs in (trusted for this run): our server only, and no built-in tools at all
      // (an empty tools.core registers none). A policy that refuses every tool but ours, on top.
      const settings = path.join(o.work, ".gemini", "settings.json");
      const policy = path.join(o.work, "gemini-policy.toml");
      return {
        command: agent.path,
        args: [
          "-p",
          o.prompt,
          "--output-format",
          "json",
          "--approval-mode",
          "default",
          "--extensions",
          "none",
          "--allowed-mcp-server-names",
          MCP_NAME,
          "--policy",
          policy,
          "--admin-policy",
          policy,
        ],
        files: [
          {
            path: settings,
            text: JSON.stringify(
              { mcpServers: { [MCP_NAME]: { ...server, trust: true } }, mcp: { allowed: [MCP_NAME] }, tools: { core: [] }, skills: { enabled: false } },
              null,
              2,
            ),
          },
          { path: policy, text: GEMINI_POLICY },
        ],
        env: { GEMINI_CLI_TRUST_WORKSPACE: "true", GEMINI_SYSTEM_MD: "" },
      };
    }
  }
}

/** Gemini CLI: every tool refused, except Context Capsule's. */
export const GEMINI_POLICY = `# Context Capsule's automatic run: only Context Capsule's tools.
[[rule]]
toolName = "*"
decision = "deny"
priority = 100

[[rule]]
mcpName = "${MCP_NAME}"
decision = "allow"
priority = 200
`;

// ---------- auto.json ----------

export interface Run {
  /** The day the capsules are for, "2026-10-23". */
  day: string;
  by: "auto" | "click";
  status: "running" | "done" | "failed";
  ai: AgentID;
  started_at: string;
  finished_at?: string;
  capsules?: number;
  /** Plain words, never content. */
  reason?: string;
  pid?: number;
}

export interface AutoFile {
  /** The day the automatic run last started (it never starts twice for one day). */
  auto_day?: string;
  last?: Run;
}

export const autoFile = () => path.join(capsuleHome(), "auto.json");

export function readAuto(): AutoFile {
  try {
    const v = JSON.parse(fs.readFileSync(autoFile(), "utf8"));
    return v && typeof v === "object" ? (v as AutoFile) : {};
  } catch {
    return {};
  }
}

function writeAuto(a: AutoFile): void {
  writeFileAtomic(autoFile(), `${JSON.stringify(a, null, 2)}\n`);
}

function alive(pid: number | undefined): boolean {
  if (!pid) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    return (e as NodeJS.ErrnoException).code === "EPERM";
  }
}

/** Is a run going on right now (started less than STALE_MS ago, by a process that's still there)? */
export function isRunning(a: AutoFile, now = Date.now()): boolean {
  const r = a.last;
  if (!r || r.status !== "running") return false;
  const t = Date.parse(r.started_at);
  return Number.isFinite(t) && now - t < STALE_MS && alive(r.pid);
}

// ---------- running it ----------

/** `.capsule.md` names and their change times, to count what a run saved. */
function snapshot(): Map<string, number> {
  const out = new Map<string, number>();
  try {
    for (const f of fs.readdirSync(capsulesDir())) {
      if (!f.endsWith(CAPSULE_EXT)) continue;
      try {
        out.set(f, fs.statSync(path.join(capsulesDir(), f)).mtimeMs);
      } catch {
        // gone meanwhile
      }
    }
  } catch {
    // no capsules yet
  }
  return out;
}

export function changedCount(before: Map<string, number>, after: Map<string, number>): number {
  let n = 0;
  for (const [f, t] of after) if (before.get(f) !== t) n++;
  return n;
}

/** Was anything recorded on this day (a non-empty events.jsonl in a recording started that day)? */
export function recordedOn(day: string): boolean {
  const prefix = day.replace(/-/g, "");
  try {
    return fs.readdirSync(sessionsDir()).some((id) => {
      if (!id.startsWith(prefix)) return false;
      try {
        return fs.statSync(path.join(sessionsDir(), id, "events.jsonl")).size > 0;
      } catch {
        return false;
      }
    });
  } catch {
    return false;
  }
}

/** Why a run failed, as far as can be told for sure. Only exact messages each tool is known to print count;
 * anything else is "error", so a run that failed for another reason is never blamed on the sign-in. */
export type FailureKind = "timeout" | "signed-out" | "limit" | "offline" | "no-capsules" | "error";

/** Each tool's own words (or exit code) for "not signed in" and "out of usage", and nothing broader. */
const SIGNALS: Record<AgentID, { signedOut: RegExp[]; limit: RegExp[]; signedOutCode?: number }> = {
  // Claude Code: "Invalid API key · Please run /login", "Not logged in · Please run /login", "OAuth token has
  // expired", or the API's own {"type":"authentication_error"}.
  "claude-code": {
    signedOut: [/please run \/login/i, /"type"\s*:\s*"authentication_error"/, /oauth token (has expired|revoked)/i, /invalid api key ·/i],
    limit: [/claude ai usage limit reached/i, /credit balance is too low/i, /you've hit your limit/i, /\b\d+-hour limit reached/i, /"type"\s*:\s*"rate_limit_error"/],
  },
  // Codex: "Please log out and sign in again" (its sign-in can't be renewed), "unexpected status 401
  // Unauthorized", or telling you to run `codex login`.
  codex: {
    signedOut: [/please log out and sign in again/i, /unexpected status 401\b/i, /\bcodex login\b/i],
    limit: [/you've hit your usage limit/i, /quota exceeded\. check your plan/i, /unexpected status 429\b/i],
  },
  // Gemini CLI: exits with 41 (its sign-in error) or asks for an auth method before it starts.
  gemini: {
    signedOut: [/please set an auth method/i, /"type"\s*:\s*"FatalAuthenticationError"/],
    signedOutCode: 41,
    limit: [/resource_exhausted/i, /quota exceeded for quota metric/i],
  },
};

/** No internet: the system's own error codes, not just the word "network" somewhere in the output. */
const OFFLINE = [/\bENOTFOUND\b/, /\bEAI_AGAIN\b/, /\bECONNREFUSED\b/, /\bENETUNREACH\b/, /getaddrinfo/];

/** Sort a failed run by its exit code and what it printed (only matched, never kept). */
export function failureKind(agent: AgentID, code: number | null, output: string, timedOut: boolean): FailureKind {
  if (timedOut) return "timeout";
  const s = SIGNALS[agent];
  if ((s.signedOutCode !== undefined && code === s.signedOutCode) || s.signedOut.some((r) => r.test(output))) return "signed-out";
  if (s.limit.some((r) => r.test(output))) return "limit";
  if (OFFLINE.some((r) => r.test(output))) return "offline";
  if (code === 0) return "no-capsules";
  return "error";
}

/** Plain words for a failed run. */
export function failureReason(kind: FailureKind, name: string): string {
  switch (kind) {
    case "timeout":
      return `${name} took longer than 10 minutes, so it was stopped.`;
    case "signed-out":
      return `${name} isn't signed in. Open it once and sign in.`;
    case "limit":
      return `${name} has reached its usage limit. Try again later.`;
    case "offline":
      return `${name} couldn't reach the internet.`;
    case "no-capsules":
      return `${name} didn't save any capsules.`;
    case "error":
      return `Couldn't make the capsules. Try again, or open ${name} to check it's working.`;
  }
}

export interface MakeResult {
  ok: boolean;
  day: string;
  /** "running" (one is already going), "none" (no tool installed), "nothing" (nothing recorded), "done", "failed". */
  status: "running" | "none" | "nothing" | "done" | "failed";
  ai?: AgentID;
  name?: string;
  capsules?: number;
  reason?: string;
}

/** Stop a process and everything it started. */
function killTree(pid: number): void {
  try {
    if (process.platform === "win32") spawn("taskkill", ["/pid", String(pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
    else process.kill(-pid, "SIGKILL");
  } catch {
    // already gone
  }
}

/** cmd.exe runs .cmd files; arguments are quoted, and anything cmd would still read specially is refused. */
export function windowsCmdLine(cmd: Command): { command: string; args: string[] } | null {
  const all = [cmd.command, ...cmd.args];
  if (all.some((a) => /["%\r\n]/.test(a))) return null;
  return { command: process.env.ComSpec || "cmd.exe", args: ["/d", "/s", "/c", `"${all.map((a) => `"${a}"`).join(" ")}"`] };
}

export interface MakeOpts {
  day: string;
  by: "auto" | "click";
  /** Override which tool (tests); else Your AI's match or the first found. */
  agent?: Agent;
  timeLimitMs?: number;
  self?: SelfCommand;
}

/** Make the day's capsules with a local AI tool, headless and restricted. Writes auto.json; logs the outcome. */
export async function make(o: MakeOpts): Promise<MakeResult> {
  const { day, by } = o;
  const state = readAuto();
  if (isRunning(state)) return { ok: false, day, status: "running", reason: "Already making today's capsules." };
  if (!recordedOn(day)) return { ok: false, day, status: "nothing", reason: "Nothing recorded today yet." };
  let yourAI: unknown;
  try {
    yourAI = JSON.parse(fs.readFileSync(configFile(), "utf8"))?.ai;
  } catch {
    // no config
  }
  const agent = o.agent ?? pickAgent(findAgents(), yourAI);
  if (!agent) return { ok: false, day, status: "none", reason: "Needs Claude Code, Codex or Gemini CLI on this computer." };

  const home = capsuleHome();
  const work = path.join(home, "auto");
  fs.mkdirSync(work, { recursive: true });
  const prompt =
    makeDayCapsulesPrompt(day) +
    "\n\nYou are running on your own, with nobody to answer questions: don't ask, finish the job with Context Capsule's tools, then reply briefly.";
  const self = o.self ?? selfCommand();
  // our server with only the tools that write a day's capsules (nothing that records, exports or imports)
  const cmd = agentCommand(agent, { prompt, self: { ...self, args: [...self.args, "--make-only"] }, home, work });
  for (const f of cmd.files) writeFileAtomic(f.path, f.text);

  const started = new Date();
  const run: Run = { day, by, status: "running", ai: agent.id, started_at: isoSeconds(started), pid: process.pid };
  writeAuto({ ...state, ...(by === "auto" ? { auto_day: day } : {}), last: run });
  logLine(`auto: ${by} run for ${day} with ${agent.name} started`);

  const before = snapshot();
  // the tool's own folder first, so a script tool (#!/usr/bin/env node) finds its node
  const env: Record<string, string> = { ...(process.env as Record<string, string>), ...cmd.env, CONTEXT_CAPSULE_HOME: home };
  for (const [k, v] of Object.entries(env)) if (v === "") delete env[k];
  const sep = process.platform === "win32" ? ";" : ":";
  env.PATH = [path.dirname(agent.path), ...searchDirs({ os: currentOS(), home: homeDir(), env: process.env })].join(sep);

  let line: { command: string; args: string[] } | null = { command: cmd.command, args: cmd.args };
  if (process.platform === "win32" && /\.(cmd|bat)$/i.test(cmd.command)) line = windowsCmdLine(cmd);

  const limit = o.timeLimitMs ?? TIME_LIMIT_MS;
  const outcome = await new Promise<{ code: number | null; output: string; timedOut: boolean; error?: string }>((resolve) => {
    if (!line) return resolve({ code: null, output: "", timedOut: false, error: "unsafe path" });
    let output = "";
    let timedOut = false;
    let child;
    try {
      child = spawn(line.command, line.args, {
        cwd: work,
        env,
        stdio: ["ignore", "pipe", "pipe"],
        detached: process.platform !== "win32",
        windowsHide: true,
        windowsVerbatimArguments: line.command !== cmd.command,
      });
    } catch (e) {
      return resolve({ code: null, output: "", timedOut: false, error: (e as Error).message });
    }
    try {
      if (child.pid) os.setPriority(child.pid, os.constants.priority.PRIORITY_LOW);
    } catch {
      // best effort
    }
    const keep = (b: Buffer) => {
      output = (output + b.toString("utf8")).slice(-20000);
    };
    child.stdout?.on("data", keep);
    child.stderr?.on("data", keep);
    const timer = setTimeout(() => {
      timedOut = true;
      if (child.pid) killTree(child.pid);
    }, limit);
    child.on("error", (e) => {
      clearTimeout(timer);
      resolve({ code: null, output, timedOut, error: e.message });
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code, output, timedOut });
    });
  });

  const n = changedCount(before, snapshot());
  const ok = !outcome.timedOut && !outcome.error && outcome.code === 0 && n > 0;
  const kind = ok || outcome.error ? undefined : failureKind(agent.id, outcome.code, outcome.output, outcome.timedOut);
  const reason = ok ? undefined : kind ? failureReason(kind, agent.name) : `Couldn't start ${agent.name}.`;
  const finished = new Date();
  const done: Run = { ...run, status: ok ? "done" : "failed", finished_at: isoSeconds(finished), capsules: n, ...(reason ? { reason } : {}) };
  delete done.pid;
  writeAuto({ ...readAuto(), last: done });
  const mins = Math.max(1, Math.round((finished.getTime() - started.getTime()) / 60000));
  logLine(`auto: ${by} run for ${day} with ${agent.name} ${ok ? "done" : "failed"}: ${n} capsule(s) saved, ${mins} min${ok ? "" : `, exit ${outcome.timedOut ? "timeout" : (outcome.code ?? "none")}, ${kind ?? "couldn't start"}`}`);
  return { ok, day, status: ok ? "done" : "failed", ai: agent.id, name: agent.name, capsules: n, ...(reason ? { reason } : {}) };
}

/** `agents`: what's installed and which one a run would use. */
export function agentsInfo(): { agents: { id: AgentID; name: string }[]; use: { id: AgentID; name: string } | null } {
  let yourAI: unknown;
  try {
    yourAI = JSON.parse(fs.readFileSync(configFile(), "utf8"))?.ai;
  } catch {
    // no config
  }
  const agents = findAgents();
  const use = pickAgent(agents, yourAI);
  return { agents: agents.map((a) => ({ id: a.id, name: a.name })), use: use ? { id: use.id, name: use.name } : null };
}

export const today = () => localDay(new Date().toISOString());
