// context-capsule — mcp: tests for capsules (slug, save, front matter, collisions, import, export)
import { beforeEach, describe, expect, test } from "bun:test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  exportCapsule,
  findCapsule,
  importCapsule,
  listCapsules,
  missingSections,
  parseCapsule,
  saveCapsule,
  slugify,
  stringifyFrontMatter,
  validateCapsuleText,
} from "../src/capsules.ts";
import { tempHome, writeFixtureSession } from "./helpers.ts";

const BODY = `## Summary
We redesigned pricing.
## What I worked on
- Pricing page in Figma
## Decisions
- **Teams at $12/seat** — matches competitors. *(seen in Linear, 10:08)*
## Open questions
- Annual discount?
## Next steps
- Ship it
## Sources
- PRI-12 in Linear
`;

let home: string;
beforeEach(() => {
  home = tempHome();
});

describe("slugify", () => {
  test("makes clean slugs", () => {
    expect(slugify("Pricing page redesign")).toBe("pricing-page-redesign");
    expect(slugify("  Café: Q4 / launch!! ")).toBe("cafe-q4-launch");
    expect(slugify("???")).toBe("capsule");
    expect(slugify("a".repeat(100)).length).toBe(60);
  });
});

describe("front matter", () => {
  test("round-trips scalars and lists", () => {
    const fm = { capsule: 1, title: "Plan: v2, final", author: "Saachi Badal", sessions: ["20261019-100000"], apps: ["Figma", "Google Chrome"] };
    const { front, body } = parseCapsule(stringifyFrontMatter(fm) + "\nhello\n");
    expect(front).toEqual(fm);
    expect(body.trim()).toBe("hello");
  });

  test("reads the spec example (comments, unquoted)", () => {
    const text = `---\ncapsule: 1\ntitle: Pricing page redesign\nauthor: Saachi            # from config\ncreated: 2026-10-19T11:30:00Z\nsessions: [20261019-100103]\napps: [Figma, Google Chrome, Linear]\n---\n\n## Summary\n`;
    const v = validateCapsuleText(text);
    expect(v.ok).toBe(true);
    if (v.ok) {
      expect(v.front.author).toBe("Saachi");
      expect(v.front.apps).toEqual(["Figma", "Google Chrome", "Linear"]);
      expect(v.front.sessions).toEqual(["20261019-100103"]);
    }
  });
});

describe("save_capsule", () => {
  test("writes front matter with author from config, sessions and apps", () => {
    fs.writeFileSync(path.join(home, "config.json"), JSON.stringify({ author: "Test Author" }));
    writeFixtureSession(home, "20261019-100000");
    const r = saveCapsule({ title: "Pricing page redesign", markdown: BODY, session_ids: ["20261019-100000"] });
    expect(r.path).toBe(path.join(home, "capsules", "pricing-page-redesign.capsule.md"));
    expect(r.warnings).toEqual([]);
    const { front, body } = parseCapsule(fs.readFileSync(r.path, "utf8"));
    expect(front!.capsule).toBe(1);
    expect(front!.title).toBe("Pricing page redesign");
    expect(front!.author).toBe("Test Author");
    expect(String(front!.created)).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
    expect(front!.sessions).toEqual(["20261019-100000"]);
    expect(front!.apps).toEqual(["Figma", "Google Chrome", "Linear"]);
    expect(body).toContain("## Decisions");
    expect(fs.readFileSync(r.path, "utf8")).toMatch(/\ncreated: \d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z\n/);
  });

  test("author falls back to the OS user", () => {
    const r = saveCapsule({ title: "x", markdown: BODY });
    expect(String(r.front.author).length).toBeGreaterThan(0);
  });

  test("never overwrites: -2, -3", () => {
    const a = saveCapsule({ title: "Same", markdown: BODY });
    const b = saveCapsule({ title: "Same", markdown: BODY });
    const c = saveCapsule({ title: "Same", markdown: BODY });
    expect(path.basename(a.path)).toBe("same.capsule.md");
    expect(path.basename(b.path)).toBe("same-2.capsule.md");
    expect(path.basename(c.path)).toBe("same-3.capsule.md");
    expect(b.warnings.join(" ")).toContain("already existed");
  });

  test("warns (not fails) on missing sections and unknown sessions", () => {
    const r = saveCapsule({ title: "Short", markdown: "## Summary\nJust a note", session_ids: ["20990101-000000"] });
    expect(fs.existsSync(r.path)).toBe(true);
    expect(r.warnings.join("\n")).toContain("## Decisions");
    expect(r.warnings.join("\n")).toContain("20990101-000000");
    expect(missingSections(BODY)).toEqual([]);
  });

  test("replaces front matter the AI added and redacts secrets", () => {
    const md = `---\ncapsule: 9\ntitle: wrong\n---\n${BODY}\nkey: sk-abcdefghijklmnopqrstuvwxyz123456\npassword: hunter2\n`;
    const r = saveCapsule({ title: "Right", markdown: md });
    const text = fs.readFileSync(r.path, "utf8");
    expect(parseCapsule(text).front!.title).toBe("Right");
    expect(parseCapsule(text).front!.capsule).toBe(1);
    expect(text).not.toContain("sk-abcdef");
    expect(text).not.toContain("hunter2");
    expect(r.warnings.join(" ")).toContain("Redacted 2");
  });

  test("list and find by slug, file name or title", () => {
    saveCapsule({ title: "Pricing page redesign", markdown: BODY });
    const all = listCapsules();
    expect(all).toHaveLength(1);
    expect(findCapsule("pricing-page-redesign").title).toBe("Pricing page redesign");
    expect(findCapsule("pricing-page-redesign.capsule.md").slug).toBe("pricing-page-redesign");
    expect(findCapsule("pricing PAGE redesign").slug).toBe("pricing-page-redesign");
    expect(() => findCapsule("nope")).toThrow(/No capsule called "nope"/);
  });
});

