// context-capsule — mcp: Automatic capsules — finding the AI tools, the restricted command lines, auto.json, and a
// whole `make` run with a stand-in tool (a small script that saves a capsule, or fails, or hangs)
import { afterAll, beforeAll, beforeEach, describe, expect, test } from "bun:test";
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
  failureKind,
  failureReason,
  checkAgent,
  findAgents,
  findProgram,
  inOrder,
  isRunning,
  launchLine,
  make,
  npmShimScript,
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

/** An npm launcher, exactly as npm writes it on Windows, for `node_modules\<pkg>\bin\<pkg>.js`. */
const npmCmd = (pkg: string) =>
  `@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n:start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\n` +
  `IF EXIST "%dp0%\\node.exe" (\r\n  SET "_prog=%dp0%\\node.exe"\r\n) ELSE (\r\n  SET "_prog=node"\r\n  SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\n` +
  `endLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & "%_prog%"  "%dp0%\\node_modules\\${pkg}\\bin\\${pkg}.js" %*\r\n`;

describe("starting a tool on Windows", () => {
  let dir: string;
  const win = (env: Record<string, string> = {}) => ({ os: "windows" as const, home: dir, env: { PATH: path.join(dir, "nodejs"), ...env } });
  beforeAll(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "cc-npm-"));
    fs.mkdirSync(path.join(dir, "node_modules", "codex", "bin"), { recursive: true });
    fs.writeFileSync(path.join(dir, "node_modules", "codex", "bin", "codex.js"), "");
    fs.writeFileSync(path.join(dir, "codex.cmd"), npmCmd("codex"));
    fs.writeFileSync(path.join(dir, "other.cmd"), "@echo off\r\nsomething.exe %*\r\n");
    fs.mkdirSync(path.join(dir, "nodejs"));
    fs.writeFileSync(path.join(dir, "nodejs", "node.exe"), "");
  });

  test("an npm launcher runs as node and its script, so a request with line breaks and quotes gets there whole", () => {
    const script = path.join(dir, "node_modules", "codex", "bin", "codex.js");
    expect(npmShimScript(path.join(dir, "codex.cmd"))).toBe(script);
    const request = 'Turn my work into capsules.\n\n1. Call prepare_day with date "2026-10-19". 100%';
    const line = launchLine(path.join(dir, "codex.cmd"), ["exec", request], win())!;
    expect(line).toEqual({ command: path.join(dir, "nodejs", "node.exe"), args: [script, "exec", request], verbatim: false });
  });

  test("its own node.exe next to it first; without node, cmd.exe (refused when the request isn't safe there)", () => {
    fs.writeFileSync(path.join(dir, "node.exe"), "");
    expect(launchLine(path.join(dir, "codex.cmd"), ["x"], win())!.command).toBe(path.join(dir, "node.exe"));
    fs.rmSync(path.join(dir, "node.exe"));
    const nowhere = { os: "windows" as const, home: path.join(dir, "none"), env: { PATH: path.join(dir, "none"), ProgramFiles: path.join(dir, "none") } };
    expect(launchLine(path.join(dir, "codex.cmd"), ["exec", "a b"], nowhere)!.verbatim).toBe(true);
    expect(launchLine(path.join(dir, "codex.cmd"), ["exec", "two\nlines"], nowhere)).toBeNull();
  });

  test("another .cmd goes through cmd.exe; a program, and anything off Windows, as it is", () => {
    expect(npmShimScript(path.join(dir, "other.cmd"))).toBeNull();
    expect(launchLine(path.join(dir, "other.cmd"), ["a"], win())!.verbatim).toBe(true);
    expect(launchLine("C:\\bin\\claude.exe", ["-p", "x\ny"], win())).toEqual({ command: "C:\\bin\\claude.exe", args: ["-p", "x\ny"], verbatim: false });
    expect(launchLine("/bin/codex.cmd", ["x"], { ...win(), os: "linux" })!.command).toBe("/bin/codex.cmd");
  });

  test("node is found in Program Files when the app has no shell PATH", () => {
    fs.mkdirSync(path.join(dir, "pf", "nodejs"), { recursive: true });
    fs.writeFileSync(path.join(dir, "pf", "nodejs", "node.exe"), "");
    expect(findProgram("node", { os: "windows", home: dir, env: { PATH: "", ProgramFiles: path.join(dir, "pf") } })).toBe(path.join(dir, "pf", "nodejs", "node.exe"));
  });
});

