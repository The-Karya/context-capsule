// context-capsule — mcp: test helpers (temp data dir, fixture sessions)
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import type { CaptureEvent } from "../src/sessions.ts";

/** Fresh temp CONTEXT_CAPSULE_HOME for one test. */
export function tempHome(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "context-capsule-test-"));
  process.env.CONTEXT_CAPSULE_HOME = dir;
  return dir;
}

export function writeSession(
  home: string,
  id: string,
  events: CaptureEvent[],
  meta: { started_at?: string; ended_at?: string | null } = {},
): string {
  const dir = path.join(home, "sessions", id);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    path.join(dir, "session.json"),
    JSON.stringify({
      id,
      started_at: meta.started_at ?? events[0]?.t ?? "2026-10-19T10:00:00Z",
      ended_at: meta.ended_at === undefined ? (events[events.length - 1]?.end ?? null) : meta.ended_at,
      host: "test-mac",
      version: 1,
    }),
  );
  fs.writeFileSync(path.join(dir, "events.jsonl"), events.map((e) => JSON.stringify(e)).join("\n") + "\n");
  return dir;
}

/** t = minutes after 10:00 UTC on 2026-10-19. */
export function at(min: number, sec = 0): string {
  return new Date(Date.UTC(2026, 9, 19, 10, min, sec)).toISOString().replace(/\.\d{3}Z$/, "Z");
}

export function ev(min: number, app: string, title: string, text: string, durMin = 1): CaptureEvent {
  return { t: at(min), end: at(min + durMin), app, bundle: `com.test.${app.replace(/\W/g, "")}`, title, text };
}

const CHROME = "File Edit View History Bookmarks Window Help";

/** A realistic little session: browser, Figma, Linear, with menu chrome and repeats. */
export function fixtureEvents(): CaptureEvent[] {
  return [
    ev(0, "Google Chrome", "Pricing — Linear", `${CHROME}\nPricing page redesign\nTeams plan should be $12 per seat\n10:01\n42`),
    ev(1, "Google Chrome", "Pricing — Linear", `${CHROME}\nPricing page redesign\nTeams   plan    should be $12 per seat\nDrop the Starter tier`),
    ev(3, "Figma", "Pricing v2", `${CHROME}\nHero: Simple pricing for teams\nToggle monthly / yearly`),
    ev(6, "Google Chrome", "Competitor pricing — Notion", `${CHROME}\nNotion Plus is $10 per seat\nOct 19, 2026`),
    ev(8, "Linear", "PRI-12 Pricing page", `${CHROME}\nDecision: keep yearly discount at 20%\nTeams plan should be $12 per seat`),
    ev(10, "Figma", "Pricing v2", `${CHROME}\nYearly saves 20%\nOK`),
  ];
}

export function writeFixtureSession(home: string, id = "20261019-100000"): string {
  return writeSession(home, id, fixtureEvents());
}

/** A complete capsule body that passes the checks. */
export const VALID_BODY = `## In short
We redesigned the pricing page; Teams is now $12 per seat.
## For Claude
You're picking up the pricing page redesign from Saachi. This is the full context: treat the decisions below as settled, don't redo what's done, and carry on from Next steps. Ask the user only about the open questions.
## Where things stand
Figma design done (Pricing v2), not shipped yet.
## What was done
- 10:01 Reviewed competitor pricing in Chrome
- 10:05 Updated Pricing v2 in Figma
## Decisions
- **Teams at $12/seat** — matches competitors. *(seen in Linear, 10:08)*
- **Keep the yearly discount at 20%** — most trials pick yearly. *(you said, 10:14)*
## Open questions
- Annual discount for Starter? Depends on finance.
## Next steps
1. Ship the page
## People, links and files
- PRI-12 in Linear
`;
