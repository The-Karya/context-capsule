// context-capsule — ui: the Timeline — one day's recordings, their reads grouped into stretches, notes and gaps,
// the day's recorded time, day bar, "Where your time went", search, and the page's words. A port of `Timeline`
// (capture/Sources/Core.swift) and TimelineView.swift; core/src/timeline.rs has the same rules (SPEC "Timeline").
// Times are ms; local time is the page's (the JS runtime's) time zone.

import { detect } from "./projects.js";
import { parseAssign, assignedAt } from "./stats.js";
import { addDays, parseKey } from "./dates.js";
import { graphemes, jsonLines, lines, parseISO, searchable, str, trimSpaces, utf8Length } from "./text.js";

export { searchable };

export const PREVIEW_LINES = 3;
export const PREVIEW_WIDTH = 300;
/** A stretch ends when the next read starts more than this after the last one's end (ms). */
export const PAUSE = 60_000;
const PEEK_SHORTEST = 20;
const PEEK_WIDTH = 120;

// MARK: sessions and days

/** "YYYYMMDD-HHMMSS" and nothing else (never a path). */
export function isSessionID(id) {
  return typeof id === "string" && /^\d{8}-\d{6}$/.test(id);
}

/** A session id (local time) -> when it started (ms); null when it isn't one. */
export function sessionStart(id) {
  if (!isSessionID(id)) return null;
  const n = (a, b) => +id.slice(a, b);
  const d = new Date(n(0, 4), n(4, 6) - 1, n(6, 8), n(9, 11), n(11, 13), n(13, 15));
  return d.getMonth() === n(4, 6) - 1 && d.getDate() === n(6, 8) ? d.getTime() : null;
}

/** Sessions that can overlap the day: started that day or the day before (a recording can run past midnight). */
export function candidates(ids, dateKey) {
  const today = dateKey.replaceAll("-", "");
  const before = addDays(dateKey, -1).replaceAll("-", "");
  return ids.filter((id) => isSessionID(id) && (id.startsWith(today) || id.startsWith(before))).sort();
}

/** Days with a recording ("YYYY-MM-DD"), from the session ids: the calendar's dots. */
export function recordedDays(ids) {
  return new Set(ids.filter(isSessionID).map((id) => `${id.slice(0, 4)}-${id.slice(4, 6)}-${id.slice(6, 8)}`));
}

// MARK: reading

/** The first lines of a read's text (blank lines left out, long lines cut with "…"), and whether there's more. */
export function fold(text, max = PREVIEW_LINES, width = PREVIEW_WIDTH) {
  const kept = [];
  let more = false;
  for (const raw of lines(text)) {
    const line = trimSpaces(raw);
    if (!line) continue;
    if (kept.length === max) {
      more = true;
      break;
    }
    const g = graphemes(line);
    if (g.length > width) {
      kept.push(g.slice(0, width - 1).join("") + "…");
      more = true;
    } else kept.push(line);
  }
  return [kept.join("\n"), more];
}

/** The first line of 20 or more letters (trimmed), cut at 120 with "…"; "" if no line is that long. */
export function peek(text) {
  for (const raw of lines(text)) {
    const g = graphemes(trimSpaces(raw));
    if (g.length < PEEK_SHORTEST) continue;
    return g.length > PEEK_WIDTH ? g.slice(0, PEEK_WIDTH).join("") + "…" : g.join("");
  }
  return "";
}

/** The day, local midnight to midnight: every recording on it, in time order. */
export function loadDay(dateKey, filesById, { projects = [], recording = null } = {}) {
  const from = parseKey(dateKey);
  const to = parseKey(addDays(dateKey, 1));
  const blocks = [];
  if (from == null) return { blocks, from, to };
  for (const id of candidates(Object.keys(filesById || {}), dateKey)) {
    const b = loadBlock(id, filesById[id] || {}, from, to, { projects, recording });
    if (b) blocks.push(b);
  }
  blocks.sort((a, b) => a.start - b.start);
  return { blocks, from, to };
}

