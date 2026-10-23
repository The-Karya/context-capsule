// context-capsule — mcp: Automatic capsules — finding the AI tools, the restricted command lines, auto.json, and a
// whole `make` run with a stand-in tool (a small script that saves a capsule, or fails, or hangs)
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { spawnSync } from "node:child_process";
import {
  CLAUDE_ALLOWED,
  CODEX_OFF,
  GEMINI_POLICY,
  agentCommand,
  autoFile,
  changedCount,
  failureReason,
  findAgents,
  isRunning,
  make,
  pickAgent,
  readAuto,
  windowsCmdLine,
  type Agent,
} from "../src/auto.ts";
import { MAKE_TOOLS } from "../src/server.ts";
import { tempHome, writeFixtureSession } from "./helpers.ts";

const opts = { prompt: "Make my capsules.", self: { command: "/Apps/cc-mcp", args: ["serve", "--make-only"] }, home: "/data", work: "/data/auto" };
const agent = (id: Agent["id"]): Agent => ({ id, name: id, path: `/bin/${id}` });
/** The value after a flag. */
const after = (args: string[], flag: string) => args[args.indexOf(flag) + 1];

describe("restricted command lines", () => {
  test("Claude Code: only our server, no built-in tools, our tools allowed, everything else refused", () => {
    const c = agentCommand(agent("claude-code"), opts);
    expect(c.command).toBe("/bin/claude-code");
    expect(after(c.args, "-p")).toBe("Make my capsules.");
    expect(c.args).toContain("--strict-mcp-config");
    expect(after(c.args, "--tools")).toBe("");
    expect(after(c.args, "--allowedTools")).toBe(CLAUDE_ALLOWED);
    expect(CLAUDE_ALLOWED).toBe("mcp__context-capsule__*");
    expect(after(c.args, "--permission-mode")).toBe("dontAsk");
    expect(c.args).toContain("--disable-slash-commands");
    expect(c.args).not.toContain("--dangerously-skip-permissions");
    expect(c.args.join(" ")).not.toMatch(/bypassPermissions|Bash|WebFetch|Edit/);
    const file = after(c.args, "--mcp-config");
    expect(file).toBe(path.join("/data/auto", "mcp.json"));
    const cfg = JSON.parse(c.files.find((f) => f.path === file)!.text);
    expect(Object.keys(cfg.mcpServers)).toEqual(["context-capsule"]);
    expect(cfg.mcpServers["context-capsule"]).toEqual({ type: "stdio", command: "/Apps/cc-mcp", args: ["serve", "--make-only"], env: { CONTEXT_CAPSULE_HOME: "/data" } });
  });

  test("Codex: read-only sandbox, never approves, no shell / web / apply_patch, none of the user's config", () => {
    const c = agentCommand(agent("codex"), opts);
    expect(c.args[0]).toBe("exec");
    for (const f of ["--ignore-user-config", "--ignore-rules", "--skip-git-repo-check", "--ephemeral"]) expect(c.args).toContain(f);
    expect(after(c.args, "--sandbox")).toBe("read-only");
    expect(c.args).not.toContain("--dangerously-bypass-approvals-and-sandbox");
    expect(c.args).not.toContain("--search");
    const sets = c.args.filter((_, i) => c.args[i - 1] === "-c");
    expect(sets).toContain("approval_policy=never");
    expect(sets).toContain("web_search=disabled");
    expect(sets).toContain("include_apply_patch_tool=false");
    expect(sets).toContain("mcp_servers.context-capsule.command=/Apps/cc-mcp");
    expect(sets).toContain("mcp_servers.context-capsule.args=['serve', '--make-only']");
    expect(sets).toContain("mcp_servers.context-capsule.env.CONTEXT_CAPSULE_HOME=/data");
    const off = c.args.filter((_, i) => c.args[i - 1] === "--disable");
    for (const f of ["shell_tool", "unified_exec", "browser_use", "computer_use", "apps", "plugins", "hooks"]) expect(off).toContain(f);
    expect(off).toEqual(CODEX_OFF);
    expect(after(c.args, "--cd")).toBe("/data/auto");
    expect(c.args[c.args.length - 1]).toBe("Make my capsules.");
    expect(c.args.join("\n")).not.toContain('"'); // nothing a Windows cmd.exe line could break on
  });

  test("Gemini CLI: no built-in tools, only our server, a deny-all-but-ours policy, no extensions", () => {
    const c = agentCommand(agent("gemini"), opts);
    expect(after(c.args, "-p")).toBe("Make my capsules.");
    expect(after(c.args, "--approval-mode")).toBe("default");
    expect(after(c.args, "--extensions")).toBe("none");
    expect(after(c.args, "--allowed-mcp-server-names")).toBe("context-capsule");
    expect(c.args).not.toContain("--yolo");
    const policy = after(c.args, "--policy");
    expect(after(c.args, "--admin-policy")).toBe(policy);
    expect(c.files.find((f) => f.path === policy)!.text).toBe(GEMINI_POLICY);
    expect(GEMINI_POLICY).toMatch(/toolName = "\*"\ndecision = "deny"/);
    expect(GEMINI_POLICY).toMatch(/mcpName = "context-capsule"\ndecision = "allow"/);
    const settings = JSON.parse(c.files.find((f) => f.path === path.join("/data/auto", ".gemini", "settings.json"))!.text);
    expect(settings.tools.core).toEqual([]);
    expect(Object.keys(settings.mcpServers)).toEqual(["context-capsule"]);
    expect(settings.mcpServers["context-capsule"].trust).toBe(true);
    expect(c.env.GEMINI_CLI_TRUST_WORKSPACE).toBe("true");
  });

  test("Windows .cmd: every argument quoted; anything cmd.exe would still read refused", () => {
    const line = windowsCmdLine({ command: "C:\\npm\\codex.cmd", args: ["exec", "a b"], files: [], env: {} })!;
    expect(line.args.slice(0, 3)).toEqual(["/d", "/s", "/c"]);
    expect(line.args[3]).toBe('""C:\\npm\\codex.cmd" "exec" "a b""');
    expect(windowsCmdLine({ command: "x.cmd", args: ["100%"], files: [], env: {} })).toBeNull();
    expect(windowsCmdLine({ command: "x.cmd", args: ['say "hi"'], files: [], env: {} })).toBeNull();
  });
});

