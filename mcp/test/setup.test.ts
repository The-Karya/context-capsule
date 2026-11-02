// context-capsule — mcp: tests for the one-line install (setup, remove) and what an app starts when it came from npx
import { beforeEach, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { CLIENTS, isTemporaryRun, listClients, npxCommand, pointsAtExisting, type Ctx } from "../src/clients.ts";
import { remove, setup } from "../src/setup.ts";

let home: string;
let ctx: Ctx;
let self: { command: string; args: string[] };
const fileOf = (id: string) => CLIENTS.find((d) => d.id === id)!.file!(ctx);
const connected = (id: string) => listClients(ctx).find((c) => c.id === id)!.connected;

beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), "cc-setup-"));
  ctx = { os: "mac", home, env: {}, appDirs: [] };
  const bin = path.join(home, "bin", "context-capsule-mcp");
  fs.mkdirSync(path.dirname(bin), { recursive: true });
  fs.writeFileSync(bin, "");
  self = { command: bin, args: ["serve"] };
});

describe("setup", () => {
  test("connects every app found, and only those", () => {
    fs.mkdirSync(path.join(home, ".codex"));
    fs.mkdirSync(path.join(home, ".cursor"));
    const r = setup({ ctx, self });
    expect(r.ok).toBe(true);
    expect(connected("codex")).toBe(true);
    expect(connected("cursor")).toBe(true);
    expect(fs.existsSync(path.join(home, ".gemini"))).toBe(false);
    expect(r.lines.some((l) => l.startsWith("✓") && l.includes("Cursor"))).toBe(true);
    expect(r.lines.join("\n")).toContain("Context Capsule app");
  });

  test("twice is the same as once", () => {
    fs.mkdirSync(path.join(home, ".cursor"));
    setup({ ctx, self });
    const before = fs.readFileSync(fileOf("cursor"), "utf8");
    expect(setup({ ctx, self }).ok).toBe(true);
    expect(fs.readFileSync(fileOf("cursor"), "utf8")).toBe(before);
  });

  test("just the apps named; an app that isn't here is an error", () => {
    fs.mkdirSync(path.join(home, ".codex"));
    fs.mkdirSync(path.join(home, ".cursor"));
    expect(setup({ ctx, self, ids: ["cursor"] }).ok).toBe(true);
    expect(connected("cursor")).toBe(true);
    expect(connected("codex")).toBe(false);
    const r = setup({ ctx, self, ids: ["gemini"] });
    expect(r.ok).toBe(false);
    expect(r.lines[0]).toStartWith("✗");
  });

  test("Claude's two kinds: says how, and writes nothing of theirs", () => {
    fs.mkdirSync(path.join(home, "Library", "Application Support", "Claude"), { recursive: true });
    fs.mkdirSync(path.join(home, ".claude"));
    const out = setup({ ctx, self }).lines.join("\n");
    expect(out).toContain("Install Extension…");
    expect(out).toContain(`claude mcp add --scope user context-capsule -- ${self.command} serve`);
    expect(fs.existsSync(path.join(home, ".claude.json"))).toBe(false);
  });

  test("nothing here: says so, and fails", () => {
    const r = setup({ ctx, self });
    expect(r.ok).toBe(false);
    expect(r.lines[0]).toContain("No AI apps found");
  });
});

describe("remove", () => {
  test("takes it out and keeps the app's other settings", () => {
    fs.mkdirSync(path.join(home, ".cursor"));
    fs.writeFileSync(fileOf("cursor"), JSON.stringify({ mcpServers: { other: { command: "x" } } }));
    setup({ ctx, self });
    const r = remove({ ctx });
    expect(r.ok).toBe(true);
    expect(connected("cursor")).toBe(false);
    expect(JSON.parse(fs.readFileSync(fileOf("cursor"), "utf8")).mcpServers).toEqual({ other: { command: "x" } });
    expect(r.lines.some((l) => l.includes("Disconnected from Cursor"))).toBe(true);
  });

  test("nothing connected: one plain line, nothing written", () => {
    expect(remove({ ctx }).lines).toEqual(["Context Capsule wasn't connected to any app."]);
    expect(fs.readdirSync(home)).toEqual(["bin"]);
  });
});

describe("run with npx", () => {
  test("its cache isn't a place to point an app at", () => {
    expect(isTemporaryRun("/Users/me/.npm/_npx/abc123/node_modules/context-capsule-mcp/dist")).toBe(true);
    expect(isTemporaryRun("C:\\Users\\me\\AppData\\Local\\npm-cache\\_npx\\abc\\node_modules\\context-capsule-mcp\\dist")).toBe(true);
    expect(isTemporaryRun("/private/tmp/bunx-501-context-capsule-mcp@latest/node_modules/context-capsule-mcp/dist")).toBe(true);
    expect(isTemporaryRun("/usr/local/lib/node_modules/context-capsule-mcp/dist")).toBe(false);
  });

  test("the entry goes through cmd on Windows, and counts as connected", () => {
    expect(npxCommand("darwin")).toEqual({ command: "npx", args: ["-y", "context-capsule-mcp", "serve"] });
    expect(npxCommand("win32")).toEqual({ command: "cmd", args: ["/c", "npx", "-y", "context-capsule-mcp", "serve"] });
    expect(pointsAtExisting(npxCommand("darwin"))).toBe(true);
    expect(pointsAtExisting(npxCommand("win32"))).toBe(true);
    expect(pointsAtExisting({ command: "npx", args: ["-y", "someone-elses-server"] })).toBe(false);
    expect(pointsAtExisting({ command: "cmd", args: ["/c", "del", "context-capsule-mcp"] })).toBe(false);
  });
});

describe("the command itself", () => {
  test("setup then remove, for real, in a throwaway home", () => {
    const root = path.resolve(import.meta.dir, "..");
    const build = spawnSync("bun", ["run", "build"], { cwd: root });
    expect(build.status).toBe(0);
    fs.mkdirSync(path.join(home, ".cursor"));
    const env = { ...process.env, HOME: home, USERPROFILE: home };
    const run = (cmd: string) => spawnSync("node", [path.join(root, "dist", "index.js"), cmd], { env, encoding: "utf8" });
    const on = run("setup");
    expect(on.status).toBe(0);
    expect(on.stdout).toContain("Connected to Cursor");
    const entry = JSON.parse(fs.readFileSync(path.join(home, ".cursor", "mcp.json"), "utf8")).mcpServers["context-capsule"];
    expect(pointsAtExisting(entry)).toBe(true);
    const off = run("remove");
    expect(off.status).toBe(0);
    expect(off.stdout).toContain("Disconnected from Cursor");
    expect(JSON.parse(fs.readFileSync(path.join(home, ".cursor", "mcp.json"), "utf8")).mcpServers?.["context-capsule"]).toBeUndefined();
  }, 60000);
});
