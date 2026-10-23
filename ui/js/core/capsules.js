// context-capsule — ui: saved capsules as the window shows them — front matter, "In short", groups, row words, prompts.
// A port of CapsuleSummary and ClaudeLink (capture/Sources/Core.swift), LibraryModel.groups and CapsuleRow
// (capture/Sources/Library.swift) and Home's recent capsules (MainWindow.swift).

/** Capsule files end with this. */
export const SUFFIX = ".capsule.md";

/** What "Make today's capsules" asks your AI. */
export const dayPrompt = "Use Context Capsule to make my capsules for today.";

/** Opening a received capsule. */
export function continuePrompt(path) {
  return `Use Context Capsule to continue from the capsule at ${path}.`;
}

/** Ask your capsules (the Capsules page). */
export function searchPrompt(question) {
  return `Use Context Capsule to search my capsules: ${String(question).trim()}`;
}

const spaces = (s) => s.replace(/^[ \t]+|[ \t]+$/g, "");

/** Lower case, accents removed: what the search box compares. */
function plain(s) {
  return s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** The file name without its last two extensions ("a.capsule.md" -> "a"). */
function fallbackTitle(name) {
  let file = String(name || "").split(/[\\/]/).pop();
  for (let i = 0; i < 2; i++) {
    const dot = file.lastIndexOf(".");
    if (dot > 0) file = file.slice(0, dot);
  }
  return file;
}

/** Reads `---` front matter (simple `key: value` lines) and the lines under `## In short`. */
export function parse(text, name, path) {
  // Saved on Windows: "\r\n" line ends.
  text = String(text).replace(/\r\n/g, "\n");
  const front = {};
  let body = text;
  const end = text.startsWith("---\n") ? text.indexOf("\n---", 4) : -1;
  if (end >= 0) {
    for (const line of text.slice(4, end).split("\n")) {
      const colon = line.indexOf(":");
      if (colon < 0) continue;
      const key = spaces(line.slice(0, colon));
      let value = spaces(line.slice(colon + 1));
      const hash = value.indexOf(" #");
      if (hash >= 0) value = spaces(value.slice(0, hash));
      const q = value[0];
      if (value.length >= 2 && (q === '"' || q === "'") && value.endsWith(q)) value = value.slice(1, -1);
      front[key] = value; // the last one wins
    }
    body = text.slice(end + 4);
  }
  let inShort = "";
  const start = body.toLowerCase().indexOf("## in short");
  if (start >= 0) {
    const rest = body.slice(start + "## In short".length);
    const stop = rest.indexOf("\n## ");
    const section = stop >= 0 ? rest.slice(0, stop) : rest;
    inShort = section.split("\n").map(spaces).filter((l) => l).join(" ");
  }
  return {
    name: String(name || ""),
    path: path ?? String(name || ""),
    title: front.title || fallbackTitle(name),
    project: front.project ?? "",
    kind: front.kind ?? "", // "daily", "project" (the growing one) or "" (a one-off)
    date: front.date ?? "",
    created: front.created ?? "",
    author: front.author ?? "",
    inShort,
    draft: front.draft === "true",
  };
}

/** Every `*.capsule.md` in the list ([{name, text, path?}]), newest first. */
export function all(list) {
  return (list || [])
    .filter((f) => String(f.name).endsWith(SUFFIX) && typeof f.text === "string")
    .map((f) => parse(f.text, f.name, f.path))
    .sort((a, b) => cmp(b.created, a.created) || cmp(b.title, a.title));
}

const cmp = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

/** A capsule file: front matter that has `capsule:` and `title:`. */
export function isCapsule(text) {
  let t = String(text);
  if (t.startsWith("﻿")) t = t.slice(1);
  const lines = t.replace(/\r\n/g, "\n").split("\n");
  if (spaces(lines[0]) !== "---") return false;
  const keys = new Set();
  for (const line of lines.slice(1)) {
    if (spaces(line) === "---") return keys.has("capsule") && keys.has("title");
    const colon = line.indexOf(":");
    if (colon >= 0) keys.add(spaces(line.slice(0, colon)));
  }
  return false;
}

/** The search box: every word typed is in the title or "In short" (any case or accent). Empty: all. */
export function matches(c, query) {
  const words = String(query || "").split(/\s+/).filter((w) => w);
  const hay = plain(`${c.title} ${c.inShort}`);
  return words.every((w) => hay.includes(plain(w)));
}

/** Projects with capsules by name, then "Other"; in each the growing capsule first, then days, newest first. */
export function groups(capsules, query, nameOf = (id) => id) {
  const shown = capsules.filter((c) => matches(c, query));
  const order = [];
  for (const c of shown) if (!order.includes(c.project)) order.push(c.project);
  order.sort((a, b) => {
    if (!a || !b) return (a ? 0 : 1) - (b ? 0 : 1);
    return nameOf(a).localeCompare(nameOf(b), undefined, { sensitivity: "accent" });
  });
  return order.map((p) => ({
    title: p ? nameOf(p) : "Other",
    items: shown
      .filter((c) => c.project === p)
      .sort((a, b) => {
        const pa = a.kind === "project", pb = b.kind === "project";
        if (pa !== pb) return pa ? -1 : 1;
        return cmp(b.date, a.date) || cmp(b.created, a.created);
      }),
  }));
}

/** "Today", "Yesterday", else "19 Oct" (dates.shortDay); a text that isn't a date as it is. */
function day(ymd, dates, now) {
  return /^\d{4}-\d{2}-\d{2}$/.test(ymd) ? dates.shortDay(ymd, now) : ymd;
}

/** A row's name: "Whole project" for the growing capsule, the day for daily ones, else the title. */
export function label(c, dates, now) {
  if (c.kind === "project") return "Whole project";
  if (c.kind === "daily" && c.date) return day(c.date, dates, now);
  return c.title;
}

/** The day on the right, for capsules not already named by their day ("" for none). */
export function when(c, dates, now) {
  if (c.kind === "daily" || c.created.length < 10) return "";
  return day(c.created.slice(0, 10), dates, now);
}

/** Home's "Recent capsules": the newest few. */
export function recent(capsules, n = 3) {
  return capsules.slice(0, n);
}

/** Capsules made since the week began (not the growing project capsule, rewritten every day). */
export function weekCount(capsules, weekStartMs) {
  return capsules.filter((c) => {
    if (c.kind === "project" || !/^\d{4}-\d{2}-\d{2}T/.test(c.created)) return false;
    const t = Date.parse(c.created);
    return !Number.isNaN(t) && t >= weekStartMs;
  }).length;
}
