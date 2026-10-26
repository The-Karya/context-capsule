// context-capsule — ui: Home's numbers — time per project (Today, This week), the streak, the greeting — and
// Claude's corrections (assign.json).

import { detect } from "./projects.js";
import { length } from "./timeline.js";
import { jsonLines, parseISO, str } from "./text.js";

/** Shortest time one moment counts for (one sample). */
export const MIN_SECONDS = 3;

/** assign.json text -> [{from, to, project}] (ms or null); a missing or broken file means none. */
export function parseAssign(text) {
  let obj;
  try {
    obj = JSON.parse(text);
  } catch {
    return [];
  }
  const list = obj && typeof obj === "object" ? obj.assign : null;
  // Like Swift's `as? [[String: Any]]`: one entry that isn't an object drops the list.
  if (!Array.isArray(list) || !list.every((a) => a && typeof a === "object" && !Array.isArray(a))) return [];
  return list
    .filter((a) => typeof a.project === "string")
    .map((a) => ({ from: parseISO(a.from), to: parseISO(a.to), project: a.project }));
}

/** The correction covering `t` (later ones win); "" means "no project"; null: none. */
export function assignedAt(fixes, t) {
  let hit = null;
  for (const f of fixes) if ((f.from == null || t >= f.from) && (f.to == null || t < f.to)) hit = f.project;
  return hit;
}

/** "yyyyMMdd-HHmmss" in local time. */
export function sessionID(ms) {
  const d = new Date(ms);
  const p = (n, w = 2) => String(n).padStart(w, "0");
  return `${p(d.getFullYear(), 4)}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
}

/** Recordings that can hold time from `from` on: ids from the day before (a recording can start then). */
export function weekCandidates(ids, from) {
  const earliest = sessionID(from - 86_400_000);
  return ids.filter((id) => id.length === 15 && id >= earliest);
}

/**
 * Time in windows in use between `from` and `to` (ms): { seconds, byProject: [{id, seconds}] } most first.
 * Notes and windows only looked at don't count; each moment counts at least 3 s. A correction wins, then the
 * recorder's project, then the clues. `filesById`: { id: { events, assign } } (raw texts).
 */
export function totals(filesById, from, to, projects = []) {
  const by = new Map();
  for (const id of weekCandidates(Object.keys(filesById), from)) {
    const files = filesById[id] || {};
    if (files.events == null) continue;
    const fixes = files.assign ? parseAssign(files.assign) : [];
    for (const [, , obj] of jsonLines(files.events)) {
      const t = parseISO(obj.t);
      if (t == null || t < from || t >= to || "kind" in obj || obj.focus === "pointer") continue;
      const end = parseISO(obj.end) ?? t;
      let project = str(obj, "project");
      const fixed = assignedAt(fixes, t);
      if (fixed != null) project = fixed;
      else if (!project) project = detect(projects, { title: str(obj, "title"), where: str(obj, "where"), app: str(obj, "app") })?.id ?? "";
      by.set(project, (by.get(project) || 0) + Math.max(MIN_SECONDS, (end - t) / 1000));
    }
  }
  // Most first; equal ones by id (like the Rust port).
  const byProject = [...by].map(([id, seconds]) => ({ id, seconds }));
  byProject.sort((a, b) => b.seconds - a.seconds || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
  return { seconds: byProject.reduce((n, p) => n + p.seconds, 0), byProject };
}

/** Days in a row with a recording, ending today (or yesterday, when nothing is recorded yet today). */
export function streak(ids, nowMs) {
  const days = new Set(ids.filter((id) => id.length === 15).map((id) => id.slice(0, 8)));
  const day = new Date(nowMs);
  day.setHours(12, 0, 0, 0); // noon: safe across daylight saving
  const key = () => sessionID(day.getTime()).slice(0, 8);
  if (!days.has(key())) day.setDate(day.getDate() - 1);
  let count = 0;
  while (days.has(key())) {
    count++;
    day.setDate(day.getDate() - 1);
  }
  return count;
}

/** The Timeline's words ("1 h 5 min", "under 1 min"); nothing at all is "0 min". */
export function duration(seconds) {
  return seconds < 1 ? "0 min" : length(seconds);
}

/** "Good morning" / "Good afternoon" / "Good evening". */
export function greeting(hour) {
  return hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
}

/** Weeks start on Sunday in these regions (else Monday), when the browser can't say (stats.rs). */
const SUNDAY = "US CA MX BR JP KR TW HK IL PH IN ZA AU SA PE CO VE GT HN PA PR DO SV NI BZ JM TH ID KE ZW BW MO PK BD NP ET MT".split(" ");

/** The locale's first weekday like Swift's Calendar: 1 = Sunday, 2 = Monday… */
export function firstWeekday(locale) {
  try {
    const loc = new Intl.Locale(locale || Intl.DateTimeFormat().resolvedOptions().locale);
    const info = loc.getWeekInfo?.() ?? loc.weekInfo;
    if (info?.firstDay) return (info.firstDay % 7) + 1;
    const region = loc.maximize().region || "";
    return SUNDAY.includes(region) ? 1 : 2;
  } catch {
    return 2;
  }
}

/** Local midnight at the start of this week. `first`: 1 = Sunday, 2 = Monday… (default from the locale). */
export function weekStart(nowMs, first = firstWeekday()) {
  const d = new Date(nowMs);
  d.setHours(0, 0, 0, 0);
  const back = (d.getDay() + 1 - first + 7) % 7;
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() - back).getTime();
}
