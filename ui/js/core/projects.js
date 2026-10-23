// context-capsule — ui: projects — projects.json, a window's project from its clues, and the edits the
// Projects page makes. A port of `Projects` (capture/Sources/Core.swift) and core/src/projects.rs.

import { graphemes, trimSpaces } from "./text.js";

/** "Client Report (Q3)!" -> "client-report-q3" (ASCII letters and digits, at most 60, never ending on a dash). */
export function slug(name) {
  const folded = String(name).normalize("NFKD").replace(/\p{M}/gu, "").toLowerCase();
  let out = "";
  let dash = false;
  for (const c of folded) {
    if (/^[a-z0-9]$/.test(c)) {
      out += c;
      dash = false;
    } else if (!dash && out) {
      out += "-";
      dash = true;
    }
  }
  return out.replace(/-+$/, "").slice(0, 60).replace(/-+$/, "");
}

/** projects.json text -> [{id, name, match}]; a missing or broken file means no projects. */
export function parse(text) {
  let obj;
  try {
    obj = JSON.parse(text);
  } catch {
    return [];
  }
  const list = obj && typeof obj === "object" ? obj.projects : null;
  // Like Swift's `as? [[String: Any]]`: one entry that isn't an object spoils the whole list.
  if (!Array.isArray(list) || !list.every((p) => p && typeof p === "object" && !Array.isArray(p))) return [];
  const out = [];
  for (const p of list) {
    if (typeof p.name !== "string") continue;
    const name = trimSpaces(p.name);
    if (!name) continue;
    const id = typeof p.id === "string" && p.id ? p.id : slug(name);
    // Like `as? [String]`: a list with anything but strings counts as no clues.
    const m = Array.isArray(p.match) && p.match.every((c) => typeof c === "string") ? p.match : [];
    out.push({ id, name, match: m.map(trimSpaces).filter((c) => c) });
  }
  return out;
}

/** The project whose clue is in the title, where or app name (any case). Longest clue wins, then list order. */
export function detect(projects, { title = "", where = "", app = "" } = {}) {
  const hay = [title, where, app].map((s) => String(s).toLowerCase());
  let best = null;
  let length = 0;
  for (const p of projects || []) {
    for (const clue of p.match || []) {
      const c = clue.toLowerCase();
      const n = graphemes(c).length;
      if (n < 2 || !hay.some((h) => h.includes(c))) continue;
      if (!best || n > length) {
        best = p;
        length = n;
      }
    }
  }
  return best;
}

/** A project's name; its id when there's no such project. */
export function nameOf(projects, id) {
  return (projects || []).find((p) => p.id === id)?.name ?? id;
}

/** Renamed (trimmed; an empty name changes nothing). */
export function rename(projects, id, name) {
  const clean = trimSpaces(name);
  if (!clean) return projects;
  return projects.map((p) => (p.id === id ? { ...p, name: clean } : p));
}

/** "acme, q3 report , x" -> ["acme", "q3 report"]: comma-separated, trimmed, 2 letters or more. */
export function setClues(text) {
  return String(text)
    .split(",")
    .map(trimSpaces)
    .filter((c) => graphemes(c).length >= 2);
}

/** The list with that project's clues replaced. */
export function withClues(projects, id, text) {
  const match = setClues(text);
  return projects.map((p) => (p.id === id ? { ...p, match } : p));
}

/** The list without that project. */
export function remove(projects, id) {
  return projects.filter((p) => p.id !== id);
}

/** A new project (or a clue added to the one with that slug), like the recorder's createProject. null: no id. */
export function create(projects, name, clue = null) {
  const id = slug(name);
  if (!id) return null;
  const i = projects.findIndex((p) => p.id === id);
  if (i < 0) return [...projects, { id, name: trimSpaces(name), match: clue ? [clue] : [] }];
  const p = projects[i];
  if (!clue || p.match.some((c) => c.toLowerCase() === clue.toLowerCase())) return projects;
  return projects.map((q, j) => (j === i ? { ...q, match: [...q.match, clue] } : q));
}

/** A JSON string as Foundation writes it: `/` escaped, \b and \f short, other controls as \u00xx. */
function jsonString(s) {
  let out = '"';
  for (const c of String(s)) {
    const n = c.codePointAt(0);
    if (c === '"') out += '\\"';
    else if (c === "\\") out += "\\\\";
    else if (c === "/") out += "\\/";
    else if (c === "\n") out += "\\n";
    else if (c === "\r") out += "\\r";
    else if (c === "\t") out += "\\t";
    else if (c === "\b") out += "\\b";
    else if (c === "\f") out += "\\f";
    else if (n < 0x20) out += "\\u" + n.toString(16).padStart(4, "0");
    else out += c;
  }
  return out + '"';
}

/** projects.json as the Mac app writes it (Foundation's pretty printing, keys sorted), with its last newline. */
export function toJSON(projects) {
  let out = '{\n  "projects" : [';
  if (!projects.length) return out + "\n\n  ]\n}\n";
  projects.forEach((p, i) => {
    out += i === 0 ? "\n" : ",\n";
    out += `    {\n      "id" : ${jsonString(p.id)},\n      "match" : [`;
    const match = p.match || [];
    out += match.length ? "\n" + match.map((c) => "        " + jsonString(c)).join(",\n") + "\n      ]" : "\n\n      ]";
    out += `,\n      "name" : ${jsonString(p.name)}\n    }`;
  });
  return out + "\n  ]\n}\n";
}
