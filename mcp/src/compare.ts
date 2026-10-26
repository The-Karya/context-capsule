// context-capsule — mcp: two versions of one capsule (the one you sent and the one that came back): what changed,
// section by section, and one merged capsule from both. Plain rules, no AI: lines are compared as items (a bullet,
// a numbered step, a paragraph), a decision is known by its bold name.
import { parseCapsule, type FrontMatter } from "./capsules.js";
import { blocks } from "./markdown.js";

/** One thing in a section: a list item (with its list kind), a paragraph, a quote, a code block or a sub-heading. */
export interface Item {
  text: string;
  kind: "ul" | "ol" | "p" | "quote" | "code" | "h";
}

export interface Section {
  name: string;
  /** The section's text as written (without its heading). */
  raw: string;
  items: Item[];
}

/** The `## ` sections of a body, in order. Text before the first one is left out (capsules start with a section). */
export function capsuleSections(body: string): Section[] {
  const out: Section[] = [];
  let cur: { name: string; lines: string[] } | null = null;
  let fence = "";
  const close = () => {
    if (!cur) return;
    const raw = cur.lines.join("\n").replace(/^\n+|\s+$/g, "");
    out.push({ name: cur.name, raw, items: itemsOf(raw) });
  };
  for (const line of body.replace(/\r\n?/g, "\n").split("\n")) {
    const t = line.trim();
    if (fence) {
      if (t.startsWith(fence)) fence = "";
    } else if (t.startsWith("```") || t.startsWith("~~~")) fence = t.slice(0, 3);
    const head = !fence && /^##\s+(.+?)\s*#*\s*$/.exec(line);
    if (head) {
      close();
      cur = { name: head[1]!.replace(/[:.]$/, ""), lines: [] };
    } else cur?.lines.push(line);
  }
  close();
  return out;
}

function itemsOf(raw: string): Item[] {
  return blocks(raw).flatMap((b): Item[] => {
    if ("items" in b) return b.items.map((text) => ({ text, kind: b.type }));
    if (b.type === "h") return [{ text: b.text, kind: "h" }];
    return [{ text: b.text, kind: b.type }];
  });
}

/** What two items are compared by: no Markdown marks, case or extra spaces. */
export function norm(text: string): string {
  return text
    .replace(/[*_`]/g, "")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/[.;,]+$/, "")
    .toLowerCase();
}

/** A decision's name: its **bold** start ("**Teams at $12/seat** — why…"). */
export function itemKey(text: string): string | null {
  const m = /^\s*(\*\*|__)(.+?)\1/.exec(text);
  return m ? norm(m[2]!) : null;
}

const words = (s: string) => new Set(norm(s).split(/[^\p{L}\p{N}$%]+/u).filter((w) => w.length > 1));

/** Share of words in common (0–1). */
export function similarity(a: string, b: string): number {
  const x = words(a);
  const y = words(b);
  if (!x.size || !y.size) return 0;
  let both = 0;
  for (const w of x) if (y.has(w)) both++;
  return both / (x.size + y.size - both);
}

/** Items this close are the same item, reworded. */
export const SAME_ITEM = 0.5;

export interface SectionChange {
  name: string;
  /** Only in the newer version / only in the older one. */
  added: Item[];
  removed: Item[];
  /** The same item, reworded (matched by a decision's bold name, or mostly the same words). */
  changed: { from: Item; to: Item }[];
  /** The section is only in one of them. */
  only?: "older" | "newer";
}

/** older → newer for one section's items. */
export function diffItems(older: Item[], newer: Item[]): Pick<SectionChange, "added" | "removed" | "changed"> {
  const restOld = [...older];
  const restNew: Item[] = [];
  for (const n of newer) {
    const i = restOld.findIndex((o) => norm(o.text) === norm(n.text));
    if (i >= 0) restOld.splice(i, 1);
    else restNew.push(n);
  }
  const changed: { from: Item; to: Item }[] = [];
  // the same decision by name first, then the closest wording
  for (const n of [...restNew]) {
    const k = itemKey(n.text);
    const i = k ? restOld.findIndex((o) => itemKey(o.text) === k) : -1;
    if (i < 0) continue;
    changed.push({ from: restOld[i]!, to: n });
    restOld.splice(i, 1);
    restNew.splice(restNew.indexOf(n), 1);
  }
  for (const n of [...restNew]) {
    let best = -1;
    let score = SAME_ITEM;
    restOld.forEach((o, i) => {
      const k1 = itemKey(o.text);
      const k2 = itemKey(n.text);
      // two decisions: alike by their names ("Teams at $12/seat" / "Teams at $14/seat"), not by their reasons
      const s = k1 && k2 ? similarity(k1, k2) : similarity(o.text, n.text);
      if (s >= score) {
        score = s;
        best = i;
      }
    });
    if (best < 0) continue;
    changed.push({ from: restOld[best]!, to: n });
    restOld.splice(best, 1);
    restNew.splice(restNew.indexOf(n), 1);
  }
  return { added: restNew, removed: restOld, changed };
}

/** Every section's changes, older → newer, in the newer version's order (sections only in the older one last). */
export function diffSections(olderBody: string, newerBody: string): SectionChange[] {
  const older = capsuleSections(olderBody);
  const newer = capsuleSections(newerBody);
  const find = (list: Section[], name: string) => list.find((s) => s.name.toLowerCase() === name.toLowerCase());
  const out: SectionChange[] = [];
  for (const n of newer) {
    const o = find(older, n.name);
    out.push(o ? { name: n.name, ...diffItems(o.items, n.items) } : { name: n.name, added: n.items, removed: [], changed: [], only: "newer" });
  }
  for (const o of older)
    if (!find(newer, o.name)) out.push({ name: o.name, added: [], removed: o.items, changed: [], only: "older" });
  return out;
}

export const hasChanges = (c: SectionChange) => !!(c.added.length || c.removed.length || c.changed.length || c.only);

// ---------- the same capsule? which is newer? ----------

const str = (v: unknown) => (typeof v === "string" ? v.trim() : "");

/** Why two capsules are versions of the same one, or null: the same project, kind and (daily) date; or, without a
 * project, the same title. */
export function sameCapsule(a: FrontMatter, b: FrontMatter): string | null {
  const pa = str(a.project);
  const pb = str(b.project);
  if (pa || pb) {
    if (pa !== pb) return null;
    const ka = str(a.kind) || "daily";
    if (ka !== (str(b.kind) || "daily")) return null;
    if (ka === "project") return `the same project (${pa}), whole-project capsule`;
    return str(a.date) === str(b.date) ? `the same project (${pa}) and day (${str(a.date) || "no date"})` : null;
  }
  const ta = str(a.title).toLowerCase();
  return ta && ta === str(b.title).toLowerCase() ? `the same title` : null;
}

/** Which one is newer, by `created`: 1 = b, -1 = a, 0 = can't tell. */
export function newerOf(a: FrontMatter, b: FrontMatter): -1 | 0 | 1 {
  const ta = Date.parse(str(a.created));
  const tb = Date.parse(str(b.created));
  if (Number.isNaN(ta) || Number.isNaN(tb) || ta === tb) return 0;
  return tb > ta ? 1 : -1;
}

/** Split a capsule file into its front matter and body (front matter may be missing). */
export function splitCapsule(text: string): { front: FrontMatter; body: string } {
  const { front, body } = parseCapsule(text);
  return { front: front ?? {}, body };
}
