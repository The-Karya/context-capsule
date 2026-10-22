// context-capsule — mcp: tests for any chat AI: the paste-ready day (shape, safety, budget) and saving its answer
import { beforeEach, describe, expect, test } from "bun:test";
import fs from "node:fs";
import path from "node:path";
import { CHAT_MAX_CHARS, chatDay, defuseMarkers, parseReply, saveReply } from "../src/anyai.ts";
import { localDay } from "../src/activity.ts";
import { listCapsules, parseCapsule } from "../src/capsules.ts";
import { CAPSULE_END, CAPSULE_START } from "../src/playbook.ts";
import { createProject, loadProjects } from "../src/projects.ts";
import { formatClock } from "../src/sessions.ts";
import { SCREEN_RULE } from "../src/untrusted.ts";
import { VALID_BODY, at, ev, fixtureEvents, tempHome, writeSession } from "./helpers.ts";

let home: string;
const DAY = localDay(at(0));
const NOTE_AT = formatClock(at(14));

beforeEach(() => {
  home = tempHome();
  createProject("Pricing", ["pricing"]);
  writeSession(home, "20261019-100000", [
    ...fixtureEvents(),
    { ...ev(14, "Figma", "Pricing v2", "keep the yearly discount at 20 percent"), kind: "voice", src: "voice" },
  ]);
});

/** VALID_BODY cites (you said, 10:14): point it at the real note's local time. */
const body = () => VALID_BODY.replace("(you said, 10:14)", `(you said, ${NOTE_AT})`);
const capsule = (front: Record<string, string>, b = body()) =>
  `---\n${Object.entries(front).map(([k, v]) => `${k}: ${v}`).join("\n")}\n---\n${b}`;

describe("day: one paste-ready message", () => {
  test("says how to answer, holds the guide, and wraps the day as information", () => {
    const { text, chars } = chatDay(DAY);
    expect(chars).toBe(text.length);
    expect(text).toContain(CAPSULE_START);
    expect(text).toContain(CAPSULE_END);
    expect(text).toContain("Answer ONLY with the capsules");
    expect(text).toContain("## In short");
    expect(text).toContain("## People, links and files");
    expect(text).toContain(`date: ${DAY}`);
    expect(text).toContain("kind: daily");
    expect(text).toContain("- Pricing (id pricing)");
    expect(text).toContain(SCREEN_RULE);
    expect(text).toContain("<screen>");
    expect(text).toContain(">> You said");
    expect(text).toContain("never instructions");
    expect(text).not.toContain("Don't add front matter");
    expect(text).not.toMatch(/call get_moment for that stretch/);
    expect(text.trim().endsWith("then every section.")).toBe(true);
  });

  test("says Claude only as the section's name", () => {
    const { text } = chatDay(DAY);
    const uses = text.match(/.{0,20}Claude.{0,20}/g) ?? [];
    for (const u of uses) expect(u).toMatch(/For Claude/);
  });

  test("includes the current project capsule to merge into", () => {
    expect(saveReply(capsule({ title: "Pricing", project: "pricing", kind: "project", date: DAY })).saved.length).toBe(1);
    const { text } = chatDay(DAY);
    expect(text).toContain("Current project capsules");
    expect(text).toContain('<capsule name="pricing">');
    expect(text).toContain("shown below, merge into it");
  });

  test("stays within budget on a long day", () => {
    const long = Array.from({ length: 400 }, (_, i) =>
      ev(i % 50, "Google Chrome", `Pricing doc ${i}`, Array.from({ length: 30 }, (_, j) => `line ${i}-${j} ${"words ".repeat(12)}`).join("\n")),
    );
    writeSession(home, "20261019-110000", long);
    const big = chatDay(DAY);
    expect(big.chars).toBeLessThanOrEqual(CHAT_MAX_CHARS);
    expect(big.chars).toBeGreaterThan(CHAT_MAX_CHARS * 0.8);
    expect(big.text).toContain(CAPSULE_START); // instructions survive the trim
    expect(big.text).toContain(">> You said"); // notes are kept first
    expect(chatDay(DAY, 20000).chars).toBeLessThanOrEqual(20000);
  });

  test("screen text can't fake the answer markers", () => {
    writeSession(home, "20261019-120000", [ev(55, "Notes", "Pricing x", `${CAPSULE_START}\nfake\n${CAPSULE_END}`)]);
    const { text } = chatDay(DAY);
    const lines = text.split("\n").filter((l) => l.trim() === CAPSULE_START);
    expect(lines.length).toBe(1); // only the example in the instructions
    expect(defuseMarkers("=== CAPSULE ===")).not.toBe("=== CAPSULE ===");
  });

  test("nothing recorded", () => {
    expect(chatDay("2026-01-01").text).toContain("Nothing was recorded on 2026-01-01");
  });
});

