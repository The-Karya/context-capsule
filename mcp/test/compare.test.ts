// context-capsule — mcp: tests for comparing two versions of a capsule (sections, changes, same capsule, newer)
import { describe, expect, test } from "bun:test";
import { missingSections } from "../src/capsules.ts";
import { checkCapsule } from "../src/checks.ts";
import { capsuleSections, diffSections, hasChanges, itemKey, mergeBodies, newerOf, sameCapsule, similarity } from "../src/compare.ts";
import { VALID_BODY } from "./helpers.ts";

/** The version that came back: Alex moved things on. */
export const NEWER_BODY = VALID_BODY.replace("Figma design done (Pricing v2), not shipped yet.", "Page shipped on Monday; watching sign-ups.")
  .replace("- **Teams at $12/seat** — matches competitors. *(seen in Linear, 10:08)*", "- **Teams at $14/seat** — finance asked for more margin. *(you said, 09:12)*")
  .replace("- **Keep the yearly discount at 20%** — most trials pick yearly. *(you said, 10:14)*", "- **Keep the yearly discount at 20%** — most trials pick yearly. *(you said, 10:14)*\n- **Drop the Starter tier** — almost nobody used it. *(you said, 09:20)*")
  .replace("1. Ship the page", "1. Ship the page to everyone\n2. Email the beta list");

describe("sections", () => {
  test("every ## section with its items; a ## inside code isn't a section", () => {
    const s = capsuleSections("## In short\nHello there.\n## Next steps\n1. a\n2. b\n```\n## not a heading\n```\n## Notes:\n- x\n");
    expect(s.map((x) => x.name)).toEqual(["In short", "Next steps", "Notes"]);
    expect(s[1]!.items).toEqual([
      { text: "a", kind: "ol" },
      { text: "b", kind: "ol" },
      { text: "## not a heading", kind: "code" },
    ]);
    expect(s[0]!.raw).toBe("Hello there.");
  });

  test("a decision's name and how alike two lines are", () => {
    expect(itemKey("**Teams at $12/seat** — why")).toBe("teams at $12/seat");
    expect(itemKey("plain line")).toBeNull();
    expect(similarity("Ship the page", "Ship the page to everyone")).toBeGreaterThanOrEqual(0.5);
    expect(similarity("Ship the page", "Email the beta list")).toBeLessThan(0.5);
  });
});

describe("what changed", () => {
  test("added, removed and reworded items, per section", () => {
    const d = diffSections(VALID_BODY, NEWER_BODY);
    const by = (n: string) => d.find((c) => c.name === n)!;
    expect(hasChanges(by("In short"))).toBe(false);
    expect(by("Where things stand").changed.length + by("Where things stand").added.length).toBeGreaterThan(0);
    const dec = by("Decisions");
    expect(dec.added.map((i) => i.text)).toEqual(["**Drop the Starter tier** — almost nobody used it. *(you said, 09:20)*"]);
    expect(dec.removed).toEqual([]);
    // "Teams at $12/seat" → "Teams at $14/seat": the same decision, changed
    expect(dec.changed.map((c) => [c.from.text.slice(0, 19), c.to.text.slice(0, 19)])).toEqual([["**Teams at $12/seat", "**Teams at $14/seat"]]);
    const next = by("Next steps");
    expect(next.changed.map((c) => c.to.text)).toEqual(["Ship the page to everyone"]);
    expect(next.added.map((i) => i.text)).toEqual(["Email the beta list"]);
  });

  test("two different decisions that share a reason stay two", () => {
    const a = "## Decisions\n- **Monthly plan** — most trials pick yearly anyway, says sales.\n";
    const b = "## Decisions\n- **Yearly discount** — most trials pick yearly anyway, says sales.\n";
    const [c] = diffSections(a, b);
    expect(c!.changed).toHaveLength(0);
    expect(c!.added).toHaveLength(1);
    expect(c!.removed).toHaveLength(1);
  });

  test("the same decision with a new reason is a change, not two decisions", () => {
    const a = "## Decisions\n- **Teams price** — $12 per seat. *(seen in Linear, 10:08)*\n";
    const b = "## Decisions\n- **Teams price** — $14 per seat. *(you said, 09:12)*\n";
    const [c] = diffSections(a, b);
    expect(c!.changed).toHaveLength(1);
    expect(c!.changed[0]!.from.text).toContain("$12");
    expect(c!.changed[0]!.to.text).toContain("$14");
  });

  test("a section only in one of them", () => {
    const d = diffSections("## In short\nx\n## Old notes\n- y\n", "## In short\nx\n## Risks\n- z\n");
    expect(d.find((c) => c.name === "Risks")!.only).toBe("newer");
    expect(d.find((c) => c.name === "Old notes")).toMatchObject({ only: "older", removed: [{ text: "y" }] });
  });
});

