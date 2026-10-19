// context-capsule — mcp: read recorded sessions (session.json + events.jsonl)
import fs from "node:fs";
import path from "node:path";
import { isSafeName, sessionsDir } from "./paths.js";
import { readJson } from "./fsutil.js";

export interface SessionMeta {
  id: string;
  started_at: string | null;
  ended_at: string | null;
  host?: string;
  version?: number;
}

export interface CaptureEvent {
  t: string;
  end?: string;
  app: string;
  bundle?: string;
  title?: string;
  text?: string;
  /** Where `text` came from: Accessibility or OCR. Missing in older sessions. */
  src?: "ax" | "ocr";
}

export interface SessionSummary {
  id: string;
  started_at: string | null;
  ended_at: string | null;
  in_progress: boolean;
  duration_ms: number;
  events: number;
  top_apps: { app: string; ms: number }[];
  /** How many events got their text from Accessibility vs OCR. */
  sources: { ax: number; ocr: number };
}

/** Minimum time we credit to one sample (the app samples every 3 s). */
const SAMPLE_MS = 3000;

export function listSessionIds(): string[] {
  const dir = sessionsDir();
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .filter((d) => d.isDirectory() && isSafeName(d.name))
    .map((d) => d.name)
    .sort()
    .reverse(); // ids are YYYYMMDD-HHMMSS, so newest first
}

/** Resolve "latest" / "" to the newest session; validate everything else. */
export function resolveSessionId(id: string | undefined): string {
  const wanted = (id ?? "").trim();
  if (!wanted || wanted === "latest") {
    const ids = listSessionIds();
    if (!ids.length) throw new Error(noSessionsMessage());
    return ids[0]!;
  }
  if (!isSafeName(wanted)) throw new Error(`"${wanted}" is not a valid session id (looks like 20261019-100103).`);
  if (!fs.existsSync(path.join(sessionsDir(), wanted))) {
    const ids = listSessionIds().slice(0, 5);
    throw new Error(
      `No session "${wanted}".` + (ids.length ? ` Recent sessions: ${ids.join(", ")}.` : ` ${noSessionsMessage()}`),
    );
  }
  return wanted;
}

export function noSessionsMessage(): string {
  return "No recorded sessions yet. Start one with the start_recording tool or press ⌥⇧C (Option-Shift-C) with the Context Capsule app running.";
}

export function readSessionMeta(id: string): SessionMeta {
  const meta = readJson<Partial<SessionMeta>>(path.join(sessionsDir(), id, "session.json")) ?? {};
  return {
    id,
    started_at: meta.started_at ?? idToIso(id),
    ended_at: meta.ended_at ?? null,
    host: meta.host,
    version: meta.version,
  };
}

/** Read events.jsonl, skipping bad lines (the app may be mid-write). Sorted by time. */
export function readEvents(id: string): CaptureEvent[] {
  const file = path.join(sessionsDir(), id, "events.jsonl");
  let raw = "";
  try {
    raw = fs.readFileSync(file, "utf8");
  } catch {
    return [];
  }
  const out: CaptureEvent[] = [];
  for (const line of raw.split("\n")) {
    if (!line.trim()) continue;
    try {
      const e = JSON.parse(line) as Partial<CaptureEvent>;
      if (typeof e.t !== "string" || Number.isNaN(Date.parse(e.t))) continue;
      out.push({
        t: e.t,
        end: typeof e.end === "string" && !Number.isNaN(Date.parse(e.end)) ? e.end : undefined,
        app: typeof e.app === "string" && e.app ? e.app : "Unknown app",
        bundle: typeof e.bundle === "string" ? e.bundle : undefined,
        title: typeof e.title === "string" ? e.title : "",
        text: typeof e.text === "string" ? e.text : "",
        src: e.src === "ax" || e.src === "ocr" ? e.src : undefined,
      });
    } catch {
      // skip partial or broken lines
    }
  }
  return out.sort((a, b) => Date.parse(a.t) - Date.parse(b.t));
}

/** How long one event covers: t..end, at least one sample. */
export function eventMs(e: CaptureEvent): number {
  const start = Date.parse(e.t);
  const end = e.end ? Date.parse(e.end) : start;
  return Math.max(SAMPLE_MS, end - start);
}

export function appTimes(events: CaptureEvent[]): { app: string; ms: number }[] {
  const byApp = new Map<string, number>();
  for (const e of events) byApp.set(e.app, (byApp.get(e.app) ?? 0) + eventMs(e));
  return [...byApp.entries()].map(([app, ms]) => ({ app, ms })).sort((a, b) => b.ms - a.ms);
}

export function summarizeSession(id: string): SessionSummary {
  const meta = readSessionMeta(id);
  const events = readEvents(id);
  const start = meta.started_at ? Date.parse(meta.started_at) : events[0] ? Date.parse(events[0].t) : NaN;
  const last = events.length ? events[events.length - 1]! : null;
  const lastEnd = last ? Date.parse(last.end ?? last.t) + SAMPLE_MS : NaN;
  const end = meta.ended_at ? Date.parse(meta.ended_at) : lastEnd;
  return {
    id,
    started_at: meta.started_at,
    ended_at: meta.ended_at,
    in_progress: !meta.ended_at,
    duration_ms: Number.isFinite(start) && Number.isFinite(end) ? Math.max(0, end - start) : 0,
    events: events.length,
    top_apps: appTimes(events).slice(0, 5),
    sources: {
      ax: events.filter((e) => e.src === "ax").length,
      ocr: events.filter((e) => e.src === "ocr").length,
    },
  };
}

/** 20261019-100103 → local time ISO guess, used only if session.json is missing. */
function idToIso(id: string): string | null {
  const m = /^(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})/.exec(id);
  if (!m) return null;
  const d = new Date(+m[1]!, +m[2]! - 1, +m[3]!, +m[4]!, +m[5]!, +m[6]!);
  return Number.isNaN(d.getTime()) ? null : d.toISOString();
}

export function formatDuration(ms: number): string {
  const totalMin = Math.round(ms / 60000);
  if (totalMin < 1) return `${Math.max(0, Math.round(ms / 1000))}s`;
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  return h ? `${h}h ${String(m).padStart(2, "0")}m` : `${m}m`;
}

export function formatClock(iso: string | Date): string {
  const d = typeof iso === "string" ? new Date(iso) : iso;
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

export function formatLocalDateTime(iso: string): string {
  const d = new Date(iso);
  const date = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  return `${date} ${formatClock(d)}`;
}
