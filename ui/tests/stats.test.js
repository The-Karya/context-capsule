// context-capsule — ui: tests for Home's numbers — totals, corrections, streak, duration, greeting, week start.

import { beforeAll, expect, test } from "bun:test";
import * as S from "../js/core/stats.js";

beforeAll(() => {
  process.env.TZ = "Etc/GMT-2";
});

const lines = (...l) => l.map((x) => JSON.stringify(x) + "\n").join("");
const PROJECTS = [{ id: "web", name: "Website", match: ["linear.app"] }];

test("corrections", () => {
  const fixes = S.parseAssign(`{"assign":[{"project":"a"},{"project":"b","from":"2026-10-19T10:00:00Z","to":"2026-10-19T11:00:00Z"}]}`);
  expect(fixes.length).toBe(2);
  expect(S.assignedAt(fixes, Date.parse("2026-10-19T10:30:00Z"))).toBe("b");
  expect(S.assignedAt(fixes, Date.parse("2026-10-19T11:00:00Z"))).toBe("a");
  expect(S.parseAssign(`{"assign":[{"project":"a"}, 3]}`)).toEqual([]);
  expect(S.parseAssign("nope")).toEqual([]);
  expect(S.assignedAt([], 0)).toBeNull();
});

test("totals", () => {
  const files = {
    "20261020-093000": {
      events: lines(
        { t: "2026-10-20T07:30:00Z", end: "2026-10-20T07:40:00Z", app: "Chrome", title: "x", where: "linear.app/a" }, // web by clue, 600
        { t: "2026-10-20T07:40:00Z", end: "2026-10-20T07:40:00Z", app: "Code", title: "y", project: "app" }, // 3 s at least
        { t: "2026-10-20T07:41:00Z", end: "2026-10-20T07:50:00Z", app: "Figma", title: "z", focus: "pointer" }, // looked at: no
        { t: "2026-10-20T07:42:00Z", end: "2026-10-20T07:43:00Z", kind: "voice", text: "hi" }, // a note: no
        { t: "2026-10-20T07:50:00Z", end: "2026-10-20T08:00:00Z", app: "Code", title: "y", project: "app" }, // corrected to ""
        { t: "2026-10-20T07:50:00.5Z", end: "2026-10-20T08:00:00Z", app: "Code" }, // not whole seconds: skipped
      ),
      assign: `{"assign":[{"from":"2026-10-20T07:45:00Z","project":""}]}`,
    },
    "20261010-090000": { events: lines({ t: "2026-10-20T07:30:00Z", end: "2026-10-20T09:00:00Z", app: "Old" }) }, // too early an id
    junk: { events: "x" },
  };
  const from = new Date(2026, 9, 20).getTime();
  const to = new Date(2026, 9, 21).getTime();
  const t = S.totals(files, from, to, PROJECTS);
  expect(t.byProject).toEqual([{ id: "web", seconds: 600 }, { id: "", seconds: 600 }, { id: "app", seconds: 3 }].sort((a, b) => b.seconds - a.seconds || (a.id < b.id ? -1 : 1)));
  expect(t.seconds).toBe(1203);
  expect(S.weekCandidates(["20261019-000000", "20261018-235959", "x", "20261022-101010"], from)).toEqual(["20261019-000000", "20261022-101010"]);
});

test("streak, duration, greeting", () => {
  const now = new Date(2026, 9, 23, 10).getTime();
  expect(S.streak(["20261023-090000", "20261022-090000", "20261021-235959", "20261019-090000"], now)).toBe(3);
  expect(S.streak(["20261022-090000", "20261021-090000"], now)).toBe(2);
  expect(S.streak(["20261021-090000"], now)).toBe(0);
  expect(S.duration(0.5)).toBe("0 min");
  expect(S.duration(30)).toBe("under 1 min");
  expect(S.duration(3900)).toBe("1 h 5 min");
  expect([S.greeting(11), S.greeting(12), S.greeting(17), S.greeting(18)]).toEqual(["Good morning", "Good afternoon", "Good afternoon", "Good evening"]);
});

test("week start", () => {
  const fri = new Date(2026, 9, 23, 15).getTime();
  expect(S.weekStart(fri, 2)).toBe(new Date(2026, 9, 19).getTime());
  expect(S.weekStart(fri, 1)).toBe(new Date(2026, 9, 18).getTime());
  const sun = new Date(2026, 9, 25, 8).getTime();
  expect(S.weekStart(sun, 2)).toBe(new Date(2026, 9, 19).getTime());
  expect(S.weekStart(sun, 1)).toBe(new Date(2026, 9, 25).getTime());
  expect(S.firstWeekday("en-US")).toBe(1);
  expect(S.firstWeekday("en-GB")).toBe(2);
  expect(S.sessionID(new Date(2026, 9, 23, 9, 5, 7).getTime())).toBe("20261023-090507");
});
