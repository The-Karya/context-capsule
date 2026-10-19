// context-capsule — mcp: end-to-end test, spawns the built server with node over stdio
import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { tempHome, writeFixtureSession } from "./helpers.ts";

const root = path.resolve(import.meta.dir, "..");
const entry = path.join(root, "dist", "index.js");
let client: Client;
let home: string;

beforeAll(async () => {
  if (!fs.existsSync(entry)) {
    const b = spawnSync("bun", ["run", "build"], { cwd: root, stdio: "inherit" });
    if (b.status !== 0) throw new Error("build failed");
  }
  home = tempHome();
  writeFixtureSession(home, "20261019-100000");
  client = new Client({ name: "e2e", version: "0" });
  await client.connect(
    new StdioClientTransport({
      command: "node",
      args: [entry],
      env: { PATH: process.env.PATH ?? "", HOME: process.env.HOME ?? "", CONTEXT_CAPSULE_HOME: home },
      stderr: "ignore",
    }),
  );
}, 30000);

afterAll(async () => {
  await client?.close();
});

const textOf = (r: unknown) => (r as { content: { text: string }[] }).content.map((c) => c.text).join("\n");

describe("built server over stdio (node)", () => {
  test("lists all tools", async () => {
    const { tools } = await client.listTools();
    expect(tools.map((t) => t.name).sort()).toEqual(
      [
        "export_capsule",
        "get_capsule",
        "get_session_activity",
        "import_capsule",
        "list_capsules",
        "list_sessions",
        "recording_status",
        "save_capsule",
        "start_recording",
        "stop_recording",
      ].sort(),
    );
  });

  test("list_sessions shows the fixture", async () => {
    const t = textOf(await client.callTool({ name: "list_sessions", arguments: {} }));
    expect(t).toContain("20261019-100000");
    expect(t).toContain("Google Chrome");
  });

  test("get_session_activity returns the condensed timeline", async () => {
    const t = textOf(await client.callTool({ name: "get_session_activity", arguments: { session_id: "latest", max_chars: 5000 } }));
    expect(t).toContain("# Session 20261019-100000");
    expect(t).toContain("Figma — Pricing v2");
    expect(t).toContain("Decision: keep yearly discount at 20%");
    expect(t).not.toContain("File Edit View");
    expect(t.length).toBeLessThanOrEqual(5000);
  });

  test("lists the make_capsule prompt", async () => {
    const { prompts } = await client.listPrompts();
    expect(prompts.map((p) => p.name)).toContain("make_capsule");
  });
});
