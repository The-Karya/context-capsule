// context-capsule — mcp: condense raw screen events into a short timeline an AI can read
import {
  type CaptureEvent,
  type SessionMeta,
  appTimes,
  eventMs,
  formatClock,
  formatDuration,
  formatLocalDateTime,
  isNote,
} from "./sessions.js";

export interface Segment {
  app: string;
  title: string;
  start: string;
  end: string;
  ms: number;
  events: number;
  lines: string[]; // new, cleaned text lines for this segment
}

export const DEFAULT_MAX_CHARS = 60000;
const ELLIPSIS = "…";

/** Collapse all whitespace runs to one space and trim. */
export function collapse(s: string): string {
  return s.replace(/\s+/g, " ").trim();
}

const lineKey = (line: string) => line.toLowerCase();

const DAYS = "mon(day)?|tue(s(day)?)?|wed(nesday)?|thu(rs(day)?)?|fri(day)?|sat(urday)?|sun(day)?";
const MONTHS =
  "jan(uary)?|feb(ruary)?|mar(ch)?|apr(il)?|may|june?|july?|aug(ust)?|sep(t(ember)?)?|oct(ober)?|nov(ember)?|dec(ember)?";
const DATE_LINE = new RegExp(
  `^((${DAYS})\\.?,?\\s*)?((${MONTHS})\\.?\\s*)?\\d{1,4}([,\\s:/.-]+\\d{1,4})*(\\s*[ap]\\.?m\\.?)?$`,
  "i",
);

/** Lines that carry no meaning on their own: tiny, numbers, clocks, dates, symbols. */
export function isNoiseLine(line: string): boolean {
  if (line.length < 4) return true;
  const letters = (line.match(/\p{L}/gu) ?? []).length;
  if (letters < 3) return true; // "12", "10:14 AM", "• • •", "3/4", "+12.5%"
  // "Mon 19 Oct 10:14", "Oct 19, 2026", "Today at 9:41"
  if (DATE_LINE.test(line)) return true;
  if (/^(today|yesterday)( at)? \d{1,2}[:.]\d{2}(\s*[ap]\.?m\.?)?$/i.test(line)) return true;
  return false;
}

type RawSegment = Segment & { raw: string[][]; idx: number };

/** Group consecutive screen events with the same app + window title. A voice or typed note
 * ends the run (it sits between segments in the timeline) and is not part of any segment. */
export function groupSegments(events: CaptureEvent[]): RawSegment[] {
  const segs: RawSegment[] = [];
  let broken = false;
  events.forEach((e, idx) => {
    if (isNote(e)) {
      broken = true;
      return;
    }
    const title = collapse(e.title ?? "");
    const last = broken ? undefined : segs[segs.length - 1];
    broken = false;
    const lines = (e.text ?? "").split(/\r?\n/).map(collapse).filter(Boolean);
    const end = e.end ?? e.t;
    if (last && last.app === e.app && last.title === title) {
      if (Date.parse(end) > Date.parse(last.end)) last.end = end;
      last.ms += eventMs(e);
      last.events += 1;
      last.raw.push(lines);
    } else {
      segs.push({ app: e.app, title, start: e.t, end, ms: eventMs(e), events: 1, lines: [], raw: [lines], idx });
    }
  });
  return segs;
}

/**
 * Lines repeated across many different windows are UI chrome (menus, sidebars, tabs).
 * A short line found in at least 3 segments and in 30%+ of all segments is dropped.
 */
export function findChrome(segs: { raw: string[][] }[]): Set<string> {
  const segCount = new Map<string, number>();
  for (const s of segs) {
    const seen = new Set<string>();
    for (const lines of s.raw) for (const l of lines) seen.add(lineKey(l));
    for (const k of seen) segCount.set(k, (segCount.get(k) ?? 0) + 1);
  }
  const need = Math.max(3, Math.ceil(segs.length * 0.3));
  const chrome = new Set<string>();
  for (const [k, n] of segCount) if (n >= need && k.length <= 80) chrome.add(k);
  return chrome;
}