describe("finding the tools", () => {
  let dir: string;
  beforeAll(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "cc-agents-"));
    const ext = process.platform === "win32" ? ".exe" : "";
    for (const b of ["codex", "gemini"]) fs.writeFileSync(path.join(dir, b + ext), "#!/bin/sh\n", { mode: 0o755 });
  });

  test("on the PATH, in preference order; Your AI's own first", () => {
    const ctx = { os: process.platform === "win32" ? ("windows" as const) : ("mac" as const), home: dir, env: { PATH: dir } };
    const found = findAgents(ctx);
    expect(found.map((a) => a.id)).toEqual(["codex", "gemini"]);
    expect(pickAgent(found, undefined)!.id).toBe("codex");
    expect(pickAgent(found, "gemini")!.id).toBe("gemini");
    expect(pickAgent(found, "claude")!.id).toBe("codex"); // Claude Code isn't here
    expect(pickAgent([agent("claude-code"), agent("codex")], "chatgpt")!.id).toBe("codex");
    expect(pickAgent([agent("claude-code"), agent("codex")], "cursor")!.id).toBe("claude-code");
    expect(pickAgent([], "claude")).toBeNull();
    expect(inOrder([agent("claude-code"), agent("codex"), agent("gemini")], "gemini").map((a) => a.id)).toEqual(["gemini", "claude-code", "codex"]);
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
    expect(failureReason("signed-out", "Claude Code")).toBe("Claude Code isn't signed in. Open it once and sign in.");
    expect(failureReason("limit", "Codex")).toBe("Codex has reached its usage limit. Try again later.");
    expect(failureReason("timeout", "Codex")).toBe("Codex took longer than 10 minutes, so it was stopped.");
    expect(failureReason("no-capsules", "Codex")).toBe("Codex didn't save any capsules.");
    expect(failureReason("error", "Gemini CLI")).toBe("Couldn't make the capsules. Try again, or open Gemini CLI to check it's working.");
  });

  test("signed out only on each tool's own words", () => {
    // what each tool prints when it isn't signed in
    const claudeJson = JSON.stringify({ type: "result", subtype: "success", is_error: true, result: "Invalid API key · Please run /login" });
    expect(failureKind("claude-code", 1, claudeJson, false)).toBe("signed-out");
    expect(failureKind("claude-code", 1, "Not logged in · Please run /login", false)).toBe("signed-out");
    expect(
      failureKind("claude-code", 1, 'API Error: 401 {"type":"error","error":{"type":"authentication_error","message":"OAuth token has expired."}}', false),
    ).toBe("signed-out");
    expect(
      failureKind("codex", 1, "ERROR: Your access token could not be refreshed because your refresh token has expired. Please log out and sign in again.", false),
    ).toBe("signed-out");
    expect(failureKind("codex", 1, 'ERROR: unexpected status 401 Unauthorized: {"error":{"message":"Missing bearer authentication in header"}}', false)).toBe(
      "signed-out",
    );
    expect(
      failureKind(
        "gemini",
        41,
        "Please set an Auth method in your /home/a/.gemini/settings.json or specify one of the following environment variables before running: GEMINI_API_KEY, GOOGLE_GENAI_USE_VERTEXAI, GOOGLE_GENAI_USE_GCA",
        false,
      ),
    ).toBe("signed-out");
    expect(failureKind("gemini", 41, '{"error":{"type":"FatalAuthenticationError","message":"","code":41}}', false)).toBe("signed-out");
  });

  test("anything else is a plain failure, not signed out", () => {
    // the audit's case: a server that couldn't start, mentioning credentials and "signed in"
    expect(failureKind("claude-code", 17, "MCP server cannot start: missing credentials.json fixture file; already signed in", false)).toBe("error");
    // auth-ish words that aren't the tool's own sign-in message
    expect(failureKind("claude-code", 1, "Error: could not read api key from keychain helper (401 lines skipped)", false)).toBe("error");
    expect(failureKind("codex", 1, "thread 'main' panicked: authentication module failed to load", false)).toBe("error");
    expect(failureKind("gemini", 1, "Unauthorized tool call refused by policy", false)).toBe("error");
    // one tool's message from another tool doesn't count
    expect(failureKind("gemini", 1, "Please run /login", false)).toBe("error");
    // "network" or "quota" somewhere isn't enough
    expect(failureKind("codex", 1, "network sandbox: read-only; quota of 3 tool calls", false)).toBe("error");
    expect(failureKind("codex", 0, "done", false)).toBe("no-capsules");
  });

  test("usage limits and no internet on the tool's or system's own words", () => {
    expect(failureKind("claude-code", 1, "Claude AI usage limit reached|1761300000", false)).toBe("limit");
    expect(failureKind("claude-code", 1, "Credit balance is too low", false)).toBe("limit");
    expect(failureKind("codex", 1, "ERROR: You've hit your usage limit. Upgrade to Pro or try again in 2 hours.", false)).toBe("limit");
    expect(failureKind("gemini", 1, '{"error":{"code":429,"status":"RESOURCE_EXHAUSTED"}}', false)).toBe("limit");
    expect(failureKind("codex", 1, "stream error: error sending request: dns error: failed to lookup address (getaddrinfo ENOTFOUND api.openai.com)", false)).toBe(
      "offline",
    );
    expect(failureKind("codex", null, "", true)).toBe("timeout");
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

  test("a failure for another reason isn't called signed out; the log says what kind, never the output", async () => {
    const r = await make({
      day,
      by: "click",
      agent: tool('echo "MCP server cannot start: missing credentials.json fixture file; already signed in" >&2; exit 17'),
    });
    const generic = "Couldn't make the capsules. Try again, or open Claude Code to check it's working.";
    expect(r).toMatchObject({ ok: false, status: "failed", reason: generic });
    expect(readAuto().last).toMatchObject({ status: "failed", reason: generic });
    const log = fs.readFileSync(path.join(home, "app.log"), "utf8");
    expect(log).toMatch(/failed: 0 capsule\(s\) saved, \d+ min, exit 17, error/);
    expect(log).not.toContain("credentials.json");
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

describe("trying the next tool (stand-ins started like the real ones: an npm launcher on Windows)", () => {
  let home: string;
  let bin: string;
  let n = 0;
  const day = "2026-10-19";
  /** A Node script that notes it ran and what it got, then does `body`. */
  const tool = (id: Agent["id"], name: string, body: string): Agent => {
    const pkg = `fake${n++}`;
    const js =
      `const fs = require("fs"), path = require("path");\nconst home = process.env.CONTEXT_CAPSULE_HOME;\n` +
      `fs.appendFileSync(path.join(home, "ran.txt"), "${pkg}\\n");\n` +
      `fs.writeFileSync(path.join(home, "${pkg}-args.json"), JSON.stringify(process.argv.slice(2)));\n${body}\n`;
    if (process.platform === "win32") {
      fs.mkdirSync(path.join(bin, "node_modules", pkg, "bin"), { recursive: true });
      fs.writeFileSync(path.join(bin, "node_modules", pkg, "bin", `${pkg}.js`), js);
      fs.writeFileSync(path.join(bin, `${pkg}.cmd`), npmCmd(pkg));
      return { id, name, path: path.join(bin, `${pkg}.cmd`) };
    }
    fs.writeFileSync(path.join(bin, `${pkg}.js`), js);
    fs.writeFileSync(path.join(bin, pkg), `#!/bin/sh\nexec node "${path.join(bin, `${pkg}.js`)}" "$@"\n`, { mode: 0o755 });
    return { id, name, path: path.join(bin, pkg) };
  };
  const saves = (file: string) => `fs.mkdirSync(path.join(home, "capsules"), { recursive: true });\nfs.writeFileSync(path.join(home, "capsules", "${file}.capsule.md"), "x");`;
  // each tool's own sign-in words (only those count as signed out)
  const LOGIN = `console.error("Please run /login"); process.exit(1);`;
  const CODEX_LOGIN = `console.error("Please log out and sign in again."); process.exit(1);`;
  const LIMIT = `console.error("Claude AI usage limit reached"); process.exit(1);`;
  const ran = () => fs.readFileSync(path.join(home, "ran.txt"), "utf8").trim().split("\n");
  beforeEach(() => {
    home = tempHome();
    bin = fs.mkdtempSync(path.join(os.tmpdir(), "cc-tools-"));
    n = 0;
    writeFixtureSession(home, "20261019-100000");
  });

  test("the whole request reaches the tool, line breaks and quotes too", async () => {
    const a = tool("claude-code", "Claude Code", saves("site"));
    const r = await make({ day, by: "click", agent: a });
    expect(r).toMatchObject({ ok: true, status: "done", capsules: 1 });
    const args: string[] = JSON.parse(fs.readFileSync(path.join(home, "fake0-args.json"), "utf8"));
    const request = after(args, "-p")!;
    expect(request).toContain(`1. Call prepare_day with date "${day}".`);
    expect(request).toContain("\n\nYou are running on your own");
  }, 20000);

  test("one that isn't signed in is skipped; the next makes them and says why", async () => {
    const first = tool("claude-code", "Claude Code", saves("never"));
    const second = tool("codex", "Codex", saves("site"));
    const check = async (a: Agent) => (a === first ? { ready: false, kind: "signed-out" as const, reason: "Claude Code isn't signed in. Open it once and sign in." } : { ready: true });
    const r = await make({ day, by: "click", agents: [first, second], check });
    expect(r).toMatchObject({ ok: true, ai: "codex", capsules: 1, note: "Claude Code isn't signed in. Open it once and sign in. So Codex made them." });
    expect(ran()).toEqual(["fake1"]);
    expect(readAuto().last).toMatchObject({ status: "done", ai: "codex", note: r.note });
  }, 20000);

  test("one that fails before working (signed out) hands over to the next", async () => {
    const r = await make({ day, by: "click", agents: [tool("claude-code", "Claude Code", LOGIN), tool("codex", "Codex", saves("site"))] });
    expect(r).toMatchObject({ ok: true, ai: "codex", note: "Claude Code isn't signed in. Open it once and sign in. So Codex made them." });
    expect(ran()).toEqual(["fake0", "fake1"]);
  }, 20000);

  test("one that fails while working stops the run: the next isn't tried", async () => {
    const r = await make({ day, by: "click", agents: [tool("claude-code", "Claude Code", "process.exit(3);"), tool("codex", "Codex", saves("site"))] });
    expect(r).toMatchObject({ ok: false, status: "failed", ai: "claude-code", reason: "Couldn't make the capsules. Try again, or open Claude Code to check it's working." });
    expect(ran()).toEqual(["fake0"]);
  }, 20000);

  test("an error that only mentions sign-in words isn't a sign-in: the next isn't tried", async () => {
    const r = await make({ day, by: "click", agents: [tool("claude-code", "Claude Code", `console.error("missing credentials.json fixture"); process.exit(17);`), tool("codex", "Codex", saves("site"))] });
    expect(r).toMatchObject({ ok: false, status: "failed", ai: "claude-code", reason: "Couldn't make the capsules. Try again, or open Claude Code to check it's working." });
    expect(ran()).toEqual(["fake0"]);
  }, 20000);

  test("one that saved some before running out of usage: the next isn't tried, so nothing is made twice", async () => {
    const r = await make({ day, by: "click", agents: [tool("claude-code", "Claude Code", saves("half") + "\n" + LIMIT), tool("codex", "Codex", saves("site"))] });
    expect(r).toMatchObject({ ok: false, status: "failed", ai: "claude-code", capsules: 1, reason: "Claude Code has reached its usage limit. Try again later." });
    expect(ran()).toEqual(["fake0"]);
  }, 20000);

  test("out of usage before saving anything: the next makes them", async () => {
    const r = await make({ day, by: "click", agents: [tool("claude-code", "Claude Code", LIMIT), tool("codex", "Codex", saves("site"))] });
    expect(r).toMatchObject({ ok: true, ai: "codex", note: "Claude Code has reached its usage limit. Try again later. So Codex made them." });
    expect(ran()).toEqual(["fake0", "fake1"]);
  }, 20000);

  test("none can: every reason, in the order tried", async () => {
    const r = await make({ day, by: "click", agents: [tool("claude-code", "Claude Code", LOGIN), tool("codex", "Codex", CODEX_LOGIN)] });
    expect(r).toMatchObject({ ok: false, status: "failed", ai: "codex" });
    expect(r.reason).toBe("Claude Code isn't signed in. Open it once and sign in. Codex isn't signed in. Open it once and sign in.");
    expect(readAuto().last).toMatchObject({ status: "failed", reason: r.reason });
  }, 20000);

  test("signed in or not, from each tool's own status command", async () => {
    const ctx = { os: (process.platform === "win32" ? "windows" : "linux") as "windows" | "linux", home, env: process.env };
    expect(await checkAgent(tool("claude-code", "Claude Code", `console.log(JSON.stringify({ loggedIn: true }));`), ctx)).toEqual({ ready: true });
    expect(await checkAgent(tool("claude-code", "Claude Code", `console.log(JSON.stringify({ loggedIn: false }));`), ctx)).toMatchObject({ ready: false, kind: "signed-out" });
    expect(await checkAgent(tool("codex", "Codex", "process.exit(1);"), ctx)).toMatchObject({ ready: false, reason: "Codex isn't signed in. Open it once and sign in." });
    expect(await checkAgent(tool("codex", "Codex", ""), ctx)).toEqual({ ready: true });
    expect(await checkAgent({ id: "codex", name: "Codex", path: path.join(bin, "missing") }, ctx)).toMatchObject({ ready: false, kind: "start" });
    const gemini = { id: "gemini" as const, name: "Gemini CLI", path: "unused" };
    expect(await checkAgent(gemini, { ...ctx, env: {} })).toEqual({ ready: null });
    expect(await checkAgent(gemini, { ...ctx, env: { GEMINI_API_KEY: "k" } })).toEqual({ ready: true });
    // what the status commands got
    expect(JSON.parse(fs.readFileSync(path.join(home, "fake0-args.json"), "utf8"))).toEqual(["auth", "status"]);
    expect(JSON.parse(fs.readFileSync(path.join(home, "fake2-args.json"), "utf8"))).toEqual(["login", "status"]);
  }, 30000);
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
