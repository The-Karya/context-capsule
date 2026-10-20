// context-capsule — mcp: checks a capsule before it is saved, so every capsule another Claude loads is
// complete. Problems come back as precise "fix this" lines; Claude fixes them and saves again.
import { REQUIRED_SECTIONS } from "./capsules.js";

export interface CheckResult {
  /** Must be fixed before saving. */
  errors: string[];
  /** Saved anyway, but worth fixing. */
  warnings: string[];
}

/** "## Heading" → its body, keyed by the lower-case heading (first one wins). */
export function sectionsOf(markdown: string): Map<string, string> {
  const out = new Map<string, string>();
  const parts = markdown.split(/^##\s+/m).slice(1);
  for (const part of parts) {
    const nl = part.indexOf("\n");
    const heading = (nl === -1 ? part : part.slice(0, nl)).replace(/\s*#*\s*$/, "").replace(/[:.]$/, "").trim().toLowerCase();
    const body = nl === -1 ? "" : part.slice(nl + 1).trim();
    if (!out.has(heading)) out.set(heading, body);
  }
  return out;
}

/** Top-level list items of a section ("- x", "* x", "1. x"), joined with their wrapped lines. */
export function listItems(body: string): string[] {
  const items: string[] = [];
  for (const line of body.split("\n")) {
    if (/^\s{0,1}([-*+]|\d+[.)])\s+/.test(line)) items.push(line.replace(/^\s*([-*+]|\d+[.)])\s+/, "").trim());
    else if (items.length && line.trim() && /^\s{2,}/.test(line)) items[items.length - 1] += " " + line.trim();
  }
  return items;
}

const NONE = /^(none|nothing|n\/a|no (decisions|open questions|next steps)|—|-)\.?$/i;
const CITED = /\((?:[^()]*\b(?:you said|you typed|seen in|guess)\b[^()]*)\)/i;
const TIME = /\b\d{1,2}:\d{2}\b/;

const short = (s: string) => (s.length > 70 ? `${s.slice(0, 67)}…` : s);

export function checkCapsule(markdown: string): CheckResult {
  const errors: string[] = [];
  const warnings: string[] = [];
  const sections = sectionsOf(markdown);
  const get = (name: string) => sections.get(name.toLowerCase());

  for (const name of REQUIRED_SECTIONS) {
    if (get(name) === undefined) errors.push(`Add the section "## ${name}".`);
  }

  const inShort = get("In short");
  if (inShort !== undefined) {
    if (!inShort) errors.push(`"## In short" is empty: write one or two plain sentences for a person.`);
    else if (inShort.length > 400) warnings.push(`"## In short" is long; keep it to one or two sentences.`);
  }

  const forClaude = get("For Claude");
  if (forClaude !== undefined && forClaude.length < 120)
    errors.push(
      `"## For Claude" is too short. Speak to the Claude that will load this: whose work it is, what it is, ` +
        `that the decisions are settled, and to carry on from Next steps.`,
    );

  const decisions = get("Decisions");
  if (decisions !== undefined) {
    const items = listItems(decisions);
    if (!items.length && !NONE.test(decisions.trim()))
      errors.push(`"## Decisions": list each decision as "- **Decision** — why. *(you said, HH:MM)*", or write "None".`);
    const uncited = items.filter((i) => !NONE.test(i) && !CITED.test(i));
    if (uncited.length)
      errors.push(
        `"## Decisions": say where each decision came from — *(you said, HH:MM)*, *(you typed, HH:MM)*, ` +
          `*(seen in <App>, HH:MM)* or *(guess)*. Missing on: ${uncited.slice(0, 5).map((u) => `"${short(u)}"`).join("; ")}` +
          (uncited.length > 5 ? ` and ${uncited.length - 5} more.` : "."),
      );
  }

  const next = get("Next steps");
  if (next !== undefined && !next) errors.push(`"## Next steps" is empty: list what comes next, or write "None".`);

  const done = get("What was done");
  if (done !== undefined) {
    const items = listItems(done);
    if (!items.length && !NONE.test(done.trim())) warnings.push(`"## What was done": list what happened, in time order.`);
    const untimed = items.filter((i) => !TIME.test(i)).length;
    if (untimed) warnings.push(`"## What was done": ${untimed} item(s) have no time (HH:MM).`);
  }

  const words = markdown.split(/\s+/).filter(Boolean).length;
  if (words > 3000) warnings.push(`The capsule is ${words} words; aim for 2500 or fewer.`);
  return { errors, warnings };
}