/** Group, drop noise and chrome, and keep each text line only the first time it shows up. */
export function buildSegments(events: CaptureEvent[]): Segment[] {
  return buildRaw(events).map(({ raw: _raw, idx: _idx, ...s }) => s);
}

function buildRaw(events: CaptureEvent[]): RawSegment[] {
  const segs = groupSegments(events);
  const chrome = findChrome(segs);
  const seen = new Set<string>();
  for (const s of segs) {
    // the window title is already in the header, so don't repeat it as text
    if (s.title) seen.add(lineKey(s.title));
    for (const lines of s.raw) {
      for (const line of lines) {
        const k = lineKey(line);
        if (seen.has(k) || chrome.has(k) || isNoiseLine(line)) continue;
        seen.add(k);
        s.lines.push(line);
      }
    }
  }
  return segs;
}

/** `>> You said (10:14): "…"` — the user's own words, on one line. */
export function noteLine(e: CaptureEvent): string {
  const who = e.kind === "voice" ? "You said" : "You typed";
  return `>> ${who} (${formatClock(e.t)}): "${collapse(e.text ?? "")}"`;
}

/**
 * Split `budget` chars fairly: every item gets an equal share, and what short items
 * don't use flows to the longer ones (water-filling). Returns one allotment per item.
 */
export function fairShares(lengths: number[], budget: number): number[] {
  const out = new Array<number>(lengths.length).fill(0);
  const order = lengths.map((len, i) => ({ len, i })).sort((a, b) => a.len - b.len);
  let left = Math.max(0, budget);
  order.forEach(({ len, i }, idx) => {
    const share = Math.floor(left / (order.length - idx));
    const give = Math.min(len, share);
    out[i] = give;
    left -= give;
  });
  return out;
}

/** Fit text into `limit` chars, cutting with "…" (at a line end when possible). */
export function trimTo(text: string, limit: number): string {
  if (text.length <= limit) return text;
  if (limit <= 1) return limit === 1 ? ELLIPSIS : "";
  const cut = text.slice(0, limit - 1);
  const nl = cut.lastIndexOf("\n");
  // prefer a clean line break if it keeps most of the allotment
  const body = nl > limit * 0.6 ? cut.slice(0, nl) : cut.replace(/\s+\S*$/, "") || cut;
  return body.trimEnd() + ELLIPSIS;
}

function segmentHeader(s: Segment): string {
  const span = `${formatClock(s.start)}–${formatClock(s.end)}`;
  const where = s.title ? `${s.app} — ${s.title}` : s.app;
  return `## ${span} · ${where}`;
}

export interface ActivityOptions {
  maxChars?: number;
}

/** The full condensed timeline for one session, never longer than maxChars. Voice and typed
 * notes are the user's own words: they get their budget first and are never cut before screen text. */
