// karya-capsule — mcp: tests for app control (control file, state.json, start/stop tools)
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import fs from "node:fs";
import path from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { appRunning, readState, writeControl } from "../src/control.ts";
import { createServer } from "../src/server.ts";
import { tempHome, writeFixtureSession } from "./helpers.ts";

let home: string;
let client: Client;

function writeState(s: object) {
  fs.writeFileSync(path.join(home, "state.json"), JSON.stringify(s));
}
const readControl = () => JSON.parse(fs.readFileSync(path.join(home, "control"), "utf8"));

async function call(name: string, args: Record<string, unknown> = {}) {
  const r = (await client.callTool({ name, arguments: args })) as { content: { text: string }[]; isError?: boolean };
  return { text: r.content.map((c) => c.text).join("\n"), isError: !!r.isError };
}

beforeEach(async () => {
  home = tempHome();
  const server = createServer({ confirmTimeoutMs: 1500 });
  const [a, b] = InMemoryTransport.createLinkedPair();
  client = new Client({ name: "test", version: "0" });
  await Promise.all([server.connect(a), client.connect(b)]);
});
afterEach(async () => {
  await client.close();
});

describe("files", () => {
  test("writeControl writes the JSON command", () => {
    writeControl("start", 123);
    expect(readControl()).toEqual({ command: "start", at: 123 });
    writeControl("stop");
    expect(readControl().command).toBe("stop");
    expect(typeof readControl().at).toBe("number");
  });

  test("readState parses state.json and checks the pid", () => {
    expect(readState()).toBeNull();
    writeState({ recording: true, session_id: "20261019-100000", started_at: "2026-10-19T10:00:00Z", pid: process.pid });
    const s = readState()!;
    expect(s).toEqual({ recording: true, session_id: "20261019-100000", started_at: "2026-10-19T10:00:00Z", pid: process.pid });
    expect(appRunning(s)).toBe(true);
    expect(appRunning({ ...s, pid: 999999 })).toBe(false);
  });
});

describe("tools", () => {
  test("start_recording explains how to open the app when it isn't running", async () => {
    const r = await call("start_recording");
    expect(r.isError).toBe(true);
    expect(r.text).toContain("open -a Capsule");
    expect(fs.existsSync(path.join(home, "control"))).toBe(false);
  });

  test("start_recording writes control and waits for the app", async () => {
    writeState({ recording: false, session_id: null, started_at: null, pid: process.pid });
    // pretend to be the app: react to the control file
    const timer = setTimeout(() => {
      expect(readControl().command).toBe("start");
      writeState({ recording: true, session_id: "20261019-120000", started_at: "2026-10-19T12:00:00Z", pid: process.pid });
    }, 300);
    const r = await call("start_recording");
    clearTimeout(timer);
    expect(r.isError).toBe(false);
    expect(r.text).toContain("Recording started: session 20261019-120000");
  });

  test("stop_recording writes stop and returns the session id", async () => {
    writeState({ recording: true, session_id: "20261019-120000", started_at: "2026-10-19T12:00:00Z", pid: process.pid });
    const timer = setTimeout(() => {
      writeState({ recording: false, session_id: null, started_at: null, pid: process.pid });
    }, 300);
    const r = await call("stop_recording");
    clearTimeout(timer);
    expect(readControl().command).toBe("stop");
    expect(r.text).toContain("Session id: 20261019-120000");
    expect(r.text).toContain("get_session_activity");
  });

  test("recording_status reads state.json", async () => {
    expect((await call("recording_status")).text).toContain("has not run yet");
    writeState({ recording: true, session_id: "20261019-120000", started_at: new Date().toISOString(), pid: process.pid });
    const r = await call("recording_status");
    expect(r.text).toContain("Recording: yes");
    expect(r.text).toContain("Session: 20261019-120000");
  });

  test("unknown session gives a friendly error", async () => {
    writeFixtureSession(home, "20261019-100000");
    const r = await call("get_session_activity", { session_id: "20200101-000000" });
    expect(r.isError).toBe(true);
    expect(r.text).toContain("Recent sessions: 20261019-100000");
    const bad = await call("get_session_activity", { session_id: "../etc" });
    expect(bad.isError).toBe(true);
  });

  test("prompt and resources", async () => {
    const p = await client.getPrompt({ name: "make_capsule", arguments: { session_id: "20261019-100000" } });
    const msg = p.messages[0]!.content as { text: string };
    expect(msg.text).toContain("get_session_activity");
    expect(msg.text).toContain("## Decisions");
    expect(msg.text).toContain("1500 words");

    await call("save_capsule", { title: "Res test", markdown: "## Summary\nhi" });
    const list = await client.listResources();
    expect(list.resources.map((r) => r.uri)).toContain("capsule://res-test");
    const read = await client.readResource({ uri: "capsule://res-test" });
    expect((read.contents[0] as { text: string }).text).toContain("title: Res test");
  });
});