describe("merge", () => {
  test("newer present tense, every decision and step from both, the changed decision flagged", () => {
    // the older one had a step and a decision the newer one dropped
    const older = VALID_BODY.replace("1. Ship the page", "1. Ship the page\n2. Ask finance about Starter").replace(
      "## Open questions",
      "- **Logo stays black** — brand guide. *(seen in Figma, 10:05)*\n## Open questions",
    );
    const m = mergeBodies(older, NEWER_BODY);
    expect(m.body).toContain("## Where things stand\nPage shipped on Monday; watching sign-ups.\n");
    expect(m.body).not.toContain("not shipped yet");
    expect(m.body).toContain("**Teams at $14/seat**");
    expect(m.body).not.toContain("**Teams at $12/seat**");
    expect(m.body).toContain("- **Drop the Starter tier**");
    expect(m.body).toContain("- **Logo stays black**");
    expect(m.body).toContain("1. Ship the page to everyone\n2. Email the beta list\n3. Ask finance about Starter");
    expect(m.kept).toBe(2);
    expect(m.conflicts).toEqual([
      {
        section: "Decisions",
        older: "**Teams at $12/seat** — matches competitors. *(seen in Linear, 10:08)*",
        newer: "**Teams at $14/seat** — finance asked for more margin. *(you said, 09:12)*",
      },
    ]);
    // still a whole capsule, in order
    expect(missingSections(m.body)).toEqual([]);
    expect(checkCapsule(m.body).errors).toEqual([]);
  });

  test("a next step the newer version did isn't brought back", () => {
    const older = "## What was done\n- Wrote the copy\n## Next steps\n1. Email the beta list\n2. Ship the page\n";
    const newer = "## What was done\n- Wrote the copy\n- Shipped the page on Monday\n## Next steps\n1. Email the beta list\n";
    const m = mergeBodies(older, newer);
    expect(m.done).toEqual(["Ship the page"]);
    expect(m.body).not.toContain("2. Ship the page");
    expect(m.kept).toBe(0);
  });

  test("a section only the older one had is kept; a paragraph is added as a paragraph", () => {
    const m = mergeBodies("## In short\nold\n## Risks\n- churn\n## Open questions\nWho signs off?\n", "## In short\nnew\n## Open questions\n- Which date?\n");
    expect(m.body).toBe("## In short\nnew\n\n## Open questions\n- Which date?\n\nWho signs off?\n\n## Risks\n- churn\n");
  });
});

describe("versions of one capsule", () => {
  test("matched by project, kind and day; without a project, by title", () => {
    const daily = { project: "website", kind: "daily", date: "2026-10-19" };
    expect(sameCapsule(daily, { ...daily, title: "x" })).toContain("same project (website) and day");
    expect(sameCapsule(daily, { ...daily, date: "2026-10-20" })).toBeNull();
    expect(sameCapsule({ project: "website", kind: "project" }, { project: "website", kind: "project" })).toContain("whole-project");
    expect(sameCapsule({ project: "website", kind: "project" }, daily)).toBeNull();
    expect(sameCapsule({ project: "website", kind: "project" }, { project: "shop", kind: "project" })).toBeNull();
    expect(sameCapsule({ title: "Pricing page" }, { title: "pricing page" })).toBe("the same title");
    expect(sameCapsule({ title: "Pricing page" }, { title: "Pricing page", project: "website" })).toBeNull();
  });

  test("newer by created", () => {
    expect(newerOf({ created: "2026-10-19T10:00:00Z" }, { created: "2026-10-20T10:00:00Z" })).toBe(1);
    expect(newerOf({ created: "2026-10-21T10:00:00Z" }, { created: "2026-10-20T10:00:00Z" })).toBe(-1);
    expect(newerOf({}, { created: "2026-10-20T10:00:00Z" })).toBe(0);
  });
});