describe("finding the tools", () => {
  let dir: string;
  beforeAll(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "cc-agents-"));
    for (const b of ["codex", "gemini"]) fs.writeFileSync(path.join(dir, b), "#!/bin/sh\n", { mode: 0o755 });
  });

  test("on the PATH, in preference order; Your AI's own first", () => {
    const ctx = { os: "mac" as const, home: dir, env: { PATH: dir } };
    const found = findAgents(ctx);
    expect(found.map((a) => a.id)).toEqual(["codex", "gemini"]);
    expect(pickAgent(found, undefined)!.id).toBe("codex");
    expect(pickAgent(found, "gemini")!.id).toBe("gemini");
    expect(pickAgent(found, "claude")!.id).toBe("codex"); // Claude Code isn't here
    expect(pickAgent([agent("claude-code"), agent("codex")], "chatgpt")!.id).toBe("codex");
    expect(pickAgent([agent("claude-code"), agent("codex")], "cursor")!.id).toBe("claude-code");
    expect(pickAgent([], "claude")).toBeNull();
  });

  test("in the usual install folders when the app has no shell PATH", () => {
    fs.mkdirSync(path.join(dir, ".local", "bin"), { recursive: true });
    fs.writeFileSync(path.join(dir, ".local", "bin", "claude"), "#!/bin/sh\n", { mode: 0o755 });
    const found = findAgents({ os: "linux", home: dir, env: { PATH: "/nowhere" } });
    expect(found[0]).toEqual({ id: "claude-code", name: "Claude Code", path: path.join(dir, ".local", "bin", "claude") });
  });
});

describe("outcome words", () => {
  test("plain reasons, never the tool's output", () => {
    expect(failureReason("Claude Code", 1, "Invalid API key · Please run /login", false)).toBe("Claude Code isn't signed in. Open it once and sign in.");
    expect(failureReason("Codex", 1, "You've hit your usage limit", false)).toBe("Codex has reached its usage limit. Try again later.");
    expect(failureReason("Codex", null, "", true)).toBe("Codex took longer than 10 minutes, so it was stopped.");
    expect(failureReason("Codex", 0, "all good", false)).toBe("Codex didn't save any capsules.");
    expect(failureReason("Gemini CLI", 3, "some secret screen text", false)).toBe("Gemini CLI stopped with an error (code 3).");
  });

  test("capsules saved = new or changed files", () => {
    const before = new Map([["a.capsule.md", 1], ["b.capsule.md", 2]]);
    expect(changedCount(before, new Map([["a.capsule.md", 1], ["b.capsule.md", 5], ["c.capsule.md", 1]]))).toBe(2);
    expect(changedCount(before, before)).toBe(0);
  });

  test("a running entry is over when its process is gone or it is too old", () => {
    const now = Date.parse("2026-10-23T18:05:00Z");
    const run = { day: "2026-10-23", by: "auto" as const, status: "running" as const, ai: "codex" as const, started_at: "2026-10-23T18:00:00Z" };
    expect(isRunning({ last: { ...run, pid: process.pid } }, now)).toBe(true);
    expect(isRunning({ last: { ...run, pid: 999999 } }, now)).toBe(false);
    expect(isRunning({ last: { ...run, pid: process.pid } }, now + 20 * 60000)).toBe(false);
    expect(isRunning({ last: { ...run, status: "done", pid: process.pid } }, now)).toBe(false);
  });
});