/** One session's part of [from, to); null when nothing of it is on that day (and it didn't start then). */
export function loadBlock(id, files, from, to, { projects = [], recording = null } = {}) {
  const fixes = files.assign ? parseAssign(files.assign) : [];
  const moments = [];
  for (const [index, line, obj] of jsonLines(files.events)) {
    const stamp = obj.t;
    const t = parseISO(stamp);
    if (t == null || t < from || t >= to) continue;
    const e = parseISO(obj.end);
    const end = e == null ? t : Math.max(e, t);
    const kind = str(obj, "kind");
    const isNote = kind === "voice" || kind === "note";
    const app = str(obj, "app");
    const title = str(obj, "title");
    const where = str(obj, "where");
    const text = str(obj, "text");
    // A correction, the recorder's choice, then the clues (as Home and the Claude side).
    let project = str(obj, "project");
    const fixed = assignedAt(fixes, t);
    if (fixed != null) project = fixed;
    else if (!project && !(isNote && !app)) project = detect(projects, { title, where, app })?.id ?? "";
    const [preview, more] = isNote ? [text.trim(), false] : fold(text);
    moments.push({
      id: `${id}#${index}`,
      session: id,
      stamp,
      line,
      t,
      end,
      app,
      bundle: str(obj, "bundle"),
      title,
      where,
      project,
      kind: isNote ? kind : "",
      lookedAt: obj.focus === "pointer",
      text,
      preview,
      more,
      peek: isNote ? "" : peek(text),
      size: utf8Length(text),
      hay: searchable([title, where, app, text].join("\n")),
      isNote,
    });
  }
  const gaps = [];
  for (const [, , obj] of jsonLines(files.gaps)) {
    const t = parseISO(obj.t);
    if (t == null || typeof obj.why !== "string") continue;
    const e = parseISO(obj.end);
    const end = e == null ? t : Math.max(e, t);
    if (end < from || t >= to) continue;
    const g = { session: id, t: Math.max(t, from), end: Math.min(end, to), why: obj.why };
    gaps.push({ id: `${id}~${Math.floor(g.t / 1000)}~${g.why}`, ...g, message: gapMessage(g.why) });
  }
  const started = sessionStart(id);
  const startedToday = started != null && started >= from && started < to;
  if (!moments.length && !gaps.length && !startedToday) return null;

  const items = [...moments.map((m) => ({ type: "moment", moment: m, t: m.t })), ...gaps.map((g) => ({ type: "gap", gap: g, t: g.t }))];
  items.sort((a, b) => a.t - b.t); // stable: file order within the same time
  // The block's project: the most time (not notes, not looked at); a tie goes to the smaller id.
  const byProject = new Map();
  for (const m of moments) {
    if (m.isNote || m.lookedAt || !m.project) continue;
    byProject.set(m.project, (byProject.get(m.project) || 0) + Math.max(3, (m.end - m.t) / 1000));
  }
  let project = "";
  for (const [k, v] of byProject) {
    const best = byProject.get(project) ?? -1;
    if (v > best || (v === best && k < project)) project = k;
  }
  const times = [...moments.flatMap((m) => [m.t, m.end]), ...gaps.flatMap((g) => [g.t, g.end])];
  const start = times.length ? Math.min(...times) : started ?? from;
  let end = times.length ? Math.max(...times) : start;
  if (!times.length) {
    const ended = endedAt(files.session);
    if (ended != null) end = Math.min(Math.max(ended, start), to);
  }
  const notes = moments.filter((m) => m.isNote).length;
  return {
    session: id,
    start,
    end,
    project,
    moments: moments.length - notes,
    notes,
    items,
    entries: entries(items),
    recording: recording === true || (typeof recording === "string" && recording === id),
  };
}

function endedAt(text) {
  if (!text) return null;
  try {
    return parseISO(JSON.parse(text)?.ended_at);
  } catch {
    return null;
  }
}

/** After "Not read 10:40 – 10:55: " (Gaps.message). */
export function gapMessage(why) {
  return (
    {
      private_app: "a private app was in front",
      not_shared: "this window wasn't shared",
      no_text: "nothing readable",
      system: "system screen",
      locked: "the screen was locked",
    }[why] ?? "not read"
  );
}

// MARK: stretches

/** The most common value that isn't empty (the first seen wins a tie); "" if none. */
export function mostUsed(values) {
  const count = new Map();
  let best = "";
  for (const v of values) {
    if (!v) continue;
    count.set(v, (count.get(v) || 0) + 1);
    if (v !== best && count.get(v) > (count.get(best) || 0)) best = v;
  }
  return best;
}

/** Reads in a row with the same app and title (never empty). */
function makeStretch(reads) {
  const first = reads[0];
  const last = reads[reads.length - 1];
  const start = first.t;
  const end = Math.max(last.end, start);
  const app = first.app;
  const title = first.title;
  return {
    id: first.id,
    session: first.session,
    reads,
    found: null,
    start,
    end,
    app,
    bundle: first.bundle,
    title,
    where: reads.find((m) => m.where)?.where ?? "",
    peek: reads.reduce((a, m) => (m.size > a.size ? m : a), first).peek,
    project: mostUsed(reads.map((m) => m.project)),
    seconds: (end - start) / 1000,
    name: !title ? app || "A window" : app ? `${app} · ${title}` : title,
  };
}

