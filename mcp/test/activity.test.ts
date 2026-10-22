// context-capsule — mcp: tests for the activity timeline (grouping, dedupe, noise, budget)
import { beforeEach, describe, expect, test } from "bun:test";
import fs from "node:fs";
import path from "node:path";
import {
  buildSegments,
  collapse,
  condenseActivity,
  condenseDay,
  fairShares,
  isNoiseLine,
  localDay,
  momentText,
  trimTo,
} from "../src/activity.ts";
import { loadDay } from "../src/day.ts";
import { readEvents, readGaps, readSessionMeta, formatClock, type CaptureEvent } from "../src/sessions.ts";
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

describe("voice and typed notes", () => {
  const meta = { id: "20261019-100000", started_at: at(0), ended_at: at(12) };
  const voice = (min: number, text: string): CaptureEvent => ({
    t: at(min),
    end: at(min, 20),
    kind: "voice",
    src: "voice",
    app: "Figma",
    bundle: "com.figma.Desktop",
    title: "Pricing v2",
    text,
  });
  const typed = (min: number, text: string): CaptureEvent => ({ t: at(min), end: at(min), kind: "note", src: "typed", app: "", title: "", text });

  test("notes show inline, in time order, in the user's words", () => {
    const events = [
      ...fixtureEvents().slice(0, 3),
      voice(4, "We drop the Starter tier\nbecause nobody   buys it"),
      ...fixtureEvents().slice(3),
      typed(11, "Ship the new page on Monday"),
    ].sort((a, b) => Date.parse(a.t) - Date.parse(b.t));
    const out = condenseActivity(meta, events);
    const said = `>> You said (${formatClock(at(4))}): "We drop the Starter tier because nobody buys it"`;
    const wrote = `>> You typed (${formatClock(at(11))}): "Ship the new page on Monday"`;
    expect(out).toContain(said);
    expect(out).toContain(wrote);
    expect(out).toContain("1 voice note and 1 typed note");
    // after the Figma segment at 10:03, before the Notion segment at 10:06
    expect(out.indexOf(said)).toBeGreaterThan(out.indexOf("Hero: Simple pricing for teams"));
    expect(out.indexOf(said)).toBeLessThan(out.indexOf("Notion Plus is $10 per seat"));
    expect(out.indexOf(wrote)).toBeGreaterThan(out.indexOf("Yearly saves 20%"));
  });

  test("notes are not screen samples: no segment, no app time, no dedupe against screen text", () => {
    const events = [ev(0, "Figma", "A", "Drop the Starter tier"), voice(1, "Drop the Starter tier")];
    const segs = buildSegments(events);
    expect(segs).toHaveLength(1);
    const out = condenseActivity(meta, events);
    expect(out).toContain(`>> You said (${formatClock(at(1))}): "Drop the Starter tier"`);
    expect(out).toContain("1 segments from 1 screen samples");
  });

  test("a note splits a segment of the same window", () => {
    const events = [ev(0, "Figma", "A", "first line here"), voice(1, "a reason"), ev(2, "Figma", "A", "second line here")];
    expect(buildSegments(events)).toHaveLength(2);
  });

  test("notes get the budget first: screen text is cut, the words are not", () => {
    const long = Array.from({ length: 400 }, (_, i) => `screen line number ${i} with words`).join("\n");
    const said = "We keep annual pricing by default because seventy percent of trials pick it";
    const events = [ev(0, "Chrome", "Docs", long), voice(5, said), ev(6, "Chrome", "Docs 2", long + " again")];
    const out = condenseActivity(meta, events, { maxChars: 1500 });
    expect(out.length).toBeLessThanOrEqual(1500);
    expect(out).toContain(said);
    expect(out).toContain("…");
  });

  test("when even the headers don't fit, segments go before notes", () => {
    const many = Array.from({ length: 60 }, (_, i) => ev(i, `App${i}`, `Window ${i}`, `text for window ${i}`));
    const said = "The one thing that matters is the reason for this decision";
    const out = condenseActivity(meta, [...many, voice(61, said)], { maxChars: 700 });
    expect(out.length).toBeLessThanOrEqual(700);
    expect(out).toContain(said);
    expect(out).toContain("notes kept");
  });

  test("readEvents keeps kind and the note's empty app", () => {
    const home = tempHome();
    writeFixtureSession(home);
    const dir = `${home}/sessions/20261019-100000/events.jsonl`;
    require("node:fs").appendFileSync(dir, JSON.stringify(typed(20, "typed thing")) + "\n" + JSON.stringify(voice(21, "said thing")) + "\n");
    const evs = readEvents("20261019-100000");
    const notes = evs.filter((e) => e.kind);
    expect(notes.map((e) => `${e.kind}|${e.src}|${e.app}`)).toEqual(["note|typed|", "voice|voice|Figma"]);
    expect(evs.filter((e) => !e.kind).every((e) => e.app)).toBe(true);
  });
});

