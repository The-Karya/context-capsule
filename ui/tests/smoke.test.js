// context-capsule — ui: the real seeded data (/tmp/mac-ref-data, from capture-rs/ci/windows-smoke-seed.py) gives what
// the Mac shows for 2026-10-23 (/tmp/mac-ref/timeline-light.png). Skipped when the data isn't there.

import { beforeAll, expect, test } from "bun:test";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import * as T from "../js/core/timeline.js";
import * as P from "../js/core/projects.js";
import * as S from "../js/core/stats.js";
import { makeDates, parseKey } from "../js/core/dates.js";

const ROOT = "/tmp/mac-ref-data";
const has = existsSync(join(ROOT, "sessions"));

// The Mac that rendered it was in India (06:59:57Z shows as 12:29 PM).
let D;
beforeAll(() => {
  process.env.TZ = "Asia/Kolkata";
  D = makeDates({ locale: "en-US" });
});

function load() {
  const read = (p) => (existsSync(p) ? readFileSync(p, "utf8") : null);
  const files = {};
  for (const id of readdirSync(join(ROOT, "sessions"))) {
    const dir = join(ROOT, "sessions", id);
    files[id] = { events: read(join(dir, "events.jsonl")), gaps: read(join(dir, "gaps.jsonl")), assign: read(join(dir, "assign.json")), session: read(join(dir, "session.json")) };
  }
  return { files, projects: P.parse(read(join(ROOT, "projects.json"))) };
}

test.skipIf(!has)("the seeded day looks as on the Mac", () => {
  const { files, projects } = load();
  const day = T.loadDay("2026-10-23", files, { projects });
  expect(day.blocks.length).toBe(3);
  expect(T.summary(day)).toBe("3 h 32 min recorded · 3 recordings");
  const sh = T.shares(T.stretchesOf(day));
  expect(sh.rows.map((r) => [r.key ? P.nameOf(projects, r.key) : "Not sorted", T.length(r.seconds)])).toEqual([
    ["Context Capsule", "1 h 20 min"],
    ["Website", "1 h 5 min"],
    ["Client report", "55 min"],
    ["Not sorted", "12 min"],
  ]);
  const plain = (s) => s.replace(/[  ]/g, " ");
  const b = day.blocks[0];
  const n = T.names(b);
  expect(plain(T.blockLine(b, D.span(b.start, b.end), n.keys.map((k) => P.nameOf(projects, k))))).toBe("12:29 PM – 1:53 PM · 1 h 24 min · Website");
  const range = T.barRange(T.stretchesOf(day));
  expect(T.barTicks(range).map((t) => plain(D.hour(t.t)))).toEqual(["12 PM", "1 PM", "2 PM", "3 PM", "4 PM", "5 PM", "6 PM"]);
  // Home: "Today 3 h 32 min".
  const from = parseKey("2026-10-23");
  expect(S.duration(S.totals(files, from, from + 86_400_000, projects).seconds)).toBe("3 h 32 min");
});
