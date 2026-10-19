// context-capsule — mcp: capsule files (front matter, save, list, get, import, export)
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { capsulesDir, configFile, expandHome, homeDir } from "./paths.js";
import { readJson, writeUnique } from "./fsutil.js";
import { appTimes, readEvents, resolveSessionId } from "./sessions.js";

export const CAPSULE_EXT = ".capsule.md";
export const REQUIRED_SECTIONS = ["Summary", "What I worked on", "Decisions", "Open questions", "Next steps", "Sources"];

export type FrontValue = string | number | boolean | string[];
export type FrontMatter = Record<string, FrontValue>;

export interface CapsuleInfo {
  slug: string;
  title: string;
  author: string;
  created: string;
  path: string;
}

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

const SECRET_PATTERNS: RegExp[] = [
  /\bsk-[A-Za-z0-9_-]{16,}/g,
  /\bghp_[A-Za-z0-9]{20,}/g,
  /\bgithub_pat_[A-Za-z0-9_]{20,}/g,
  /\bxox[bpas]-[A-Za-z0-9-]{10,}/g,
  /\bAKIA[0-9A-Z]{16}\b/g,
  /^(\s*[-*]?\s*password\s*[:=]\s*).+$/gim,
];

export function redactSecrets(text: string): { text: string; count: number } {
  let count = 0;
  let out = text;
  for (const re of SECRET_PATTERNS) {
    out = out.replace(re, (match, prefix?: unknown) => {
      count++;
      return typeof prefix === "string" && /password/i.test(prefix) ? `${prefix}[redacted]` : "[redacted]";
    });
  }
  return { text: out, count };
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

export interface SaveResult {
  path: string;
  slug: string;
  front: FrontMatter;
  warnings: string[];
}

export function saveCapsule(input: { title: string; markdown: string; session_ids?: string[] }): SaveResult {
  const title = input.title.replace(/\s+/g, " ").trim();
  if (!title) throw new Error("Give the capsule a title.");
  const warnings: string[] = [];

  // the AI may have added its own front matter; ours wins
  let body = parseCapsule(input.markdown).body.trim();
  if (!body) throw new Error("The capsule markdown is empty.");

  const missing = missingSections(body);
  if (missing.length) warnings.push(`Missing sections: ${missing.map((s) => `## ${s}`).join(", ")}. Saved anyway.`);

  const red = redactSecrets(body);
  if (red.count) {
    body = red.text;
    warnings.push(`Redacted ${red.count} thing(s) that looked like a secret (API key, token or password).`);
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
  };
  const slug = slugify(title);
  const file = writeUnique(capsulesDir(), slug, CAPSULE_EXT, `${stringifyFrontMatter(front)}\n${body}\n`);
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

export function importCapsule(from: string): { path: string; slug: string; title: string; already: boolean } {
  const src = path.resolve(expandHome(from.trim()));
  if (!fs.existsSync(src)) throw new Error(`No file at ${src}. Check the path (drag the file into the chat to get it).`);
  if (!fs.statSync(src).isFile()) throw new Error(`${src} is a folder, not a capsule file.`);
  const text = fs.readFileSync(src, "utf8");
  const v = validateCapsuleText(text);
  if (!v.ok) throw new Error(`Can't import ${path.basename(src)}: ${v.error}`);
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

export function exportCapsule(name: string, toPath?: string): string {
  const { info, text } = readCapsule(name);
  let dir: string;
  let base: string;
  let ext: string;
  const target = toPath?.trim() ? path.resolve(expandHome(toPath.trim())) : defaultExportDir();
  const isDir = (fs.existsSync(target) && fs.statSync(target).isDirectory()) || /[/\\]$/.test(toPath ?? "") || !toPath?.trim();
  if (isDir) {
    dir = target;
    base = info.slug;
    ext = CAPSULE_EXT;
  } else {
    dir = path.dirname(target);
    const b = path.basename(target);
    ext = b.endsWith(CAPSULE_EXT) ? CAPSULE_EXT : path.extname(b);
    base = ext ? b.slice(0, -ext.length) : b;
    if (!ext) ext = CAPSULE_EXT;
  }
  fs.mkdirSync(dir, { recursive: true });
  return writeUnique(dir, base, ext, text);
}
