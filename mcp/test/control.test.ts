// context-capsule — mcp: tests for app control (control file, state.json, start/stop tools)
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import fs from "node:fs";
import path from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { appRunning, describeSkip, readState, writeControl } from "../src/control.ts";
import { words } from "../src/platform.ts";
import { createServer } from "../src/server.ts";
import { ev, tempHome, writeFixtureSession, writeSession } from "./helpers.ts";

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
    expect(s).toEqual({
      recording: true,
      session_id: "20261019-100000",
      started_at: "2026-10-19T10:00:00Z",
      pid: process.pid,
      picking: false,
      last_error: null,
      events: null,
      skip: null,
      listening: false,
      note_at: null,
      project: null,
      project_by: null,
    });
    writeState({ recording: true, pid: process.pid, events: 4, skip: "not_picked" });
    expect(readState()).toMatchObject({ events: 4, skip: "not_picked" });
    expect(describeSkip("not_picked")).toContain("not part of");
    expect(describeSkip("new_reason")).toBe("new_reason");
    writeState({ recording: false, pid: process.pid, picking: true, last_error: "cancelled" });
    expect(readState()).toMatchObject({ picking: true, last_error: "cancelled" });
    expect(appRunning(s)).toBe(true);
    expect(appRunning({ ...s, pid: 999999 })).toBe(false);
  });
});

describe("tools", () => {
  test("start_recording explains how to open the app when it isn't running", async () => {
    const r = await call("start_recording");
    expect(r.isError).toBe(true);
    expect(r.text).toContain(words().openApp);
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

  test("start_recording says the picker is open while the user chooses", async () => {
    writeState({ recording: false, session_id: null, started_at: null, pid: process.pid, picking: false, last_error: null });
    const timer = setTimeout(() => {
      writeState({ recording: false, session_id: null, started_at: null, pid: process.pid, picking: true, last_error: null });
    }, 300);
    const r = await call("start_recording");
    clearTimeout(timer);
    expect(readControl().command).toBe("start");
    expect(r.isError).toBe(false);
    expect(r.text).toContain(`${words().picker} is open`.replace(/^t/, "T"));
  });

  test("start_recording reports a cancelled picker", async () => {
    writeState({ recording: false, session_id: null, started_at: null, pid: process.pid, picking: false, last_error: null });
    const timer = setTimeout(() => {
      writeState({ recording: false, session_id: null, started_at: null, pid: process.pid, picking: false, last_error: "cancelled" });
    }, 300);
    const r = await call("start_recording");
    clearTimeout(timer);
    expect(r.isError).toBe(true);
    expect(r.text).toContain("picker was closed");
  });

  test("start_recording ignores a last_error left from before, and never mentions Screen Recording", async () => {
    writeState({ recording: false, session_id: null, started_at: null, pid: process.pid, picking: false, last_error: "cancelled" });
    const r = await call("start_recording"); // the app never answers
    expect(r.isError).toBe(false);
    expect(r.text).toContain("hasn't answered yet");
    expect(r.text).not.toContain("Screen Recording");
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

    writeState({ recording: false, pid: process.pid, picking: true, last_error: null });
    expect((await call("recording_status")).text).toContain("Picker: open");
    writeState({ recording: false, pid: process.pid, picking: false, last_error: "cancelled" });
    const c = await call("recording_status");
    expect(c.text).toContain("Recording: no");
    expect(c.text).toContain("picker closed without a choice");
  });

  test("list_sessions shows where the text came from", async () => {
    writeSession(home, "20261019-090000", [
      { ...ev(0, "Notes", "A", "alpha line here"), src: "ax" },
      { ...ev(1, "Notes", "B", "beta line here"), src: "ax" },
      { ...ev(2, "Preview", "C", "gamma line here"), src: "ax" },
      { ...ev(3, "Preview", "D", "delta line here"), src: "ocr" },
    ]);
    writeFixtureSession(home, "20261019-100000"); // older format, no src
    const r = await call("list_sessions");
    expect(r.text).toContain("(text: 75% accessibility, 25% OCR)");
    expect(r.text.split("\n").find((l) => l.includes("20261019-100000"))).not.toContain("text:");
    const a = await call("get_session_activity", { session_id: "20261019-090000" });
    expect(a.text).toContain("Accessibility (3 samples) and OCR (1 sample,");
  });

  test("add_note sends the note to the app and waits for it", async () => {
    writeState({ recording: true, session_id: "20261019-120000", started_at: "2026-10-19T12:00:00Z", pid: process.pid });
    const timer = setTimeout(() => {
      const c = readControl() as { command: string; text: string; at: number };
      expect(c.command).toBe("note");
      expect(c.text).toBe("We chose Postgres because the team knows it");
      writeState({ recording: true, session_id: "20261019-120000", pid: process.pid, note_at: c.at });
    }, 300);
    const r = await call("add_note", { text: "  We chose Postgres because the team knows it  " });
    clearTimeout(timer);
    expect(r.isError).toBe(false);
    expect(r.text).toContain("Added the note to session 20261019-120000");
  });

  test("add_note needs a recording", async () => {
    const off = await call("add_note", { text: "hello" });
    expect(off.isError).toBe(true);
    expect(off.text).toContain("isn't running");
    writeState({ recording: false, pid: process.pid });
    const idle = await call("add_note", { text: "hello" });
    expect(idle.isError).toBe(true);
    expect(idle.text).toContain("Not recording right now");
    expect(fs.existsSync(path.join(home, "control"))).toBe(false);
  });

  test("recording_status shows a voice note in progress", async () => {
    writeState({ recording: true, session_id: "s", started_at: new Date().toISOString(), pid: process.pid, listening: true });
    expect((await call("recording_status")).text).toContain("Voice note: listening");
  });

  test("list_sessions counts notes, and they are not screen samples", async () => {
    writeSession(home, "20261019-090000", [
      { ...ev(0, "Figma", "A", "alpha line here"), src: "ax" },
      { ...ev(1, "Figma", "A", "We keep three tiers"), kind: "voice", src: "voice" },
      { ...ev(2, "", "", "typed context"), kind: "note", src: "typed" },
    ]);
    const line = (await call("list_sessions")).text.split("\n").find((l) => l.includes("20261019-090000"))!;
    expect(line).toContain("1 samples");
    expect(line).toContain("2 notes");
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
    expect(msg.text).not.toContain("## Summary");
    expect(msg.text).toContain("## For Claude");
    expect(msg.text).toContain("context for another Claude");
    expect(msg.text).toContain("are the user's own words: the primary source for decisions, reasons and intent");

    await call("save_capsule", { title: "Res test", markdown: "## Summary\nhi" });
    const list = await client.listResources();
    expect(list.resources.map((r) => r.uri)).toContain("capsule://res-test");
    const read = await client.readResource({ uri: "capsule://res-test" });
    expect((read.contents[0] as { text: string }).text).toContain("title: Res test");
  });
});
