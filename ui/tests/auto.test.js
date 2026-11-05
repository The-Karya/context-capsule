// context-capsule — ui: Automatic capsules — reading auto.status and what the make control shows (button, status
// line, running)

import { expect, test } from "bun:test";
import * as A from "../js/core/auto.js";

const at = (hhmm) => new Date(`2026-10-23T${hhmm}:00`).getTime();
const time = (ms) => {
  const d = new Date(ms);
  const h = d.getHours() % 12 || 12;
  return `${h}:${String(d.getMinutes()).padStart(2, "0")} ${d.getHours() < 12 ? "AM" : "PM"}`;
};
const iso = (hhmm) => new Date(at(hhmm)).toISOString();
const agent = { id: "claude-code", name: "Claude Code" };
const st = (o = {}) => A.parseStatus({ mode: "auto", at: "18:00", agent, running: false, autoDay: null, last: null, ...o });

test("parseStatus: safe defaults", () => {
  expect(A.parseStatus(null)).toEqual({ mode: "click", at: "18:00", agent: null, problem: "", running: false, autoDay: null, last: null });
  expect(A.parseStatus({ mode: "auto", at: "7:30", agent: { id: "codex", name: "Codex" } })).toMatchObject({ mode: "auto", at: "07:30", agent: { id: "codex", name: "Codex" } });
  expect(A.parseStatus({ at: "soon" }).at).toBe("18:00");
  const s = A.parseStatus({ last: { day: "2026-10-23", by: "auto", status: "weird", started_at: iso("18:00"), capsules: 2 } });
  expect(s.last).toMatchObject({ day: "2026-10-23", by: "auto", status: "failed", capsules: 2, startedAt: at("18:00") });
});

test("the blue button: When I click, no tool, no host answer", () => {
  expect(A.control(null, at("12:00"), time).kind).toBe("button");
  expect(A.control(st({ mode: "click" }), at("12:00"), time).kind).toBe("button");
  expect(A.control(st({ agent: null }), at("12:00"), time).kind).toBe("button");
});

test("Automatically, before the time: Next, with Make now", () => {
  expect(A.control(st(), at("15:00"), time)).toEqual({ kind: "line", text: "Next at 6:00 PM", action: "make", actionText: "Make now" });
});

test("Automatically, past the time with no run today: the button (want it now)", () => {
  expect(A.control(st(), at("19:00"), time).kind).toBe("button");
  // yesterday's run doesn't count
  const y = st({ autoDay: "2026-10-22", last: { day: "2026-10-22", by: "auto", status: "done", started_at: iso("18:00"), finished_at: iso("18:03"), capsules: 2 } });
  expect(A.control(y, at("19:00"), time).kind).toBe("button");
});

test("made today: automatically, or by a click after the time", () => {
  const done = st({ autoDay: "2026-10-23", last: { day: "2026-10-23", by: "auto", status: "done", started_at: iso("18:00"), finished_at: iso("18:04"), capsules: 3 } });
  expect(A.control(done, at("19:00"), time)).toEqual({ kind: "line", text: "Made automatically at 6:04 PM" });
  const click = st({ last: { day: "2026-10-23", by: "click", status: "done", started_at: iso("18:30"), finished_at: iso("18:33"), capsules: 1 } });
  expect(A.control(click, at("19:00"), time)).toEqual({ kind: "line", text: "Made at 6:33 PM" });
  // a click run in the afternoon: the automatic one still comes
  const early = st({ last: { day: "2026-10-23", by: "click", status: "done", started_at: iso("14:00"), finished_at: iso("14:03"), capsules: 1 } });
  expect(A.control(early, at("15:00"), time).text).toBe("Next at 6:00 PM");
  // made by the next tool: why, so the first one can be fixed
  const note = "Claude Code isn't signed in. Open it once and sign in. So Codex made them.";
  const other = st({ autoDay: "2026-10-23", last: { day: "2026-10-23", by: "auto", status: "done", started_at: iso("18:00"), finished_at: iso("18:04"), capsules: 2, note } });
  expect(A.control(other, at("19:00"), time)).toEqual({ kind: "line", text: "Made automatically at 6:04 PM", detail: note });
});

test("failed: a quiet line with the reason and Try again", () => {
  const failed = st({ autoDay: "2026-10-23", last: { day: "2026-10-23", by: "auto", status: "failed", started_at: iso("18:00"), reason: "Codex isn't signed in." } });
  expect(A.control(failed, at("19:00"), time)).toEqual({ kind: "line", text: "Couldn't make them", detail: "Codex isn't signed in.", action: "retry", actionText: "Try again" });
});

test("running wins, in every mode", () => {
  expect(A.control(st({ running: true }), at("19:00"), time)).toEqual({ kind: "running", text: "Making today's capsules…" });
  expect(A.control(st({ running: true, mode: "click" }), at("10:00"), time).kind).toBe("running");
});

test("Settings and Home words", () => {
  expect(A.settingsNote(st())).toBe("Claude Code makes them in the background, using only Context Capsule.");
  expect(A.settingsNote(st({ agent: null }))).toBeNull();
  expect(A.settingsNote(st({ mode: "click" }))).toBeNull();
  expect(A.homeLine(st())).toBe("Claude Code makes them in the background. Nothing to paste.");
  expect(A.words.needs).toBe("Needs Claude Code, Codex or Gemini CLI on this computer.");
  expect(A.dayOf(at("23:59"))).toBe("2026-10-23");
});

test("tools here, but none can make them now: said ahead of time, in Settings and on Home", () => {
  const problem = "Claude Code isn't signed in. Open it once and sign in.";
  const s = st({ agent: null, problem });
  expect(s.problem).toBe(problem);
  expect(A.settingsProblem(s)).toBe(`Can't make them automatically yet: ${problem}`);
  expect(A.homeLine(s)).toBe(`Can't make them in the background: ${problem}`);
  expect(A.settingsProblem(st({ agent: null }))).toBe(A.words.needs);
  expect(A.settingsProblem(st())).toBeNull();
  expect(A.settingsProblem(st({ mode: "click", agent: null, problem }))).toBeNull();
  // a tool that can: any stale problem is ignored
  expect(A.parseStatus({ agent: { id: "codex", name: "Codex" }, problem }).problem).toBe("");
  // the button still works (Your AI's way) when none can
  expect(A.control(s, at("19:00"), time).kind).toBe("button");
});