/**
 * Items in time order ({type: "moment"|"gap", moment|gap}) grouped: reads in a row with the same app and title
 * make one stretch; a note, a gap, or more than a minute between two reads ends it.
 */
export function entries(items) {
  const out = [];
  let run = [];
  const close = () => {
    if (run.length) {
      const s = makeStretch(run);
      out.push({ type: "stretch", id: "s-" + s.id, stretch: s });
    }
    run = [];
  };
  for (const item of items) {
    if (item.type === "gap") {
      close();
      out.push({ type: "gap", id: item.gap.id, gap: item.gap });
    } else if (item.moment.isNote) {
      close();
      out.push({ type: "note", id: item.moment.id, moment: item.moment });
    } else {
      const m = item.moment;
      const last = run[run.length - 1];
      if (last && (last.app !== m.app || last.title !== m.title || m.t - last.end > PAUSE)) close();
      run.push(m);
    }
  }
  close();
  return out;
}

/** The stretches of a block, or of every block of a day. */
export function stretchesOf(x) {
  const blocks = x.blocks ?? (Array.isArray(x) ? x : [x]);
  return blocks.flatMap((b) => b.entries.filter((e) => e.type === "stretch").map((e) => e.stretch));
}

/** The day's recorded time (s): the stretches' lengths added up. */
export function recorded(blocks) {
  return stretchesOf(blocks).reduce((n, s) => n + s.seconds, 0);
}

/** Where your time went: time per project ("" = not sorted), longest first; per app when no stretch has one. */
export function shares(stretches) {
  const byProject = stretches.some((s) => s.project);
  const time = new Map();
  for (const s of stretches) {
    const key = byProject ? s.project : s.app;
    time.set(key, (time.get(key) || 0) + s.seconds);
  }
  // Longest first; a tie keeps the order they came in (sort is stable).
  const rows = [...time].map(([key, seconds]) => ({ key, seconds })).sort((a, b) => b.seconds - a.seconds);
  return { byProject, rows };
}

/** What a folded recording names: its projects by time, or its top 2 apps when none has a project. */
export function names(block) {
  const s = shares(stretchesOf(block));
  const keys = s.rows.map((r) => r.key).filter((k) => k);
  return { byProject: s.byProject, keys: s.byProject ? keys : keys.slice(0, 2) };
}

/** The day bar: from the start of the first stretch's hour to the end of the last one's (local hours). */
export function barRange(stretches) {
  if (!stretches.length) return null;
  const hour = (ms) => {
    const d = new Date(ms);
    d.setMinutes(0, 0, 0);
    return d.getTime();
  };
  const first = Math.min(...stretches.map((s) => s.start));
  const last = Math.max(...stretches.map((s) => s.end));
  return { from: hour(first), to: hour(last) + 3_600_000 };
}

/** The day bar's hour labels: where each is (0–1); at most about 8. */
export function barTicks(range) {
  if (!range) return [];
  const total = Math.max(1, (range.to - range.from) / 1000);
  const hours = Math.round(total / 3600);
  const step = Math.max(1, Math.ceil(hours / 8));
  const out = [];
  for (let h = 0; h <= hours; h += step) out.push({ t: range.from + h * 3_600_000, at: (h * 3600) / total });
  return out;
}

/** Colours, in this order: blue, teal, orange, green, pink, brown. */
export const COLOURS = ["blue", "teal", "orange", "green", "pink", "brown"];

/** keys longest first ("" ignored) -> key -> colour index 0–5, or -1 for none (grey). */
export function palette(keys) {
  const list = keys.filter((k) => k);
  return (key) => {
    const i = key ? list.indexOf(key) : -1;
    return i < 0 ? -1 : i % COLOURS.length;
  };
}

/** A stretch's key for the palette: its project, or its app when the day goes by app. */
export function paletteKey(s, byProject) {
  return byProject ? s.project : s.app;
}

// MARK: search

export function words(query) {
  return searchable(query).split(/\s+/u).filter((w) => w);
}

function matches(hay, w) {
  return w.every((x) => hay.includes(x));
}

/** Every recording of the day, each with only what matches (stretches get `found`; no gaps). Empty: all. */
export function filterDay(day, query) {
  const w = words(query);
  if (!w.length) return day.blocks;
  return day.blocks.map((b) => ({
    ...b,
    entries: b.entries.flatMap((e) => {
      if (e.type === "stretch") {
        const found = e.stretch.reads.filter((m) => matches(m.hay, w));
        return found.length ? [{ ...e, stretch: { ...e.stretch, found } }] : [];
      }
      if (e.type === "note") return matches(e.moment.hay, w) ? [e] : [];
      return [];
    }),
  }));
}

