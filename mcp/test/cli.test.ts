// context-capsule — mcp: the built context-capsule-mcp commands (JSON on stdout) and the MCP server over HTTP
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import { localDay } from "../src/activity.ts";
import { startHttp, tokenFile, type HttpServer } from "../src/http.ts";
import { TOOL_HINTS } from "../src/server.ts";
import { VALID_BODY, at, tempHome, writeFixtureSession } from "./helpers.ts";

const root = path.resolve(import.meta.dir, "..");
const entry = path.join(root, "dist", "index.js");
let home: string;
let fakeHome: string;

beforeAll(() => {
  if (!fs.existsSync(entry)) {
    const b = spawnSync("bun", ["run", "build"], { cwd: root, stdio: "inherit" });
    if (b.status !== 0) throw new Error("build failed");
  }
  home = tempHome();
  fakeHome = fs.mkdtempSync(path.join(os.tmpdir(), "cc-home-"));
  writeFixtureSession(home, "20261019-100000");
}, 60000);

function run(args: string[], input?: string) {
  const r = spawnSync("node", [entry, ...args], {
    input,
    encoding: "utf8",
    env: { PATH: process.env.PATH ?? "", HOME: fakeHome, USERPROFILE: fakeHome, CONTEXT_CAPSULE_HOME: home },
  });
  return { code: r.status, out: r.stdout, err: r.stderr, json: () => JSON.parse(r.stdout) };
}

describe("commands", () => {
  test("clients", () => {
    const r = run(["clients"]);
    expect(r.code).toBe(0);
    const list = r.json() as { id: string; how: string }[];
    expect(list.map((c) => c.id)).toContain("cursor");
    expect(list.find((c) => c.id === "claude")!.how).toBe("extension");
  });

  test("connect / disconnect write the app's file under HOME and answer {ok, message}", () => {
    fs.mkdirSync(path.join(fakeHome, ".cursor"), { recursive: true });
    const r = run(["connect", "cursor"]);
    expect(r.code).toBe(0);
    expect(r.json()).toMatchObject({ ok: true });
    const cfg = JSON.parse(fs.readFileSync(path.join(fakeHome, ".cursor", "mcp.json"), "utf8"));
    expect(cfg.mcpServers["context-capsule"].args).toEqual([entry, "serve"]);
    expect((run(["clients"]).json() as { id: string; connected: boolean }[]).find((c) => c.id === "cursor")!.connected).toBe(true);
    expect(run(["disconnect", "cursor"]).json()).toMatchObject({ ok: true });
    const claude = run(["connect", "claude"]);
    expect(claude.code).toBe(1);
    expect(claude.json()).toMatchObject({ ok: false, how: "extension" });
    expect(run(["connect"]).code).toBe(2);
  }, 30000); // six commands, each a fresh node process: slow on a busy computer

  test("day", () => {
    const r = run(["day", "--date", localDay(at(0))]);
    expect(r.code).toBe(0);
    const d = r.json() as { text: string; chars: number };
    expect(d.chars).toBe(d.text.length);
    expect(d.text).toContain("=== CAPSULE ===");
    expect(run(["day", "--date", "soon"]).json().error).toContain("not a date");
  });

  test("save reads stdin", () => {
    const reply = `=== CAPSULE ===\n---\ntitle: From a chat\n---\n${VALID_BODY.replace(/\*\(you said, 10:14\)\*/, "*(guess)*")}\n=== END ===\n`;
    const r = run(["save"], reply);
    expect(r.code).toBe(0);
    const s = r.json() as { saved: { title: string; path: string }[]; refused: unknown[] };
    expect(s.refused).toEqual([]);
    expect(s.saved[0]!.title).toBe("From a chat");
    expect(fs.existsSync(s.saved[0]!.path)).toBe(true);
    expect(run(["save"], "nothing here").json().refused.length).toBe(1);
  });

  test("unknown command, version", () => {
    expect(run(["frobnicate"]).code).toBe(2);
    expect(run(["--version"]).out.trim()).toMatch(/^\d+\.\d+\.\d+$/);
  });
});