describe("import_capsule", () => {
  const inbox = () => fs.mkdtempSync(path.join(os.tmpdir(), "context-capsule-inbox-"));

  test("imports a valid capsule, once", () => {
    const file = path.join(inbox(), "their-plan.capsule.md");
    fs.writeFileSync(file, `---\ncapsule: 1\ntitle: Their plan\nauthor: Friend\n---\n${BODY}`);
    const r = importCapsule(file);
    expect(r.already).toBe(false);
    expect(r.path).toBe(path.join(home, "capsules", "their-plan.capsule.md"));
    expect(importCapsule(file).already).toBe(true);
    expect(listCapsules()).toHaveLength(1);
  });

  test("a different capsule with the same name gets -2", () => {
    const dir = inbox();
    const file = path.join(dir, "plan.capsule.md");
    fs.writeFileSync(file, `---\ncapsule: 1\ntitle: Plan A\n---\nA`);
    importCapsule(file);
    fs.writeFileSync(file, `---\ncapsule: 1\ntitle: Plan B\n---\nB`);
    expect(path.basename(importCapsule(file).path)).toBe("plan-2.capsule.md");
  });

  test("rejects files that are not capsules", () => {
    const dir = inbox();
    const plain = path.join(dir, "notes.md");
    fs.writeFileSync(plain, "# just notes");
    expect(() => importCapsule(plain)).toThrow(/no capsule front matter/);
    const v2 = path.join(dir, "v2.capsule.md");
    fs.writeFileSync(v2, "---\ncapsule: 2\ntitle: Future\n---\n");
    expect(() => importCapsule(v2)).toThrow(/Unsupported capsule version/);
    const noTitle = path.join(dir, "nt.capsule.md");
    fs.writeFileSync(noTitle, "---\ncapsule: 1\n---\n");
    expect(() => importCapsule(noTitle)).toThrow(/no `title`/);
    expect(() => importCapsule(path.join(dir, "missing.capsule.md"))).toThrow(/No file at/);
  });
});

describe("export_capsule", () => {
  test("to a folder, never overwriting", () => {
    saveCapsule({ title: "Share me", markdown: BODY });
    const out = fs.mkdtempSync(path.join(os.tmpdir(), "context-capsule-out-"));
    const a = exportCapsule("share-me", out);
    const b = exportCapsule("share-me", out);
    expect(a).toBe(path.join(out, "share-me.capsule.md"));
    expect(b).toBe(path.join(out, "share-me-2.capsule.md"));
    expect(validateCapsuleText(fs.readFileSync(a, "utf8")).ok).toBe(true);
  });

  test("to a file path", () => {
    saveCapsule({ title: "Share me", markdown: BODY });
    const out = fs.mkdtempSync(path.join(os.tmpdir(), "context-capsule-out-"));
    const target = path.join(out, "sub", "for-alex.capsule.md");
    expect(exportCapsule("Share me", target)).toBe(target);
  });

  test("defaults to ~/Desktop", () => {
    saveCapsule({ title: "Share me", markdown: BODY });
    const fakeHome = fs.mkdtempSync(path.join(os.tmpdir(), "context-capsule-home-"));
    fs.mkdirSync(path.join(fakeHome, "Desktop"));
    const oldHome = process.env.HOME;
    process.env.HOME = fakeHome;
    try {
      expect(exportCapsule("share-me")).toBe(path.join(fakeHome, "Desktop", "share-me.capsule.md"));
    } finally {
      process.env.HOME = oldHome;
    }
  });
});