describe("get_moment: look closer at a stretch", () => {
  test("everything in the range, in full, with notes; nothing outside it", async () => {
    const { momentText } = await import("../src/activity.ts");
    const long = Array.from({ length: 300 }, (_, i) => `document line ${i}`).join("\n");
    const events: CaptureEvent[] = [
      ev(0, "Pages", "Plan", "before the range", 1),
      ev(5, "Pages", "Plan", long, 1),
      { t: at(6), end: at(6, 10), kind: "voice", src: "voice", app: "Pages", title: "Plan", text: "keep section 3" },
      ev(7, "Pages", "Plan", "", 1),
      ev(30, "Mail", "Inbox", "after the range", 1),
    ];
    const out = momentText("s1", events, Date.parse(at(5)), Date.parse(at(10)), 100_000);
    expect(out).toContain("document line 0");
    expect(out).toContain("document line 299"); // nothing shortened
    expect(out).toContain('>> You said');
    expect(out).toContain("(nothing new on screen)");
    expect(out).not.toContain("before the range");
    expect(out).not.toContain("after the range");
    expect(out).toContain("<screen>");
  });

  test("says where to continue when it doesn't fit", async () => {
    const { momentText } = await import("../src/activity.ts");
    const events = Array.from({ length: 20 }, (_, i) => ev(i, "Pages", `Doc ${i}`, "x".repeat(400), 1));
    const out = momentText("s1", events, Date.parse(at(0)), Date.parse(at(30)), 3000);
    expect(out.length).toBeLessThanOrEqual(3200);
    expect(out).toMatch(/More after .*call get_moment again with from "/);
  });
});

describe("the window under the mouse", () => {
  test("counted as seen, not as time; doesn't split the main run; labelled", () => {
    const looked = (min: number, text: string): CaptureEvent => ({ ...ev(min, "Safari", "Docs", text, 1), focus: "pointer" });
    const events = [ev(0, "Code", "main.ts", "first", 1), looked(1, "api reference"), ev(2, "Code", "main.ts", "second", 1)];
    const segs = buildSegments(events);
    expect(segs.map((s) => `${s.app}${s.lookedAt ? "*" : ""}`)).toEqual(["Code", "Safari*"]);
    expect(segs[0]!.events).toBe(2);
    const out = condenseActivity({ id: "x", started_at: at(0), ended_at: at(3) }, events);
    expect(out).toContain("Safari — Docs (looked at, on another screen)");
    expect(out).toContain("Apps: Code 2m"); // Safari not counted as time
  });
});

describe("gaps: stretches that weren't read (gaps.jsonl)", () => {
  const gap = (from: number, to: number, why: string) => ({ t: at(from), end: at(to), why });
  const writeGaps = (dir: string, lines: string[]) => fs.writeFileSync(path.join(dir, "gaps.jsonl"), lines.join("\n") + "\n");

  test("read if present; bad lines skipped; none when the file is missing", () => {
    const home = tempHome();
    const dir = writeFixtureSession(home);
    expect(readGaps("20261019-100000")).toEqual([]);
    writeGaps(dir, [
      JSON.stringify(gap(5, 7, "locked")),
      "{broken",
      JSON.stringify({ t: "nope", why: "x" }),
      JSON.stringify(gap(2, 3, "private_app")),
    ]);
    expect(readGaps("20261019-100000").map((g) => g.why)).toEqual(["private_app", "locked"]);
  });

  test("one short line each, in time order, in the session view", () => {
    const gaps = [gap(4, 5, "private_app"), gap(11, 12, "locked")];
    const out = condenseActivity({ id: "x", started_at: at(0), ended_at: at(12) }, fixtureEvents(), { gaps });
    const line = `(Not read ${formatClock(at(4))}–${formatClock(at(5))}: a private app was in front)`;
    expect(out).toContain(line);
    expect(out).toContain(`(Not read ${formatClock(at(11))}–${formatClock(at(12))}: the screen was locked)`);
    // after the Figma segment at 10:03, before the Notion one at 10:06
    expect(out.indexOf(line)).toBeGreaterThan(out.indexOf("Pricing v2"));
    expect(out.indexOf(line)).toBeLessThan(out.indexOf("Competitor pricing"));
  });

  test("many gaps stay inside the budget", () => {
    const gaps = Array.from({ length: 300 }, (_, i) => gap(i, i, i % 2 ? "not_shared" : "no_text"));
    const meta = { id: "x", started_at: at(0), ended_at: at(300) };
    for (const max of [800, 3000, 20_000]) {
      const out = condenseActivity(meta, fixtureEvents(), { maxChars: max, gaps });
      expect(out.length).toBeLessThanOrEqual(max);
      expect(out).toContain("Not read");
    }
    const big = condenseActivity(meta, fixtureEvents(), { gaps });
    expect(big).toMatch(/\(271 more stretches not read, /);
    expect(big).toContain("Teams plan should be $12 per seat"); // screen text still there
  });

  test("a session with only gaps says so", () => {
    const out = condenseActivity({ id: "x", started_at: at(0), ended_at: at(5) }, [], { gaps: [gap(0, 5, "system")] });
    expect(out).toContain("(No activity was captured in this session.)");
    expect(out).toContain(": system screen)");
  });

  test("the day view and get_moment list them too", () => {
    const home = tempHome();
    const dir = writeFixtureSession(home);
    writeGaps(dir, [JSON.stringify(gap(4, 5, "not_shared"))]);
    const d = loadDay(localDay(at(0)));
    expect(d.gaps.length).toBe(1);
    const day = condenseDay(d.day, d.events, d.sessionIds, { gaps: d.gaps });
    expect(day).toContain(`(Not read ${formatClock(at(4))}–${formatClock(at(5))}: this window wasn't shared)`);
    const gaps = readGaps("20261019-100000");
    expect(momentText("20261019-100000", d.events, Date.parse(at(3)), Date.parse(at(6)), 50_000, gaps)).toContain(
      "this window wasn't shared",
    );
    expect(momentText("20261019-100000", d.events, Date.parse(at(7)), Date.parse(at(9)), 50_000, gaps)).not.toContain("Not read");
  });

  test("a moment deleted in the timeline is simply gone", () => {
    const home = tempHome();
    const dir = writeFixtureSession(home);
    const file = path.join(dir, "events.jsonl");
    const lines = fs.readFileSync(file, "utf8").split("\n").filter(Boolean);
    fs.writeFileSync(file, lines.filter((l) => !l.includes("Notion Plus")).join("\n") + "\n");
    const out = condenseActivity(readSessionMeta("20261019-100000"), readEvents("20261019-100000"));
    expect(out).not.toContain("Notion Plus");
    expect(out).toContain("Drop the Starter tier");
  });
});
