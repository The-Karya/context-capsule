// context-capsule — mcp: one work day as a job for Claude — prepare_day hands over everything at once with a
// checklist; finish_day checks that every project got its capsules. Fewer choices, the same result every time.
import fs from "node:fs";
import { DEFAULT_MAX_CHARS, condenseDay, localDay, projectNames, trimTo } from "./activity.js";
import { listCapsules, type CapsuleInfo } from "./capsules.js";
import {
  type CaptureEvent,
  type Gap,
  formatDuration,
  isNote,
  listSessionIds,
  projectTimes,
  readEvents,
  readGaps,
} from "./sessions.js";

/** Below this, a project's time that day is noise (a glance at a window), not work to write up. */
export const MIN_PROJECT_MS = 2 * 60 * 1000;

export interface Day {
  day: string;
  sessionIds: string[];
  events: CaptureEvent[];
  /** Stretches that weren't read that day (gaps.jsonl of every session), in time order. */
  gaps: Gap[];
  /** Per project worked on (id "" = not sorted yet): time, sessions, notes. Most time first. */
  projects: { id: string; ms: number; sessions: string[]; notes: number }[];
}

/** "20261018" for day "2026-10-19": the session-id prefix of the day before. */
export function prevDayId(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const prev = new Date(y!, m! - 1, d! - 1);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${prev.getFullYear()}${pad(prev.getMonth() + 1)}${pad(prev.getDate())}`;
}

export function today(): string {
  return localDay(new Date().toISOString());
}

export function checkDay(day: string | undefined): string {
  const d = day?.trim() || today();
  if (!/^\d{4}-\d{2}-\d{2}$/.test(d)) throw new Error(`"${d}" is not a date like 2026-10-19.`);
  return d;
}

/** Every session's events on a local day (a session from the day before may run past midnight). */
export function loadDay(day: string): Day {
  const ids = listSessionIds()
    .filter((id) => id.startsWith(day.replace(/-/g, "")) || id.startsWith(prevDayId(day)))
    .reverse();
  const sessionIds: string[] = [];
  const events: CaptureEvent[] = [];
  const gaps: Gap[] = [];
  const sessionsOf = new Map<string, Set<string>>();
  const notesOf = new Map<string, number>();
  for (const id of ids) {
    const ev = readEvents(id).filter((e) => localDay(e.t) === day);
    gaps.push(...readGaps(id).filter((g) => localDay(g.t) === day || localDay(g.end) === day));
    if (!ev.length) continue;
    sessionIds.push(id);
    for (const e of ev) {
      const p = e.project ?? "";
      if (!sessionsOf.has(p)) sessionsOf.set(p, new Set());
      sessionsOf.get(p)!.add(id);
      if (isNote(e)) notesOf.set(p, (notesOf.get(p) ?? 0) + 1);
    }
    events.push(...ev);
  }
  const times = new Map(projectTimes(events).map((t) => [t.project, t.ms]));
  const ids2 = new Set([...times.keys(), ...notesOf.keys()]);
  const projects = [...ids2]
    .map((id) => ({ id, ms: times.get(id) ?? 0, sessions: [...(sessionsOf.get(id) ?? [])], notes: notesOf.get(id) ?? 0 }))
    .sort((a, b) => b.ms - a.ms);
  gaps.sort((a, b) => Date.parse(a.t) - Date.parse(b.t));
  return { day, sessionIds, events, gaps, projects };
}

/** What "Not sorted yet" holds, in words: "30m", "2 notes", "30m and 1 note" ("" if nothing worth sorting). */
export function looseLabel(d: Day): string {
  const loose = d.projects.find((p) => !p.id);
  if (!loose) return "";
  const parts = [
    ...(loose.ms >= MIN_PROJECT_MS ? [formatDuration(loose.ms)] : []),
    ...(loose.notes ? [`${loose.notes} note${loose.notes === 1 ? "" : "s"}`] : []),
  ];
  return parts.join(" and ");
}

/** Projects that need capsules that day: enough time, or a note. */
export function projectsToWrite(d: Day): Day["projects"] {
  return d.projects.filter((p) => p.id && (p.ms >= MIN_PROJECT_MS || p.notes > 0));
}

function capsulesFor(all: CapsuleInfo[], project: string, day: string) {
  return {
    daily: all.find((c) => c.project === project && c.kind !== "project" && c.date === day),
    whole: all.find((c) => c.project === project && c.kind === "project"),
  };
}

/** The growing capsule counts as updated if it was saved on or after the day itself. */
function updatedFor(whole: CapsuleInfo | undefined, day: string): boolean {
  return !!whole?.created && localDay(whole.created) >= day;
}

/** Everything Claude needs to write a day's capsules, in one go, with the steps to follow. */
export function prepareDay(day: string, maxChars = DEFAULT_MAX_CHARS): string {
  const d = loadDay(day);
  const names = projectNames();
  const name = (id: string) => names.get(id) ?? id;
  const all = listCapsules();
  if (!d.events.length) return `# ${day}\n\nNothing was recorded on ${day}. Nothing to do.`;

  const loose = looseLabel(d);
  const todo = projectsToWrite(d);
  const rows = todo.map((p) => {
    const c = capsulesFor(all, p.id, day);
    return (
      `- ${name(p.id)} (id ${p.id}) · ${formatDuration(p.ms)}${p.notes ? ` · ${p.notes} note(s)` : ""} · sessions ${p.sessions.join(", ")}` +
      ` · daily capsule: ${c.daily ? `saved (${c.daily.slug})` : "not yet"}` +
      ` · project capsule: ${c.whole ? (updatedFor(c.whole, day) ? `updated (${c.whole.slug})` : `exists, update it (${c.whole.slug})`) : "none yet, create it"}`
    );
  });
  const small = d.projects.filter((p) => p.id && !todo.includes(p));

  const steps = [
    loose
      ? `1. Sort the ${loose} that ${/ and |notes$/.test(loose) ? "are" : "is"} "Not sorted yet" yourself: create_project (with clues from its windows) or ` +
        `create_project with an existing name to add clues, and assign_project for what clues can't catch. Leave out only what is clearly ` +
        `not work. Then call prepare_day again — the list below will change.`
      : `1. Nothing to sort: every stretch already has a project.`,
    `2. For each project listed, write its daily capsule from that project's part of "The day" below, and save_capsule with ` +
      `title "<Project> — ${day}", project, kind "daily", date "${day}", session_ids.`,
    `3. Update each project's capsule (merge today into the one shown below, or create it): save_capsule with project, kind "project", title "<Project>".`,
    `4. Call finish_day with date "${day}". It tells you if anything is missing. Then reply with its table and what you sorted yourself.`,
    `If save_capsule says "Not saved yet", fix exactly what it lists and save again.`,
  ];

  const head = [
    `# Prepare capsules for ${day}`,
    "",
    "## Checklist",
    ...steps,
    "",
    "## Projects to write up",
    ...(rows.length ? rows : ["- none yet (sort the unsorted work first)"]),
    ...(small.length ? [`(Too little time to write up: ${small.map((p) => `${name(p.id)} ${formatDuration(p.ms)}`).join(", ")}.)`] : []),
    ...(loose ? [`- Not sorted yet · ${loose}`] : []),
  ].join("\n");

  // The growing project capsules to merge into: full text, a fair share each.
  const existing = todo
    .map((p) => capsulesFor(all, p.id, day).whole)
    .filter((c): c is CapsuleInfo => !!c)
    .map((c) => ({ c, text: fs.readFileSync(c.path, "utf8") }));
  const capsuleBudget = Math.floor(maxChars * 0.35);
  const each = existing.length ? Math.floor(capsuleBudget / existing.length) : 0;
  const capsules = existing.length
    ? ["## Current project capsules (merge today into these)", ...existing.map(({ c, text }) => `### ${c.slug}\n${trimTo(text, each)}`)].join("\n\n")
    : "";

  const dayBudget = maxChars - head.length - capsules.length - 40;
  const dayText = condenseDay(day, d.events, d.sessionIds, { maxChars: Math.max(500, dayBudget), gaps: d.gaps });
  return [head, capsules, `## The day\n\n${dayText}`].filter(Boolean).join("\n\n");
}