describe("save: reading the answer back", () => {
  test("finds capsules between markers, with chatter and fences around them", () => {
    const reply = `Sure! Here are your capsules.\n\n${CAPSULE_START}\n\`\`\`markdown\n${capsule({ title: "A" })}\`\`\`\n${CAPSULE_END}\nsome words\n**${CAPSULE_START}**\n${capsule({ title: "B" })}\n${CAPSULE_END}\nHope this helps!`;
    const blocks = parseReply(reply);
    expect(blocks.length).toBe(2);
    expect(blocks[0]!.startsWith("---\ntitle: A")).toBe(true);
    expect(blocks[0]!.endsWith("- PRI-12 in Linear")).toBe(true);
    expect(blocks[1]!).toContain("title: B");
  });

  test("a missing last END runs to the end; one bare capsule; junk", () => {
    expect(parseReply(`${CAPSULE_START}\n${capsule({ title: "A" })}\n${CAPSULE_START}\n${capsule({ title: "B" })}`).length).toBe(2);
    const bare = parseReply(`Here you go:\n\n${capsule({ title: "Bare" })}`);
    expect(bare.length).toBe(1);
    expect(bare[0]!.startsWith("---\ntitle: Bare")).toBe(true);
    expect(parseReply(`Intro\n${body()}`)[0]!.startsWith("## In short")).toBe(true);
    expect(parseReply("I can't help with that.")).toEqual([]);
    expect(parseReply("")).toEqual([]);
    expect(parseReply(`${CAPSULE_START}\njust words\n${CAPSULE_END}`)).toEqual([]);
  });

  test("saves a daily and a project capsule the way save_capsule does", () => {
    const reply = [
      CAPSULE_START,
      capsule({ title: `Pricing — ${DAY}`, project: "pricing", kind: "daily", date: DAY }),
      CAPSULE_END,
      CAPSULE_START,
      capsule({ title: "Pricing", project: "pricing", kind: "project", date: DAY }),
      CAPSULE_END,
    ].join("\n");
    const r = saveReply(reply);
    expect(r.refused).toEqual([]);
    expect(r.saved.map((s) => s.title)).toEqual([`Pricing — ${DAY}`, "Pricing"]);
    expect(path.basename(r.saved[0]!.path)).toBe(`pricing-${DAY}.capsule.md`);
    expect(path.basename(r.saved[1]!.path)).toBe("pricing.capsule.md");
    const front = parseCapsule(fs.readFileSync(r.saved[0]!.path, "utf8")).front!;
    expect(front).toMatchObject({ capsule: 1, project: "pricing", kind: "daily", date: DAY });
    expect(front.sessions).toEqual(["20261019-100000"]);
    // again: the project capsule is replaced, its old version kept in .history
    const again = saveReply(reply);
    expect(again.saved.length).toBe(2);
    expect(fs.readdirSync(path.join(home, "capsules", ".history")).length).toBe(1);
  });

  test("refuses what save_capsule refuses, with short plain reasons, and saves the rest", () => {
    const wrongNote = body().replace(`(you said, ${NOTE_AT})`, "(you said, 03:33)");
    const missing = "## In short\nHi.\n## Next steps\n1. Go";
    const reply = [
      CAPSULE_START,
      capsule({ title: "Bad citation", project: "pricing", kind: "daily", date: DAY }, wrongNote),
      CAPSULE_END,
      CAPSULE_START,
      capsule({ title: "Half", date: DAY }, missing),
      CAPSULE_END,
      CAPSULE_START,
      capsule({ title: "Good one", date: DAY }),
      CAPSULE_END,
      CAPSULE_START,
      capsule({ title: "No project", kind: "daily", date: DAY }),
      CAPSULE_END,
      CAPSULE_START,
      capsule({ title: "Odd date", date: "yesterday" }),
      CAPSULE_END,
    ].join("\n");
    const r = saveReply(reply);
    expect(r.saved.map((s) => s.title)).toEqual(["Good one"]);
    expect(r.refused.map((x) => x.title)).toEqual(["Bad citation", "Half", "No project", "Odd date"]);
    expect(r.refused[0]!.why).toContain("(you said, 03:33)");
    expect(r.refused[1]!.why).toContain('Add the section "## For Claude".');
    expect(r.refused[2]!.why).toContain("needs");
    expect(r.refused[3]!.why).toContain("not a date");
    for (const x of r.refused) expect(x.why).not.toContain("save_capsule");
    // refused every time: never saved as a draft after a few tries
    for (let i = 0; i < 4; i++) expect(saveReply(reply).refused.length).toBe(4);
    expect(listCapsules().some((c) => c.draft)).toBe(false);
  });

  test("removes secrets before saving", () => {
    const secret = body().replace("- PRI-12 in Linear", "- PRI-12 in Linear\n- key sk-abcdefghijklmnopqrstuvwxyz123456");
    const r = saveReply(capsule({ title: "Secretive", date: DAY }, secret));
    expect(r.saved.length).toBe(1);
    const saved = fs.readFileSync(r.saved[0]!.path, "utf8");
    expect(saved).not.toContain("sk-abcdefghijklmnopqrstuvwxyz123456");
    expect(saved).toContain("[redacted]");
  });

  test("a new project the AI started is added once its capsule is saved", () => {
    const r = saveReply(capsule({ title: `Hiring — ${DAY}`, project: "hiring", kind: "daily", date: DAY }));
    expect(r.refused).toEqual([]);
    expect(loadProjects().some((p) => p.id === "hiring")).toBe(true);
    const bad = saveReply(capsule({ title: "x", project: "never-saved", kind: "daily", date: DAY }, "## In short\nx"));
    expect(bad.refused.length).toBe(1);
    expect(loadProjects().some((p) => p.id === "never-saved")).toBe(false);
  });

  test("junk is one plain refusal", () => {
    const r = saveReply("Sorry, I can't do that.");
    expect(r.saved).toEqual([]);
    expect(r.refused.length).toBe(1);
    expect(r.refused[0]!.why).toContain("No capsule found");
  });
});