export function condenseActivity(meta: SessionMeta, events: CaptureEvent[], opts: ActivityOptions = {}): string {
  const maxChars = Math.max(200, Math.floor(opts.maxChars ?? DEFAULT_MAX_CHARS));
  const segs = buildRaw(events);
  const screen = events.filter((e) => !isNote(e));
  const notes = events.map((e, idx) => ({ e, idx })).filter(({ e }) => isNote(e) && collapse(e.text ?? ""));

  const startIso = meta.started_at ?? events[0]?.t ?? null;
  const lastEvent = events[events.length - 1];
  const endIso = meta.ended_at ?? lastEvent?.end ?? lastEvent?.t ?? null;
  const span =
    startIso && endIso
      ? `${formatLocalDateTime(startIso)}–${formatClock(endIso)} (${formatDuration(Date.parse(endIso) - Date.parse(startIso))})`
      : startIso
        ? `${formatLocalDateTime(startIso)}`
        : "unknown";
  const apps = appTimes(events)
    .slice(0, 8)
    .map((a) => `${a.app} ${formatDuration(a.ms)}`)
    .join(", ");

  const ax = screen.filter((e) => e.src === "ax").length;
  const ocr = screen.filter((e) => e.src === "ocr").length;
  const source =
    ax && ocr
      ? `Text is from Accessibility (${ax} ${ax === 1 ? "sample" : "samples"}) and OCR (${ocr} ${ocr === 1 ? "sample" : "samples"}, may be noisy)`
      : ax
        ? "Text is from Accessibility"
        : "Text is OCR";
  const voice = notes.filter(({ e }) => e.kind === "voice").length;
  const typed = notes.length - voice;
  const noteHead = notes.length
    ? `\n${[voice && `${voice} voice ${voice === 1 ? "note" : "notes"}`, typed && `${typed} typed ${typed === 1 ? "note" : "notes"}`]
        .filter(Boolean)
        .join(" and ")} (lines starting with ">>"): the user's own words about what they did and why.`
    : "";
  const head = [
    `# Session ${meta.id}${meta.ended_at ? "" : " (still recording or not closed)"}`,
    `Time: ${span}, local time`,
    `Apps: ${apps || "none"}`,
    `${segs.length} segments from ${screen.length} screen samples. ${source}: repeated lines, menus and numbers-only lines are removed; each line appears once, where it was first seen.${noteHead}`,
  ].join("\n");

  if (!segs.length && !notes.length) return trimTo(`${head}\n\n(No activity was captured in this session.)`, maxChars);

  // Timeline order: segments and notes by where they sit in events.jsonl.
  type Item = { kind: "seg"; i: number; idx: number } | { kind: "note"; line: string; idx: number };
  const items: Item[] = [
    ...segs.map((s, i) => ({ kind: "seg" as const, i, idx: s.idx })),
    ...notes.map(({ e, idx }) => ({ kind: "note" as const, line: noteLine(e), idx })),
  ].sort((a, b) => a.idx - b.idx);

  const headers = segs.map(segmentHeader);
  const noteChars = notes.length ? items.reduce((n, it) => n + (it.kind === "note" ? 2 + it.line.length : 0), 0) : 0;
  // fixed cost: head + notes in full + per segment "\n\n" + header + "\n" before a body
  const fixed = head.length + noteChars + headers.reduce((n, h) => n + 2 + h.length + 1, 0);
  const bodies = segs.map((s) => s.lines.join("\n"));
  const shares = fairShares(
    bodies.map((b) => b.length),
    maxChars - fixed,
  );

  const render = (withSegments: boolean) => {
    const parts = [head];
    for (const it of items) {
      if (it.kind === "note") parts.push(it.line);
      else if (withSegments) {
        const body = bodies[it.i] ? trimTo(bodies[it.i]!, shares[it.i]!) : "";
        parts.push(body ? `${headers[it.i]}\n${body}` : headers[it.i]!);
      }
    }
    return parts;
  };
  const parts = render(true);
  let out = parts.join("\n\n");

  if (out.length > maxChars) {
    if (notes.length) {
      // the notes come first: drop screen segments before any of the user's words
      const noteOnly = render(false);
      const cut = `\n\n(${segs.length} screen segments left out to fit max_chars; notes kept)`;
      const joined = noteOnly.join("\n\n");
      out = joined.length + cut.length <= maxChars ? joined + cut : trimTo(joined, maxChars);
    } else {
      // even the headers don't fit: keep as many whole segments as we can
      const note = (n: number) => `\n\n(${n} more segments cut to fit max_chars)`;
      let kept = head;
      for (let i = 1; i < parts.length; i++) {
        const next = `${kept}\n\n${parts[i]}`;
        if (next.length + note(parts.length - 1 - i).length > maxChars) {
          kept += note(parts.length - i);
          break;
        }
        kept = next;
      }
      out = trimTo(kept, maxChars);
    }
  }
  return out;
}