/** Did every project get its capsules? Returns a table and what's still missing. */
export function finishDay(day: string): { complete: boolean; text: string } {
  const d = loadDay(day);
  const names = projectNames();
  const all = listCapsules();
  const missing: string[] = [];
  const rows = projectsToWrite(d).map((p) => {
    const c = capsulesFor(all, p.id, day);
    const n = names.get(p.id) ?? p.id;
    if (!c.daily) missing.push(`daily capsule for ${n}: save_capsule with project "${p.id}", kind "daily", date "${day}"`);
    if (!updatedFor(c.whole, day)) missing.push(`project capsule for ${n}: save_capsule with project "${p.id}", kind "project"`);
    return `| ${n} | ${formatDuration(p.ms)} | ${c.daily?.slug ?? "—"} | ${updatedFor(c.whole, day) ? c.whole!.slug : "—"} |`;
  });
  const loose = looseLabel(d);
  if (loose) missing.push(`${loose} still "Not sorted yet": sort it (create_project / assign_project), then write its capsules`);

  const table = rows.length
    ? ["| Project | Time | Daily capsule | Project capsule |", "|---|---|---|---|", ...rows].join("\n")
    : "No projects to write up.";

  // What the user should know: drafts and guesses (Unsure), and what wasn't written up (Skipped).
  const unsure: string[] = [];
  for (const p of projectsToWrite(d)) {
    const c = capsulesFor(all, p.id, day);
    for (const cap of [c.daily, updatedFor(c.whole, day) ? c.whole : undefined]) {
      if (!cap) continue;
      if (cap.draft) unsure.push(`${cap.slug} was saved as a draft (some checks didn't pass)`);
      const guesses = (fs.readFileSync(cap.path, "utf8").match(/\(guess\)/gi) ?? []).length;
      if (guesses) unsure.push(`${cap.slug} has ${guesses} guess(es) marked *(guess)*`);
    }
  }
  const skipped = d.projects
    .filter((p) => p.id && !projectsToWrite(d).includes(p))
    .map((p) => `${names.get(p.id) ?? p.id} (${formatDuration(p.ms)}, too little to write up)`);
  const report = [
    ...(unsure.length ? ["Unsure:", ...unsure.map((u) => `- ${u}`)] : []),
    ...(skipped.length ? ["Skipped:", ...skipped.map((k) => `- ${k}`)] : []),
  ].join("\n");

  if (!missing.length) return { complete: true, text: `Done for ${day} ✓\n\n${table}${report ? `\n\n${report}` : ""}` };
  return {
    complete: false,
    text: `Not done yet for ${day}. Still missing:\n${missing.map((m) => `- ${m}`).join("\n")}\n\n${table}${report ? `\n\n${report}` : ""}`,
  };
}
