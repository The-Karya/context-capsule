// context-capsule — mcp: tests for the activity timeline (grouping, dedupe, noise, budget)
import { beforeEach, describe, expect, test } from "bun:test";
import {
  buildSegments,
  collapse,
  condenseActivity,
  fairShares,
  isNoiseLine,
  trimTo,
} from "../src/activity.ts";
import { readEvents, readSessionMeta, formatClock, type CaptureEvent } from "../src/sessions.ts";
import { at, ev, fixtureEvents, tempHome, writeFixtureSession } from "./helpers.ts";

describe("noise and whitespace", () => {
  test("collapse squashes whitespace", () => {
    expect(collapse("  a \t b\n\n c  ")).toBe("a b c");
  });

  test("drops tiny, numeric, clock and date lines", () => {
    for (const l of ["OK", "42", "10:14 AM", "+12.5%", "• • •", "Oct 19, 2026", "Monday 19", "Today at 9:41", "3/4"])
      expect(isNoiseLine(l)).toBe(true);
  });

  test("keeps real lines", () => {
    for (const l of ["Marketing 2026", "Drop the Starter tier", "May launch plan", "Teams plan $12"])
      expect(isNoiseLine(l)).toBe(false);
  });
});

describe("segments", () => {
  test("groups consecutive events by app + title", () => {
    const segs = buildSegments([
      ev(0, "Chrome", "A", "first page text"),
      ev(1, "Chrome", "A", "more text here"),
      ev(2, "Figma", "B", "design text"),
      ev(3, "Chrome", "A", "back again text"),
    ]);
    expect(segs.map((s) => `${s.app}|${s.title}|${s.events}`)).toEqual(["Chrome|A|2", "Figma|B|1", "Chrome|A|1"]);
    expect(segs[0]!.start).toBe(at(0));
    expect(segs[0]!.end).toBe(at(2));
  });

  test("each line appears once, where first seen (case and spacing ignored)", () => {
    const segs = buildSegments(fixtureEvents());
    const all = segs.flatMap((s) => s.lines);
    expect(all.filter((l) => l.toLowerCase() === "teams plan should be $12 per seat")).toHaveLength(1);
    expect(segs[0]!.lines).toContain("Teams plan should be $12 per seat");
    expect(segs[0]!.lines).toContain("Drop the Starter tier");
    // Linear segment only has its new line
    const linear = segs.find((s) => s.app === "Linear")!;
    expect(linear.lines).toEqual(["Decision: keep yearly discount at 20%"]);
  });

  test("drops menu chrome repeated across many windows, and noise lines", () => {
    const all = buildSegments(fixtureEvents()).flatMap((s) => s.lines);
    expect(all.some((l) => l.startsWith("File Edit View"))).toBe(false);
    expect(all).not.toContain("42");
    expect(all).not.toContain("10:01");
    expect(all).not.toContain("OK");
    expect(all).not.toContain("Oct 19, 2026");
  });

  test("window title is not repeated as body text", () => {
    const segs = buildSegments([ev(0, "Chrome", "Roadmap Q4", "Roadmap Q4\nShip pricing first")]);
    expect(segs[0]!.lines).toEqual(["Ship pricing first"]);
  });
});

describe("budget", () => {
  test("fairShares water-fills: short items whole, long split the rest", () => {
    expect(fairShares([10, 1000, 1000], 310)).toEqual([10, 150, 150]);
    expect(fairShares([5, 5], 100)).toEqual([5, 5]);
    expect(fairShares([100, 100], 0)).toEqual([0, 0]);
  });

  test("trimTo cuts with an ellipsis", () => {
    expect(trimTo("hello world", 50)).toBe("hello world");
    const t = trimTo("alpha beta gamma delta", 12);
    expect(t.endsWith("…")).toBe(true);
    expect(t.length).toBeLessThanOrEqual(12);
  });

  test("output fits max_chars and every segment keeps a fair share", () => {
    const words = (seed: string, n: number) =>
      Array.from({ length: n }, (_, i) => `${seed} line number ${i} with some words`).join("\n");
    const events: CaptureEvent[] = [
      ev(0, "Chrome", "Huge doc", words("alpha", 400)),
      ev(5, "Figma", "Small", "A small note about buttons"),
      ev(6, "Linear", "Also huge", words("beta", 400)),
    ];
    const out = condenseActivity({ id: "x", started_at: at(0), ended_at: at(7) }, events, { maxChars: 3000 });
    expect(out.length).toBeLessThanOrEqual(3000);
    expect(out).toContain("A small note about buttons"); // short segment kept whole
    expect(out).toContain("alpha line number 0");
    expect(out).toContain("beta line number 0");
    expect(out).toContain("…");
    const alpha = out.split("\n").filter((l) => l.startsWith("alpha")).length;
    const beta = out.split("\n").filter((l) => l.startsWith("beta")).length;
    expect(Math.abs(alpha - beta)).toBeLessThanOrEqual(2);
  });

  test("too many segments for the budget: whole segments cut with a note", () => {
    const events = Array.from({ length: 200 }, (_, i) => ev(i % 50, `App${i}`, `Window ${i}`, `text for window ${i}`));
    const out = condenseActivity({ id: "x", started_at: at(0), ended_at: at(60) }, events, { maxChars: 1500 });
    expect(out.length).toBeLessThanOrEqual(1500);
    expect(out).toMatch(/more segments cut to fit max_chars/);
  });
});

describe("condenseActivity on a fixture session", () => {
  let home: string;
  beforeEach(() => {
    home = tempHome();
  });

  test("readable timeline with local HH:MM spans", () => {
    writeFixtureSession(home, "20261019-100000");
    const id = "20261019-100000";
    const out = condenseActivity(readSessionMeta(id), readEvents(id));
    expect(out).toContain("# Session 20261019-100000");
    expect(out).toContain(`## ${formatClock(at(0))}–${formatClock(at(2))} · Google Chrome — Pricing — Linear`);
    expect(out).toContain("· Figma — Pricing v2");
    expect(out).toContain("Apps: Google Chrome");
    expect(out).not.toContain("File Edit View");
  });

  test("empty session says so", () => {
    const out = condenseActivity({ id: "e", started_at: at(0), ended_at: at(1) }, []);
    expect(out).toContain("No activity was captured");
  });
});
