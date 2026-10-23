// context-capsule — ui: tests for capsules.js (ports of capture-rs/core/src/capsules.rs and reminder.rs tests).
import { test, expect } from "bun:test";
import * as C from "../js/core/capsules.js";

// A stand-in for dates.js: today is 2026-10-20.
const dates = {
  shortDay(ymd) {
    if (ymd === "2026-10-20") return "Today";
    if (ymd === "2026-10-19") return "Yesterday";
    const [, m, d] = ymd.split("-").map(Number);
    return `${d} ${["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][m - 1]}`;
  },
};
const now = Date.UTC(2026, 9, 20, 12);

function cap(project, kind, date, created) {
  return { name: `${project}-${kind}-${date}.capsule.md`, path: "", title: `${project} ${date}`, project, kind, date, created, author: "", inShort: "", draft: false };
}

test("windows line ends", () => {
  const text = "---\r\ncapsule: 1\r\ntitle: Pricing page\r\nproject: website\r\nkind: daily\r\ndate: 2026-10-19\r\n---\r\n\r\n## In short\r\nTeams moves to $12.\r\n\r\n## Next steps\r\n- Build it.\r\n";
  const c = C.parse(text, "website-2026-10-19.capsule.md");
  expect([c.title, c.project, c.kind, c.date]).toEqual(["Pricing page", "website", "daily", "2026-10-19"]);
  expect(c.inShort).toBe("Teams moves to $12.");
  expect(C.isCapsule(text)).toBe(true);
});

test("front matter: quotes, comments, last key wins, draft", () => {
  const c = C.parse("---\ntitle: 'A: b'\nauthor: S   # note\nproject: \"web\"\nproject: app\ndraft: true\n---\nbody", "x.capsule.md");
  expect([c.title, c.author, c.project, c.draft]).toEqual(["A: b", "S", "app", true]);
  expect(C.parse("---\ndraft: yes\n---\n", "x.capsule.md").draft).toBe(false);
});

test("in short: case-insensitive heading, lines joined, stops at next section", () => {
  const c = C.parse("---\ntitle: T\n---\n## IN SHORT\n  One.\n\n\tTwo.\n## Next\nNo.", "t.capsule.md");
  expect(c.inShort).toBe("One. Two.");
  expect(C.parse("no front matter", "a.md").inShort).toBe("");
});

test("fallback title is the file name without .capsule.md / .md", () => {
  expect(C.parse("hello", "Pricing page.capsule.md").title).toBe("Pricing page");
  expect(C.parse("---\ntitle:\n---\n", "notes.md").title).toBe("notes");
  expect(C.parse("x", "/a/b/plain").title).toBe("plain");
});

test("isCapsule", () => {
  expect(C.isCapsule("﻿---\ncapsule: 1\ntitle: x\n---\n")).toBe(true);
  expect(C.isCapsule("---\ntitle: x\n---\n")).toBe(false);
  expect(C.isCapsule("---\ncapsule: 1\ntitle: x\n")).toBe(false);
  expect(C.isCapsule("# hello\n---\ncapsule: 1\ntitle: x\n---")).toBe(false);
});

test("all: only capsule files, newest first", () => {
  const list = C.all([
    { name: "a.capsule.md", text: "---\ntitle: A\ncreated: 2026-10-18T10:00:00Z\n---\n" },
    { name: "b.capsule.md", text: "---\ntitle: B\ncreated: 2026-10-19T10:00:00Z\n---\n" },
    { name: "c.capsule.md", text: "---\ntitle: C\ncreated: 2026-10-19T10:00:00Z\n---\n" },
    { name: "notes.md", text: "---\ntitle: N\ncreated: 2027-01-01T00:00:00Z\n---\n" },
  ]);
  expect(list.map((c) => c.title)).toEqual(["C", "B", "A"]);
  expect(C.recent(list, 2).map((c) => c.title)).toEqual(["C", "B"]);
});

test("labels", () => {
  const c = cap("web", "project", "", "2026-10-19T18:00:00Z");
  expect(C.label(c, dates, now)).toBe("Whole project");
  expect(C.when(c, dates, now)).toBe("Yesterday");
  c.kind = "daily";
  c.date = "2026-10-20";
  expect([C.label(c, dates, now), C.when(c, dates, now)]).toEqual(["Today", ""]);
  c.date = "someday";
  expect(C.label(c, dates, now)).toBe("someday");
  c.kind = "";
  expect(C.label(c, dates, now)).toBe("web ");
  c.created = "2026-10-05T09:00:00Z";
  expect(C.when(c, dates, now)).toBe("5 Oct");
  c.created = "short";
  expect(C.when(c, dates, now)).toBe("");
});

test("search box", () => {
  const c = cap("web", "daily", "2026-10-19", "2026-10-19T18:00:00Z");
  c.title = "Pricing page";
  c.inShort = "Moved Teams to $12 after the Acmé call.";
  expect(C.matches(c, "") && C.matches(c, "  ")).toBe(true);
  expect(C.matches(c, "pricing") && C.matches(c, "ACME teams") && C.matches(c, "PRÍCING")).toBe(true);
  expect(C.matches(c, "pricing figma")).toBe(false);
});

test("grouped by project", () => {
  const names = { web: "Website", app: "app" };
  const nameOf = (id) => names[id] ?? id;
  const list = [
    cap("web", "daily", "2026-10-18", "2026-10-18T18:00:00Z"),
    cap("", "", "", "2026-10-19T09:00:00Z"),
    cap("web", "daily", "2026-10-19", "2026-10-19T18:00:00Z"),
    cap("app", "daily", "2026-10-19", "2026-10-19T18:00:00Z"),
    cap("web", "project", "", "2026-10-17T18:00:00Z"),
  ];
  const g = C.groups(list, "", nameOf);
  expect(g.map((x) => x.title)).toEqual(["app", "Website", "Other"]);
  expect(g[1].items.map((c) => [c.kind, c.date])).toEqual([["project", ""], ["daily", "2026-10-19"], ["daily", "2026-10-18"]]);
  expect(C.groups(list, "2026-10-18", nameOf).map((x) => x.title)).toEqual(["Website"]);
  expect(C.weekCount(list, Date.parse("2026-10-19T00:00:00Z"))).toBe(3);
});

test("prompts", () => {
  expect(C.dayPrompt).toBe("Use Context Capsule to make my capsules for today.");
  expect(C.continuePrompt("/a/b.capsule.md")).toBe("Use Context Capsule to continue from the capsule at /a/b.capsule.md.");
});
