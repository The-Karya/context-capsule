// context-capsule — mcp: capsule files (front matter, save, list, get, import, export)
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { capsulesDir, configFile, expandHome, homeDir } from "./paths.js";
import { readJson, writeFileAtomic, writeUnique } from "./fsutil.js";
import { checkCapsule, checkCitations, type CheckResult } from "./checks.js";
import { PLAYBOOK_VERSION } from "./playbook.js";
import { loadProjects } from "./projects.js";
import { appTimes, readEvents, resolveSessionId } from "./sessions.js";

export const CAPSULE_EXT = ".capsule.md";
/** A capsule is context for the reader's Claude first; "In short" is the only part written for people. */
export const REQUIRED_SECTIONS = [
  "In short",
  "For Claude",
  "Where things stand",
  "What was done",
  "Decisions",
  "Open questions",
  "Next steps",
  "People, links and files",
];


export type FrontValue = string | number | boolean | string[];
export type FrontMatter = Record<string, FrontValue>;

export interface CapsuleInfo {
  slug: string;
  title: string;
  author: string;
  created: string;
  path: string;
  /** Project id, and "daily" / "project" (a growing project capsule), when set. */
  project?: string;
  kind?: string;
  date?: string;
  /** Saved before every check passed. */
  draft?: boolean;
}

export type CapsuleKind = "daily" | "project";

// ---------- slugs ----------

export function slugify(title: string): string {
  const slug = title
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60)
    .replace(/-+$/g, "");
  return slug || "capsule";
}

// ---------- front matter (a tiny YAML subset: scalars and [flow, lists]) ----------