// MARK: words

/** "under 1 min", "38 min", "1 h", "1 h 50 min" (to the nearest minute). */
export function length(seconds) {
  if (!(seconds >= 60)) return "under 1 min";
  const m = Math.round(seconds / 60);
  if (m < 60) return `${m} min`;
  return m % 60 === 0 ? `${m / 60} h` : `${Math.floor(m / 60)} h ${m % 60} min`;
}

/** "17 reads", "1 read"; while searching "3 of 17 reads match". */
export function readsLine(stretch) {
  const n = stretch.reads.length;
  const reads = `${n} read${n === 1 ? "" : "s"}`;
  return stretch.found ? `${stretch.found.length} of ${reads} match` : reads;
}

/** Under the day control: "3 h 32 min recorded · 3 recordings"; "Nothing recorded". */
export function summary(day) {
  const n = day.blocks.length;
  if (n === 0) return "Nothing recorded";
  const parts = [];
  if (stretchesOf(day).length) parts.push(`${length(recorded(day.blocks))} recorded`);
  parts.push(`${n} recording${n === 1 ? "" : "s"}`);
  return parts.join(" · ");
}

/** When the day shows nothing. `recordKey`: the shortcut as shown ("⌥C"). */
export function emptyLine({ searching = false, today = false, recordKey = "" } = {}) {
  if (searching) return "Nothing on this day matches.";
  return today ? `Nothing recorded today yet. Start contextualizing (${recordKey}) when you begin work.` : "Nothing recorded on this day.";
}

/** An opened recording with nothing to list. */
export function nothingInBlock(searching) {
  return searching ? "Nothing here matches." : "Nothing was recorded.";
}

/** A folded recording: "10:00 – 11:40 · 1 h 40 min · Website, Client report". `nameList`: names already joined or a list. */
export function blockLine(block, span, nameList) {
  const parts = [span, length((block.end - block.start) / 1000)];
  const n = Array.isArray(nameList) ? nameList.join(", ") : nameList;
  if (n) parts.push(n);
  return parts.join(" · ");
}

/** Under a note's bubble: "You said · 10:14 · in Figma" / "You typed · 10:14". */
export function noteCaption(m, time) {
  return (m.kind === "voice" ? "You said" : "You typed") + " · " + time + (m.app ? ` · in ${m.app}` : "");
}

/** "Not read 10:40 – 10:55: a private app was in front". */
export function gapLine(gap, span) {
  return `Not read ${span}: ${gap.message}`;
}

/** "this Mac" / "this PC" / "this computer". */
export function thisComputerFor(platform) {
  return platform === "mac" ? "this Mac" : platform === "windows" ? "this PC" : "this computer";
}

const KEPT = "Capsules already made stay as they are.";

/** The menu item for a read or note. */
export function deleteMomentMenu(m) {
  return m.isNote ? (m.kind === "voice" ? "Delete this voice note…" : "Delete this note…") : "Delete this read…";
}

export function deleteMomentText(m, thisComputer = "this Mac") {
  const what = m.isNote ? (m.kind === "voice" ? "voice note" : "note") : "read";
  return { title: `Delete this ${what}? It can't be undone.`, message: `It's removed from ${thisComputer}. ${KEPT}` };
}

export function deleteStretchText(s, span, thisComputer = "this Mac") {
  const n = s.reads.length;
  return {
    title: "Delete this stretch? It can't be undone.",
    message: `${n === 1 ? "Its read" : `All ${n} reads`} from ${span} ${n === 1 ? "is" : "are"} removed from ${thisComputer}. ${KEPT}`,
  };
}

export function deleteBlockText(b, startText, endText, thisComputer = "this Mac") {
  return {
    title: "Delete this recording? It can't be undone.",
    message: `Everything recorded from ${startText} to ${endText} is removed from ${thisComputer}. ${KEPT}`,
  };
}

/** Why a delete didn't happen: "recording", "gone", or the error's words. */
export function refusedText(why) {
  if (why === "recording") return { title: "Stop first", message: "This recording is still going. Stop it, then delete." };
  if (why === "gone") return { title: "Already gone", message: "It isn't there anymore. The page is up to date now." };
  return { title: "Couldn't delete", message: String(why ?? "") };
}

/** What "Make a capsule" asks Your AI. */
export function sessionPrompt(id) {
  return `Use Context Capsule to make a capsule from my session ${id}.`;
}