const unix = process.platform !== "win32";

describe.if(unix)("a whole run with a stand-in tool", () => {
  let home: string;
  let bin: string;
  const day = "2026-10-19";
  const tool = (body: string): Agent => {
    const p = path.join(bin, `fake-${Math.random().toString(36).slice(2)}`);
    fs.writeFileSync(p, `#!/bin/sh\n${body}\n`, { mode: 0o755 });
    return { id: "claude-code", name: "Claude Code", path: p };
  };
  beforeAll(() => {
    home = tempHome();
    bin = fs.mkdtempSync(path.join(os.tmpdir(), "cc-bin-"));
  });

  test("nothing recorded that day: nothing runs, auto.json untouched", async () => {
    const r = await make({ day, by: "auto", agent: tool("exit 0") });
    expect(r).toMatchObject({ ok: false, status: "nothing" });
    expect(fs.existsSync(autoFile())).toBe(false);
  });

  test("done: the tool saved capsules; auto.json says so; the tool got only our restricted setup", async () => {
    writeFixtureSession(home, "20261019-100000");
    const a = tool(
      `echo "$@" > "$CONTEXT_CAPSULE_HOME/args.txt"\nmkdir -p "$CONTEXT_CAPSULE_HOME/capsules"\n` +
        `echo x > "$CONTEXT_CAPSULE_HOME/capsules/site-2026-10-19.capsule.md"\necho x > "$CONTEXT_CAPSULE_HOME/capsules/site.capsule.md"`,
    );
    const r = await make({ day, by: "auto", agent: a });
    expect(r).toMatchObject({ ok: true, status: "done", capsules: 2, name: "Claude Code" });
    const f = readAuto();
    expect(f.auto_day).toBe(day);
    expect(f.last).toMatchObject({ day, by: "auto", status: "done", ai: "claude-code", capsules: 2 });
    expect(f.last!.pid).toBeUndefined();
    const args = fs.readFileSync(path.join(home, "args.txt"), "utf8");
    expect(args).toContain("--strict-mcp-config");
    expect(args).toContain("--permission-mode dontAsk");
    const cfg = JSON.parse(fs.readFileSync(path.join(home, "auto", "mcp.json"), "utf8"));
    expect(cfg.mcpServers["context-capsule"].args).toContain("--make-only");
    expect(fs.readFileSync(path.join(home, "app.log"), "utf8")).toMatch(/auto: auto run for 2026-10-19 with Claude Code done: 2 capsule\(s\) saved/);
  });

  test("a click run keeps auto_day; failure gives plain words", async () => {
    const r = await make({ day, by: "click", agent: tool('echo "Please run /login" >&2; exit 1') });
    expect(r).toMatchObject({ ok: false, status: "failed", reason: "Claude Code isn't signed in. Open it once and sign in." });
    expect(readAuto()).toMatchObject({ auto_day: day, last: { by: "click", status: "failed", capsules: 0 } });
  });

  test("a tool that hangs is stopped at the time limit", async () => {
    const r = await make({ day, by: "click", agent: tool("sleep 30"), timeLimitMs: 300 });
    expect(r).toMatchObject({ ok: false, status: "failed", reason: "Claude Code took longer than 10 minutes, so it was stopped." });
  }, 10000);

  test("one at a time", async () => {
    fs.writeFileSync(autoFile(), JSON.stringify({ last: { day, by: "auto", status: "running", ai: "codex", started_at: new Date().toISOString(), pid: process.pid } }));
    const r = await make({ day, by: "click", agent: tool("exit 0") });
    expect(r.status).toBe("running");
  });
});

describe("serve --make-only", () => {
  const root = path.resolve(import.meta.dir, "..");
  const entry = path.join(root, "dist", "index.js");
  let client: Client;
  beforeAll(async () => {
    if (!fs.existsSync(entry)) spawnSync("bun", ["run", "build"], { cwd: root, stdio: "inherit" });
    client = new Client({ name: "auto", version: "0" });
    await client.connect(
      new StdioClientTransport({ command: "node", args: [entry, "serve", "--make-only"], env: { PATH: process.env.PATH ?? "", HOME: os.tmpdir(), CONTEXT_CAPSULE_HOME: tempHome() }, stderr: "ignore" }),
    );
  }, 30000);
  afterAll(async () => {
    await client?.close();
  });

  test("only the tools that write a day's capsules: nothing records, exports or imports", async () => {
    const names = (await client.listTools()).tools.map((t) => t.name).sort();
    expect(names).toEqual([...MAKE_TOOLS].sort());
    for (const n of ["start_recording", "stop_recording", "add_note", "set_project", "export_capsule", "import_capsule"]) expect(names).not.toContain(n);
  });
});