const PLAIN = /^[A-Za-z0-9_][A-Za-z0-9 _.\-/()&+'’!?]*$/;
const RESERVED = /^(true|false|yes|no|on|off|null|~|-?\d+(\.\d+)?)$/i;

const ISO_TIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$/;

function yamlScalar(v: string): string {
  if (ISO_TIME.test(v)) return v;
  return PLAIN.test(v) && !RESERVED.test(v) && v === v.trim() ? v : JSON.stringify(v);
}

export function stringifyFrontMatter(fm: FrontMatter): string {
  const lines = Object.entries(fm).map(([k, v]) => {
    if (Array.isArray(v)) return `${k}: [${v.map(yamlScalar).join(", ")}]`;
    if (typeof v === "number" || typeof v === "boolean") return `${k}: ${v}`;
    return `${k}: ${yamlScalar(v)}`;
  });
  return `---\n${lines.join("\n")}\n---\n`;
}

function parseScalar(raw: string): string | number | boolean {
  const v = raw.trim();
  if (v.startsWith('"')) {
    const end = findQuoteEnd(v, '"');
    try {
      return JSON.parse(v.slice(0, end + 1)) as string;
    } catch {
      return v.slice(1, end);
    }
  }
  if (v.startsWith("'")) {
    const end = findQuoteEnd(v, "'");
    return v.slice(1, end).replace(/''/g, "'");
  }
  const noComment = v.replace(/\s+#.*$/, "").trim();
  if (/^-?\d+(\.\d+)?$/.test(noComment)) return Number(noComment);
  if (/^(true|false)$/i.test(noComment)) return noComment.toLowerCase() === "true";
  return noComment;
}

function findQuoteEnd(s: string, q: string): number {
  for (let i = 1; i < s.length; i++) {
    if (q === '"' && s[i] === "\\") {
      i++;
      continue;
    }
    if (s[i] === q) {
      if (q === "'" && s[i + 1] === "'") {
        i++;
        continue;
      }
      return i;
    }
  }
  return s.length - 1;
}

function parseFlowList(raw: string): string[] {
  const inner = raw.trim().replace(/\s+#.*$/, "");
  const body = inner.slice(1, inner.lastIndexOf("]") === -1 ? undefined : inner.lastIndexOf("]"));
  const items: string[] = [];
  let cur = "";
  let quote: string | null = null;
  for (let i = 0; i < body.length; i++) {
    const c = body[i]!;
    if (quote) {
      cur += c;
      if (c === "\\" && quote === '"') cur += body[++i] ?? "";
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'") {
      quote = c;
      cur += c;
    } else if (c === ",") {
      items.push(cur);
      cur = "";
    } else cur += c;
  }
  items.push(cur);
  return items.map((s) => s.trim()).filter(Boolean).map((s) => String(parseScalar(s)));
}

/** Split "---\nfront\n---\nbody". Returns null front matter if there is none. */
export function parseCapsule(text: string): { front: FrontMatter | null; body: string } {
  const src = text.replace(/^﻿/, "");
  const m = /^---[ \t]*\r?\n([\s\S]*?)\r?\n---[ \t]*(\r?\n|$)/.exec(src);
  if (!m) return { front: null, body: src };
  const front: FrontMatter = {};
  const lines = m[1]!.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    const kv = /^([A-Za-z_][\w-]*)\s*:\s*(.*)$/.exec(line);
    if (!kv) continue;
    const [, key, value] = kv as unknown as [string, string, string];
    if (value.trim().startsWith("[")) front[key] = parseFlowList(value);
    else if (value.trim() === "" || value.trim().startsWith("#")) {
      // block list:  key:\n  - a\n  - b
      const list: string[] = [];
      while (i + 1 < lines.length && /^\s*-\s+/.test(lines[i + 1]!)) {
        list.push(String(parseScalar(lines[++i]!.replace(/^\s*-\s+/, ""))));
      }
      front[key] = list.length ? list : "";
    } else front[key] = parseScalar(value);
  }
  return { front, body: src.slice(m[0].length) };
}

export function validateCapsuleText(text: string): { ok: true; front: FrontMatter; body: string } | { ok: false; error: string } {
  const { front, body } = parseCapsule(text);
  if (!front)
    return { ok: false, error: "This file has no capsule front matter (it should start with a `---` block containing `capsule: 1`)." };
  if (front.capsule === undefined) return { ok: false, error: "The front matter is missing `capsule: 1`, so this is not a capsule file." };
  if (front.capsule !== 1)
    return { ok: false, error: `Unsupported capsule version "${String(front.capsule)}". This server reads \`capsule: 1\`.` };
  if (typeof front.title !== "string" || !front.title.trim())
    return { ok: false, error: "The capsule front matter has no `title`." };
  return { ok: true, front, body };
}

export function missingSections(markdown: string): string[] {
  const headings = new Set(
    [...markdown.matchAll(/^##\s+(.+?)\s*#*\s*$/gm)].map((m) => m[1]!.toLowerCase().replace(/[:.]$/, "")),
  );
  return REQUIRED_SECTIONS.filter((s) => !headings.has(s.toLowerCase()));
}

// ---------- secrets: a last safety net ----------

const SECRET_PATTERNS: { kind: string; re: RegExp }[] = [
  { kind: "API key", re: /\bsk-[A-Za-z0-9_-]{16,}/g },
  { kind: "GitHub token", re: /\bgh[pousr]_[A-Za-z0-9]{20,}/g },
  { kind: "GitHub token", re: /\bgithub_pat_[A-Za-z0-9_]{20,}/g },
  { kind: "Slack token", re: /\bxox[bpasr]-[A-Za-z0-9-]{10,}/g },
  { kind: "AWS key", re: /\b(?:AKIA|ASIA)[0-9A-Z]{16}\b/g },
  { kind: "bearer token", re: /\bbearer\s+[A-Za-z0-9._\-+/=]{16,}/gi },
  { kind: "private key", re: /-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?(-----END [A-Z ]*PRIVATE KEY-----|$)/g },
  { kind: "IBAN", re: /\b[A-Z]{2}\d{2}(?: ?[A-Z0-9]{4}){3,7}(?: ?[A-Z0-9]{1,4})?\b/g },
  { kind: "password", re: /^(\s*[-*]?\s*password\s*[:=]\s*).+$/gim },
];

/** 13–19 digits (spaces or dashes allowed) passing the Luhn check: a card number. */
const CARD = /\b(?:\d[ -]?){12,18}\d\b/g;
function luhn(digits: string): boolean {
  let sum = 0;
  for (let i = 0; i < digits.length; i++) {
    let d = Number(digits[digits.length - 1 - i]);
    if (i % 2 === 1) {
      d *= 2;
      if (d > 9) d -= 9;
    }
    sum += d;
  }
  return sum % 10 === 0;
}

/** Remove secrets and payment numbers. Returns how many, and which kinds (for a plain-words note). */
export function redactSecrets(text: string): { text: string; count: number; kinds: string[] } {
  let count = 0;
  const kinds = new Set<string>();
  let out = text;
  for (const { kind, re } of SECRET_PATTERNS) {
    out = out.replace(re, (match, prefix?: unknown) => {
      count++;
      kinds.add(kind);
      return typeof prefix === "string" && /password/i.test(prefix) ? `${prefix}[redacted]` : "[redacted]";
    });
  }
  out = out.replace(CARD, (match) => {
    const digits = match.replace(/\D/g, "");
    if (digits.length < 13 || !luhn(digits)) return match;
    count++;
    kinds.add("card number");
    return "[redacted]";
  });
  return { text: out, count, kinds: [...kinds] };
}

// ---------- author ----------

export function resolveAuthor(): string {
  const cfg = readJson<{ author?: unknown }>(configFile());
  if (cfg && typeof cfg.author === "string" && cfg.author.trim()) return cfg.author.trim();
  if (process.platform === "darwin") {
    try {
      const full = execFileSync("id", ["-F"], { encoding: "utf8", timeout: 2000, stdio: ["ignore", "pipe", "ignore"] }).trim();
      if (full) return full;
    } catch {
      // fall through
    }
  }
  try {
    return os.userInfo().username || "unknown";
  } catch {
    return "unknown";
  }
}

// ---------- save / list / get ----------

/** A capsule the checks refused: not a failure — Claude gets the fix list and saves again. */
export class NotSavedYet extends Error {
  constructor(
    message: string,
    readonly problems: string[],
  ) {
    super(message);
  }
}

/** One line in the app's diagnostics log (app.log): kinds of problems only, never capsule text. */
export function logLine(message: string): void {
  try {
    fs.appendFileSync(path.join(path.dirname(capsulesDir()), "app.log"), `${new Date().toISOString().replace(/\.\d{3}Z$/, "Z")} mcp: ${message}\n`);
  } catch {
    // the log is best effort
  }
}

/** Refusals per capsule being written, so a stuck Claude ends with a draft instead of looping forever. */
const attempts = new Map<string, number>();
export const MAX_TRIES = 3;

export interface SaveResult {
  path: string;
  slug: string;
  front: FrontMatter;
  warnings: string[];
}

export function saveCapsule(input: {
  title: string;
  markdown: string;
  session_ids?: string[];
  project?: string;
  kind?: CapsuleKind;
  date?: string;
  /** Never save as a draft: every refusal is reported (the pasted-reply `save` command; nobody retries there). */
  noDraft?: boolean;
}): SaveResult {
  const title = input.title.replace(/\s+/g, " ").trim();
  if (!title) throw new Error("Give the capsule a title.");
  const warnings: string[] = [];
  const project = input.project?.trim() || undefined;
  if (input.kind && !project) throw new Error(`A "${input.kind}" capsule needs a project.`);
  if (project && !loadProjects().some((p) => p.id === project))
    warnings.push(`"${project}" is not in projects.json yet (create_project adds it).`);
  const kind = project ? (input.kind ?? "daily") : undefined;
  const date = input.date?.trim() || undefined;
  if (date && !/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new Error(`"${date}" is not a date like 2026-10-19.`);
  if (kind === "daily" && !date) throw new Error("A daily capsule needs its date (YYYY-MM-DD).");

  // the AI may have added its own front matter; ours wins
  let body = parseCapsule(input.markdown).body.trim();
  if (!body) throw new Error("The capsule markdown is empty.");

  // What it was written from: notes and screen samples of the given sessions, to check its citations.
  const notes: { kind: "voice" | "note"; t: string }[] = [];
  const screen: { app: string; t: string }[] = [];
  for (const raw of input.session_ids ?? []) {
    try {
      for (const e of readEvents(resolveSessionId(raw))) {
        if (e.kind === "voice" || e.kind === "note") notes.push({ kind: e.kind, t: e.t });
        else screen.push({ app: e.app, t: e.t });
      }
    } catch {
      // unknown sessions are reported below
    }
  }

  // Every capsule another AI loads must be complete and true to its sources: refuse with numbered fixes,
  // so Claude fixes and retries. After MAX_TRIES refusals it is saved as a draft, with what's left to fix.
  const check: CheckResult = checkCapsule(body);
  if (notes.length || screen.length) {
    const c = checkCitations(body, { notes, screen });
    check.errors.push(...c.errors);
    check.warnings.push(...c.warnings);
  }
  const attemptKey = `${capsulesDir()}|${input.kind ?? ""}|${input.project ?? ""}|${input.date ?? ""}|${slugify(title)}`;
  let draft = false;
  if (check.errors.length) {
    const tries = (attempts.get(attemptKey) ?? 0) + 1;
    attempts.set(attemptKey, tries);
    if (tries < MAX_TRIES || input.noDraft) {
      if (input.noDraft) attempts.delete(attemptKey);
      logLine(`capsule not saved yet (try ${tries}): ${check.errors.map((e) => e.split(/[:.]/)[0]!.slice(0, 60)).join("; ")}`);
      throw new NotSavedYet(
        `Not saved yet (try ${tries} of ${MAX_TRIES}). Fix these, then call save_capsule again with the whole capsule. ` +
          `Change only what this list names; keep everything else as written:\n` +
          check.errors.map((e, i) => `${i + 1}. ${e}`).join("\n"),
        check.errors,
      );
    }
    draft = true;
    warnings.push(`Saved as a DRAFT after ${MAX_TRIES} tries. Still to fix: ${check.errors.join(" ")}`);
  }
  attempts.delete(attemptKey);
  warnings.push(...check.warnings);

  const red = redactSecrets(body);
  if (red.count) {
    body = red.text;
    warnings.push(`Removed ${red.count} private thing(s) before saving: ${red.kinds.join(", ")}.`);
  }

  const sessions: string[] = [];
  const apps = new Set<string>();
  for (const raw of input.session_ids ?? []) {
    try {
      const id = resolveSessionId(raw);
      if (!sessions.includes(id)) sessions.push(id);
      for (const a of appTimes(readEvents(id))) apps.add(a.app);
    } catch (e) {
      warnings.push(`Session "${raw}" skipped: ${(e as Error).message}`);
    }
  }

  const front: FrontMatter = {
    capsule: 1,
    title,
    author: resolveAuthor(),
    created: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
    sessions,
    apps: [...apps].sort((a, b) => a.localeCompare(b)),
    ...(project ? { project, kind: kind! } : {}),
    ...(date ? { date } : {}),
    playbook: PLAYBOOK_VERSION,
    ...(draft ? { draft: true } : {}),
  };
  const text = `${stringifyFrontMatter(front)}\n${body}\n`;

  if (kind === "project") {
    // One growing capsule per project: replace it, keeping the previous version in .history/.
    const file = path.join(capsulesDir(), `${project}${CAPSULE_EXT}`);
    if (fs.existsSync(file)) {
      const stamp = new Date().toISOString().replace(/[:.]/g, "-").replace(/-\d{3}Z$/, "Z");
      const old = path.join(capsulesDir(), ".history", `${project}-${stamp}${CAPSULE_EXT}`);
      fs.mkdirSync(path.dirname(old), { recursive: true });
      fs.copyFileSync(file, old);
      warnings.push(`Updated the project capsule; the previous version is in ${old}.`);
    }
    writeFileAtomic(file, text);
    return { path: file, slug: slugOf(file), front, warnings };
  }

  const slug = kind === "daily" ? `${project}-${date}` : slugify(title);
  const file = writeUnique(capsulesDir(), slug, CAPSULE_EXT, text);
  if (path.basename(file) !== `${slug}${CAPSULE_EXT}`)
    warnings.push(`A capsule named "${slug}" already existed, so this one was saved as ${path.basename(file)}.`);
  return { path: file, slug: slugOf(file), front, warnings };
}

export function slugOf(file: string): string {
  const base = path.basename(file);
  return base.endsWith(CAPSULE_EXT) ? base.slice(0, -CAPSULE_EXT.length) : base.replace(/\.md$/, "");
}

export function listCapsules(): CapsuleInfo[] {
  const dir = capsulesDir();
  if (!fs.existsSync(dir)) return [];
  const out: CapsuleInfo[] = [];
  for (const name of fs.readdirSync(dir)) {
    if (!name.endsWith(CAPSULE_EXT)) continue;
    const file = path.join(dir, name);
    let front: FrontMatter | null = null;
    try {
      front = parseCapsule(fs.readFileSync(file, "utf8")).front;
    } catch {
      continue;
    }
    out.push({
      slug: slugOf(file),
      title: typeof front?.title === "string" ? front.title : slugOf(file),
      author: typeof front?.author === "string" ? front.author : "",
      created: typeof front?.created === "string" ? front.created : "",
      path: file,
      ...(typeof front?.project === "string" ? { project: front.project } : {}),
      ...(typeof front?.kind === "string" ? { kind: front.kind } : {}),
      ...(typeof front?.date === "string" ? { date: front.date } : {}),
      ...(front?.draft === true ? { draft: true } : {}),
    });
  }
  return out.sort((a, b) => b.created.localeCompare(a.created) || a.slug.localeCompare(b.slug));
}

/** Find a capsule by slug, file name, path, or title. */
export function findCapsule(name: string): CapsuleInfo {
  const wanted = name.trim();
  const all = listCapsules();
  const bySlug = (s: string) => all.find((c) => c.slug === s);
  const base = path.basename(wanted);
  const hit =
    bySlug(wanted) ??
    bySlug(slugOf(base)) ??
    all.find((c) => c.title.toLowerCase() === wanted.toLowerCase()) ??
    bySlug(slugify(wanted));
  if (hit) return hit;
  const names = all.slice(0, 10).map((c) => c.slug);
  throw new Error(
    `No capsule called "${wanted}".` +
      (names.length ? ` Saved capsules: ${names.join(", ")}.` : " There are no saved capsules yet; write one with save_capsule."),
  );
}

export function readCapsule(name: string): { info: CapsuleInfo; text: string } {
  const info = findCapsule(name);
  return { info, text: fs.readFileSync(info.path, "utf8") };
}

// ---------- import / export ----------

/**
 * The path as the user (or the app's "continue from the capsule at <path>." message) gave it: quotes, backticks,
 * <angle brackets> and file:// URLs removed; a sentence's trailing "." dropped when the file has none.
 */
export function cleanCapsulePath(from: string): string {
  let p = from.trim().replace(/^[<`"'“‘]+|[>`"'”’]+$/g, "").trim();
  if (/^file:\/\//i.test(p)) {
    try {
      p = fileURLToPath(p); // file:///C:/… on Windows, file:///Users/… elsewhere
    } catch {
      p = p.replace(/^file:\/\//i, "");
    }
  }
  const resolved = path.resolve(expandHome(p));
  if (!fs.existsSync(resolved) && /[.,;:]+$/.test(p)) {
    const bare = path.resolve(expandHome(p.replace(/[.,;:]+$/, "")));
    if (fs.existsSync(bare)) return bare;
  }
  return resolved;
}

// ---------- the note on top of a shared copy (SPEC "Sharing a capsule"; the same words in ui/js/core/capsules.js) ----------

/** How the note starts: also how it is found again, to take it off. */
export const SHARE_NOTE_START = "> **This is a Context Capsule";

/** Two lines for whoever gets the file, with or without Context Capsule: what it is, and what to do with it. */
export function shareNote(title: string, author?: string): string {
  const who = author ? `, from ${author}` : "";
  return (
    `${SHARE_NOTE_START}: ${title}**${who}. It holds the context of this work, written for an AI.\n` +
    `> **To carry on with it:** give this file to your AI (ChatGPT, Claude, Gemini or another) and say ` +
    `"Continue from this capsule." **To read it yourself:** start at In short.`
  );
}

/** The capsule without the note (as it was before it was shared). */
export function withoutShareNote(text: string): string {
  const t = text.replace(/\r\n/g, "\n");
  const lines = t.split("\n");
  const at = lines.findIndex((l) => l.startsWith(SHARE_NOTE_START));
  // only above the first section: a capsule may quote the words anywhere else
  const first = lines.findIndex((l) => /^##\s/.test(l));
  if (at < 0 || (first >= 0 && at > first)) return t;
  let end = at;
  while (end < lines.length && lines[end]!.startsWith(">")) end++;
  if (end < lines.length && !lines[end]!.trim()) end++;
  return [...lines.slice(0, at), ...lines.slice(end)].join("\n");
}

/** The copy that is shared: the note right after the front matter (once, however often it is shared). */
export function withShareNote(text: string): string {
  const t = withoutShareNote(text);
  const { front } = parseCapsule(t);
  const note = shareNote(typeof front?.title === "string" ? front.title : "", typeof front?.author === "string" ? front.author : "");
  const end = t.startsWith("---\n") ? t.indexOf("\n---", 4) : -1;
  if (end < 0) return `${note}\n\n${t}`;
  const after = t.indexOf("\n", end + 4);
  const head = after < 0 ? `${t}\n` : t.slice(0, after + 1);
  const body = after < 0 ? "" : t.slice(after + 1).replace(/^\n+/, "");
  return `${head}\n${note}\n\n${body}`;
}

/** Copy a received capsule in. Any file name works (.capsule.md, .md, .txt…) as long as the text is a capsule. A
 * shared copy's note on top is taken off: it was for people without Context Capsule. */
export function importCapsule(from: string): { path: string; slug: string; title: string; already: boolean } {
  const src = cleanCapsulePath(from);
  if (!fs.existsSync(src)) throw new Error(`No file at ${src}. Check the path (drag the file into the chat to get it).`);
  if (!fs.statSync(src).isFile()) throw new Error(`${src} is a folder, not a capsule file.`);
  const raw = fs.readFileSync(src, "utf8");
  const text = raw.includes(SHARE_NOTE_START) ? withoutShareNote(raw) : raw;
  const v = validateCapsuleText(text);
  if (!v.ok)
    throw new Error(
      `${path.basename(src)} isn't a Context Capsule, so nothing was imported. ${v.error} ` +
        `Tell the user in one line, and ask them for the capsule file the sender exported (it starts with a --- block with capsule: 1 and a title).`,
    );
  const title = String(v.front.title);

  // same content already imported? don't make a copy
  for (const c of listCapsules()) {
    if (path.resolve(c.path) === src) return { path: c.path, slug: c.slug, title, already: true };
    try {
      if (fs.readFileSync(c.path, "utf8") === text) return { path: c.path, slug: c.slug, title, already: true };
    } catch {
      // ignore
    }
  }
  const fromName = path.basename(src).endsWith(CAPSULE_EXT) ? slugify(slugOf(src)) : slugify(title);
  const file = writeUnique(capsulesDir(), fromName, CAPSULE_EXT, text);
  return { path: file, slug: slugOf(file), title, already: false };
}

export function defaultExportDir(): string {
  const desktop = path.join(homeDir(), "Desktop");
  return fs.existsSync(desktop) ? desktop : homeDir();
}

/**
 * Where an export goes: a folder (Desktop by default) gets `<base><ext>`; a file path is used as given (`ext` when
 * it has none). Never overwrites: a taken name gets -2, -3….
 */
export function writeExport(toPath: string | undefined, base: string, ext: string, data: string): string {
  const target = toPath?.trim() ? path.resolve(expandHome(toPath.trim())) : defaultExportDir();
  const isDir = (fs.existsSync(target) && fs.statSync(target).isDirectory()) || /[/\\]$/.test(toPath ?? "") || !toPath?.trim();
  let dir = target;
  if (!isDir) {
    dir = path.dirname(target);
    const b = path.basename(target);
    const given = b.endsWith(CAPSULE_EXT) ? CAPSULE_EXT : path.extname(b);
    base = given ? b.slice(0, -given.length) : b;
    if (given) ext = given;
  }
  fs.mkdirSync(dir, { recursive: true });
  return writeUnique(dir, base, ext, data);
}

/** The file to send: the capsule with the note on top, for whoever gets it. */
export function exportCapsule(name: string, toPath?: string): string {
  const { info, text } = readCapsule(name);
  return writeExport(toPath, info.slug, CAPSULE_EXT, withShareNote(text));
}
