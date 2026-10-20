// context-capsule — mcp: tests for projects (projects.json, detection, corrections, the day view, project capsules)
import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import fs from "node:fs";
import path from "node:path";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { condenseDay, localDay } from "../src/activity.ts";
import { listCapsules, saveCapsule } from "../src/capsules.ts";
import { addAssignment, assignedAt, createProject, detectProject, loadProjects, projectSlug } from "../src/projects.ts";
import { readEvents, summarizeSession, type CaptureEvent } from "../src/sessions.ts";
import { createServer } from "../src/server.ts";
import { VALID_BODY, at, ev, tempHome, writeSession } from "./helpers.ts";

let home: string;
let client: Client;

async function call(name: string, args: Record<string, unknown> = {}) {
  const r = (await client.callTool({ name, arguments: args })) as { content: { text: string }[]; isError?: boolean };
  return { text: r.content.map((c) => c.text).join("\n"), isError: r.isError === true };
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

const withProject = (e: CaptureEvent, project: string, by: "auto" | "you" = "auto"): CaptureEvent => ({ ...e, project, project_by: by });
const day = localDay(at(0));

/** A day: 1 h website (VS Code), 2 h report (Docs), 30 min Slack (no clue). */
function writeDay(): string {
  const events: CaptureEvent[] = [
    { ...ev(0, "Code", "index.html — acme-site", "Hero section copy", 60) },
    { ...ev(60, "Google Chrome", "Q3 report - Google Docs", "Revenue grew 12% in Q3", 120), where: "https://docs.google.com/document/d/1AbC" },
    { t: at(90), end: at(90, 20), kind: "voice", src: "voice", app: "Google Chrome", title: "Q3 report", text: "We lead with revenue because the board asked" },
    ev(180, "Slack", "general", "lunch at 1?", 30),
  ];
  writeSession(home, "20261019-100000", events);
  return "20261019-100000";
}

describe("projects.json", () => {
  test("slug, create, add clues, load", () => {
    expect(projectSlug("Client Report (Q3)!")).toBe("client-report-q3");
    const a = createProject("Client report", ["docs.google.com/document/d/1abc", " x "]);
    expect(a.created).toBe(true);
    expect(a.project.match).toEqual(["docs.google.com/document/d/1abc"]); // 1-letter clue dropped
    const b = createProject("client  report", ["Q3 report", "DOCS.google.com/document/d/1ABC"]);
    expect(b.created).toBe(false);
    expect(loadProjects()).toEqual([{ id: "client-report", name: "Client report", match: ["docs.google.com/document/d/1abc", "Q3 report"] }]);
  });

  test("detection: title, where, app; longest clue wins", () => {
    const ps = [
      { id: "site", name: "Site", match: ["acme-site"] },
      { id: "design", name: "Design", match: ["figma"] },
      { id: "acme-figma", name: "Acme in Figma", match: ["acme figma file"] },
    ];
    expect(detectProject(ps, "index.html — acme-site", "", "Code")?.id).toBe("site");
    expect(detectProject(ps, "x", "https://figma.com/file/1", "Chrome")?.id).toBe("design");
    expect(detectProject(ps, "the acme figma file", "", "Figma")?.id).toBe("acme-figma");
    expect(detectProject(ps, "Inbox", "", "Mail")).toBeNull();
  });

  test("corrections: later entries win, '' un-assigns", () => {
    const list = [{ project: "a" }, { project: "b", from: at(10), to: at(20) }, { project: "", from: at(15) }];
    expect(assignedAt(list, at(5))).toBe("a");
    expect(assignedAt(list, at(12))).toBe("b");
    expect(assignedAt(list, at(16))).toBe("");
    expect(assignedAt([], at(1))).toBeUndefined();
  });
});

describe("events get their project", () => {
  test("recorded project kept; clues added later sort older events; corrections win", () => {
    writeSession(home, "20261019-100000", [
      withProject(ev(0, "Code", "x — acme-site", "a"), "site", "you"),
      ev(1, "Google Chrome", "Q3 report - Google Docs", "b"),
      ev(2, "Slack", "general", "c"),
    ]);
    createProject("Report", ["Q3 report"]);
    let evs = readEvents("20261019-100000");
    expect(evs.map((e) => `${e.project ?? ""}/${e.project_by ?? ""}`)).toEqual(["site/you", "report/later", "/"]);
    addAssignment("20261019-100000", { project: "report", from: at(2), to: at(3) });
    evs = readEvents("20261019-100000");
    expect(evs[2]!.project).toBe("report");
    expect(evs[2]!.project_by).toBe("you");
    expect(summarizeSession("20261019-100000").projects.map((p) => p.project).sort()).toEqual(["report", "site"]);
  });
});

describe("the day view", () => {
  test("time per project, one section each, notes kept, unsorted last with a hint", () => {
    const id = writeDay();
    createProject("Website", ["acme-site"]);
    createProject("Client report", ["docs.google.com/document/d/1abc"]);
    const out = condenseDay(day, readEvents(id), [id]);
    expect(out).toContain("- Client report: 2h 00m");
    expect(out).toContain("- Website: 1h 00m");
    expect(out).toContain("- Not sorted yet: 30m");
    expect(out).toContain("suggest which project");
    const report = out.indexOf("# Project: Client report");
    const site = out.indexOf("# Project: Website");
    const loose = out.indexOf("# Project: Not sorted yet");
    expect(report).toBeGreaterThan(-1);
    expect(site).toBeGreaterThan(report); // most time first
    expect(loose).toBeGreaterThan(site); // unsorted last
    // the voice note was in Chrome on the report doc's day but has no clue of its own: it follows its own project field
    expect(out).toContain("We lead with revenue because the board asked");
    expect(out.slice(site, loose)).toContain("Hero section copy");
    expect(out.slice(site, loose)).not.toContain("Revenue grew");
  });

  test("one project only, and a size limit", () => {
    const id = writeDay();
    createProject("Website", ["acme-site"]);
    const only = condenseDay(day, readEvents(id), [id], { project: "website" });
    expect(only).toContain("# Project: Website");
    expect(only).not.toContain("Revenue grew");
    expect(condenseDay(day, readEvents(id), [id], { maxChars: 600 }).length).toBeLessThanOrEqual(600);
  });
});

describe("project capsules", () => {
  const md = VALID_BODY;

  test("daily capsules are named <project>-<date>; the project capsule is replaced, with history", () => {
    createProject("Website", ["acme-site"]);
    const d = saveCapsule({ title: "Website — 2026-10-19", markdown: md, project: "website", kind: "daily", date: "2026-10-19" });
    expect(d.slug).toBe("website-2026-10-19");
    expect(fs.readFileSync(d.path, "utf8")).toContain("kind: daily");
    const p1 = saveCapsule({ title: "Website", markdown: md, project: "website", kind: "project" });
    expect(p1.slug).toBe("website");
    const p2 = saveCapsule({ title: "Website", markdown: md, project: "website", kind: "project" });
    expect(p2.path).toBe(p1.path);
    expect(fs.readFileSync(p2.path, "utf8")).toContain("Teams is now $12 per seat");
    expect(p2.warnings.join(" ")).toContain(".history");
    expect(fs.readdirSync(path.join(home, "capsules", ".history"))).toHaveLength(1);
    const list = listCapsules();
    expect(list.map((c) => `${c.slug}:${c.kind}`).sort()).toEqual(["website-2026-10-19:daily", "website:project"]);
  });

  test("rules: kinds need a project, daily needs a date", () => {
    expect(() => saveCapsule({ title: "x", markdown: md, kind: "daily" })).toThrow("needs a project");
    expect(() => saveCapsule({ title: "x", markdown: md, project: "p", kind: "daily" })).toThrow("needs its date");
    expect(saveCapsule({ title: "x", markdown: md, project: "ghost", kind: "daily", date: "2026-10-19" }).warnings.join(" ")).toContain(
      "not in projects.json",
    );
  });
});

describe("tools", () => {
  test("create_project, list_projects, assign_project, get_day_activity", async () => {
    const id = writeDay();
    expect((await call("list_projects")).text).toContain("No projects yet");
    expect((await call("create_project", { name: "Website", match: ["acme-site"] })).text).toContain('Created project "Website"');
    expect((await call("create_project", { name: "Chat", match: [] })).text).toContain("Created");
    const fix = await call("assign_project", { session_id: id, project: "chat", from: "13:00" });
    expect(fix.isError).toBe(false);
    expect((await call("assign_project", { session_id: id, project: "nope" })).isError).toBe(true);
    const dayOut = (await call("get_day_activity", { date: day })).text;
    expect(dayOut).toContain("- Website: 1h 00m");
    expect(dayOut).toContain("# Project: Website");
    const list = (await call("list_projects")).text;
    expect(list).toContain("Website (id website)");
    expect(list).toContain("clues: acme-site");
  });

  test("set_project needs the app and a known project", async () => {
    createProject("Website", ["acme-site"]);
    expect((await call("set_project", { project: "nope" })).text).toContain("Create it first");
    expect((await call("set_project", { project: "website" })).text).toContain("isn't running");
    fs.writeFileSync(path.join(home, "state.json"), JSON.stringify({ recording: true, pid: process.pid }));
    const timer = setTimeout(() => {
      const c = JSON.parse(fs.readFileSync(path.join(home, "control"), "utf8"));
      expect(c).toMatchObject({ command: "project", project: "website" });
      fs.writeFileSync(path.join(home, "state.json"), JSON.stringify({ recording: true, pid: process.pid, project: "website", project_by: "you" }));
    }, 200);
    const r = await call("set_project", { project: "Website" });
    clearTimeout(timer);
    expect(r.text).toContain("Now recording into project website");
  });

  test("the day prompt uses the guided tools", async () => {
    const p = await client.getPrompt({ name: "make_day_capsules", arguments: { date: "2026-10-19" } });
    const t = (p.messages[0]!.content as { text: string }).text;
    expect(t).toContain('Call prepare_day with date "2026-10-19"');
    expect(t).toContain("Call finish_day");
    expect(t).toContain("sort anything".slice(1)); // "ort anything"
    expect(t).toContain("## For Claude");
    expect(t).toContain("so I can correct it");
  });

  test("prepare_day and finish_day guide a whole day", async () => {
    const id = writeDay();
    createProject("Website", ["acme-site"]);
    createProject("Client report", ["docs.google.com/document/d/1abc"]);
    let prep = (await call("prepare_day", { date: day })).text;
    expect(prep).toContain("## Checklist");
    expect(prep).toContain('Sort the 30m and 1 note that are "Not sorted yet"');
    expect(prep).toContain("- Website (id website) · 1h 00m");
    expect(prep).toContain("daily capsule: not yet");
    expect(prep).toContain("project capsule: none yet, create it");
    expect(prep).toContain("## The day");
    expect(prep).toContain(id);

    let fin = (await call("finish_day", { date: day })).text;
    expect(fin).toContain("Not done yet");
    expect(fin).toContain('daily capsule for Website: save_capsule with project "website", kind "daily"');
    expect(fin).toContain('still "Not sorted yet"');

    // sort the chat and the loose voice note, write every capsule
    createProject("Team chat", ["slack"]);
    expect((await call("prepare_day", { date: day })).text).toContain('Sort the 1 note that is "Not sorted yet"');
    expect((await call("assign_project", { session_id: id, project: "client-report", from: at(90), to: at(91) })).isError).toBe(false);
    for (const p of ["website", "client-report", "team-chat"]) {
      expect((await call("save_capsule", { title: `${p} — ${day}`, markdown: VALID_BODY, project: p, kind: "daily", date: day })).isError).toBe(false);
      expect((await call("save_capsule", { title: p, markdown: VALID_BODY, project: p, kind: "project" })).isError).toBe(false);
    }
    prep = (await call("prepare_day", { date: day })).text;
    expect(prep).toContain("Nothing to sort");
    expect(prep).toContain("## Current project capsules (merge today into these)");
    fin = (await call("finish_day", { date: day })).text;
    expect(fin).toContain(`Done for ${day} ✓`);
    expect(fin).toContain("| Website | 1h 00m | website-" + day + " | website |");
  });

  test("save_capsule refuses an incomplete capsule with what to fix", async () => {
    const r = await call("save_capsule", { title: "x", markdown: "## In short\nHi." });
    expect(r.isError).toBe(true);
    expect(r.text).toContain("Not saved yet");
    expect(r.text).toContain('Add the section "## For Claude"');
  });

  test("continue_from_capsule: by name, or import a file first", async () => {
    const byName = await client.getPrompt({ name: "continue_from_capsule", arguments: { capsule: "website" } });
    const t1 = (byName.messages[0]!.content as { text: string }).text;
    expect(t1).toContain('Call get_capsule with name "website"');
    expect(t1).toContain('Read its "For Claude" section first');
    expect(t1).toContain("at most three short lines");
    const byFile = await client.getPrompt({ name: "continue_from_capsule", arguments: { capsule: "~/Downloads/plan.capsule.md" } });
    expect((byFile.messages[0]!.content as { text: string }).text).toContain('Call import_capsule with path "~/Downloads/plan.capsule.md"');
  });
});

describe("finish_day reports what's unsure or skipped", () => {
  test("guesses and drafts are Unsure; tiny projects are Skipped", async () => {
    writeSession(home, "20261019-100000", [
      ev(0, "Code", "x — acme-site", "work", 60),
      ev(60, "Figma", "logo — acme-brand", "peek", 1),
    ]);
    createProject("Website", ["acme-site"]);
    createProject("Brand", ["acme-brand"]);
    const guessy = VALID_BODY.replace("*(seen in Linear, 10:08)*", "*(guess)*");
    expect((await call("save_capsule", { title: "w", markdown: guessy, project: "website", kind: "daily", date: day })).isError).toBe(false);
    expect((await call("save_capsule", { title: "Website", markdown: VALID_BODY, project: "website", kind: "project" })).isError).toBe(false);
    const fin = (await call("finish_day", { date: day })).text;
    expect(fin).toContain(`Done for ${day} ✓`);
    expect(fin).toContain("Unsure:");
    expect(fin).toContain("1 guess(es)");
    expect(fin).toContain("Skipped:");
    expect(fin).toContain("Brand (1m, too little to write up)");
  });

  test("the review prompt checks facts, reasons and privacy, and never exports", async () => {
    const p = await client.getPrompt({ name: "review_capsule", arguments: { name: "website" } });
    const t = (p.messages[0]!.content as { text: string }).text;
    expect(t).toContain("Confirmed problems");
    expect(t).toContain("Nothing private");
    expect(t).toContain("Don't export it");
  });
});
