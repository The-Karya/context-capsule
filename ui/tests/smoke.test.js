// context-capsule — ui: the seeded data (capture-rs/ci/windows-smoke-seed.py, seeded here into a throwaway folder for
// one clock: 2026-10-23 19:00 in India) gives what the window shows for that day. The same clock goes to the seed and
// to the checks, so it passes on any day. Skipped when there's no Python.

import { afterAll, beforeAll, expect, test } from "bun:test";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import * as T from "../js/core/timeline.js";
import * as P from "../js/core/projects.js";
import * as S from "../js/core/stats.js";
import { makeDates, parseKey } from "../js/core/dates.js";

const DAY = "2026-10-23";
const CLOCK = `${DAY}T19:00:00+05:30`;
const SEED = join(import.meta.dir, "..", "..", "capture-rs", "ci", "windows-smoke-seed.py");
const python = Bun.which("python3") ?? Bun.which("python");
let ROOT;

let D;
beforeAll(() => {
  process.env.TZ = "Asia/Kolkata";
  D = makeDates({ locale: "en-US" });
  if (!python) return;
  ROOT = mkdtempSync(join(tmpdir(), "cc-smoke-"));
  const r = spawnSync(python, [SEED, ROOT, CLOCK], { encoding: "utf8" });
  if (r.status !== 0) throw new Error(`the seed failed: ${r.stderr}`);
  expect(r.stdout.split("\n")[0]).toBe(DAY);
});
afterAll(() => ROOT && rmSync(ROOT, { recursive: true, force: true }));

function load() {
  const read = (p) => (existsSync(p) ? readFileSync(p, "utf8") : null);
  const files = {};
  for (const id of readdirSync(join(ROOT, "sessions"))) {
    const dir = join(ROOT, "sessions", id);
    files[id] = { events: read(join(dir, "events.jsonl")), gaps: read(join(dir, "gaps.jsonl")), assign: read(join(dir, "assign.json")), session: read(join(dir, "session.json")) };
  }
  return { files, projects: P.parse(read(join(ROOT, "projects.json"))) };
}

test.skipIf(!python)("the seeded day looks as on the Mac", () => {
  const { files, projects } = load();
  const day = T.loadDay(DAY, files, { projects });
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
  expect(plain(T.blockLine(b, D.span(b.start, b.end), n.keys.map((k) => P.nameOf(projects, k))))).toBe("12:55 PM – 2:19 PM · 1 h 24 min · Website");
  const range = T.barRange(T.stretchesOf(day));
  expect(T.barTicks(range).map((t) => plain(D.hour(t.t)))).toEqual(["12 PM", "1 PM", "2 PM", "3 PM", "4 PM", "5 PM", "6 PM", "7 PM"]);
  // Home: "Today 3 h 32 min".
  const from = parseKey(DAY);
  expect(S.duration(S.totals(files, from, from + 86_400_000, projects).seconds)).toBe("3 h 32 min");
});