describe("MCP over HTTP (127.0.0.1, token)", () => {
  let s: HttpServer;
  let token: string;
  beforeAll(async () => {
    s = await startHttp({ port: 0 });
    token = fs.readFileSync(tokenFile(), "utf8").trim();
  });
  afterAll(async () => {
    await s?.close();
  });

  const init = {
    jsonrpc: "2.0",
    id: 1,
    method: "initialize",
    params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "t", version: "0" } },
  };
  const post = (headers: Record<string, string>, body: unknown = init) =>
    fetch(s.url, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json, text/event-stream", ...headers },
      body: JSON.stringify(body),
    });

  test("listens on 127.0.0.1 only and keeps a private token", () => {
    expect(s.url).toMatch(/^http:\/\/127\.0\.0\.1:\d+\/mcp$/);
    expect(token).toMatch(/^[0-9a-f]{64}$/);
    if (process.platform !== "win32") expect(fs.statSync(tokenFile()).mode & 0o077).toBe(0);
  });

  test("refuses without the token, with a wrong one, from a web page, or with a foreign Host", async () => {
    expect((await post({})).status).toBe(401);
    expect((await post({ authorization: "Bearer nope" })).status).toBe(401);
    const page = await post({ authorization: `Bearer ${token}`, origin: "https://evil.example" });
    expect(page.status).toBe(403);
    expect(page.headers.get("access-control-allow-origin")).toBeNull();
    const rebind = await fetch(s.url, { method: "OPTIONS", headers: { origin: "https://evil.example" } });
    expect(rebind.status).toBe(403);
    expect((await fetch(s.url.replace("/mcp", "/other"), { headers: { authorization: `Bearer ${token}` } })).status).toBe(404);
    // DNS rebinding: a page on evil.example resolving to 127.0.0.1 sends its own Host
    const status = await new Promise<number>((resolve, reject) => {
      const u = new URL(s.url);
      const req = http.request(
        { host: u.hostname, port: u.port, path: u.pathname, method: "POST", headers: { host: `evil.example:${u.port}`, authorization: `Bearer ${token}` } },
        (res) => {
          res.resume();
          resolve(res.statusCode ?? 0);
        },
      );
      req.on("error", reject);
      req.end(JSON.stringify(init));
    });
    expect(status).toBe(403);
  });

  test("initialize, tools, prompts and resources with a real MCP client", async () => {
    const client = new Client({ name: "http-test", version: "0" });
    await client.connect(new StreamableHTTPClientTransport(new URL(s.url), { requestInit: { headers: { authorization: `Bearer ${token}` } } }));
    expect(client.getServerVersion()).toMatchObject({ name: "context-capsule", title: "Context Capsule" });
    const { tools } = await client.listTools();
    expect(tools.length).toBe(Object.keys(TOOL_HINTS).length);
    for (const t of tools) {
      expect(t.annotations).toBeDefined();
      expect(typeof t.annotations!.readOnlyHint).toBe("boolean");
      expect(t.annotations!.openWorldHint).toBe(false);
      if (!t.annotations!.readOnlyHint) {
        expect(typeof t.annotations!.destructiveHint).toBe("boolean");
        expect(typeof t.annotations!.idempotentHint).toBe("boolean");
      }
    }
    expect(tools.find((t) => t.name === "list_capsules")!.outputSchema).toBeDefined();
    const r = await client.callTool({ name: "list_sessions", arguments: {} });
    expect((r.structuredContent as { sessions: { id: string }[] }).sessions[0]!.id).toBe("20261019-100000");
    const st = await client.callTool({ name: "recording_status", arguments: {} });
    expect(st.structuredContent).toMatchObject({ app_running: false, recording: false });
    expect((await client.listPrompts()).prompts.map((p) => p.name)).toContain("make_day_capsules");
    expect((await client.listResourceTemplates()).resourceTemplates[0]!.uriTemplate).toBe("capsule://{slug}");
    await client.close();
  });
});
