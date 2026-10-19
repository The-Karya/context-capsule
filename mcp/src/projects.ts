// context-capsule — mcp: projects (projects.json), finding a sample's project, and corrections (assign.json)
import fs from "node:fs";
import path from "node:path";
import { projectsFile, sessionsDir } from "./paths.js";
import { readJson, writeFileAtomic } from "./fsutil.js";

export interface Project {
  id: string;
  name: string;
  /** Clues found in the window title, page address / file path, or app name (case-insensitive). */
  match: string[];
}

/** "Client Report (Q3)!" → "client-report-q3" (the same rule as the recorders). */
export function projectSlug(name: string): string {
  return name
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60)
    .replace(/-+$/g, "");
}

export function loadProjects(): Project[] {
  const raw = readJson<{ projects?: unknown }>(projectsFile());
  const list = Array.isArray(raw?.projects) ? (raw!.projects as Record<string, unknown>[]) : [];
  const out: Project[] = [];
  for (const p of list) {
    const name = typeof p?.name === "string" ? p.name.trim() : "";
    if (!name) continue;
    const id = typeof p.id === "string" && p.id ? p.id : projectSlug(name);
    const match = Array.isArray(p.match)
      ? p.match.filter((m): m is string => typeof m === "string").map((m) => m.trim()).filter(Boolean)
      : [];
    if (!out.some((o) => o.id === id)) out.push({ id, name, match });
  }
  return out;
}

export function saveProjects(projects: Project[]): void {
  const sorted = projects.map((p) => ({ id: p.id, match: p.match, name: p.name }));
  writeFileAtomic(projectsFile(), JSON.stringify({ projects: sorted }, null, 2) + "\n");
}

/** Add a project, or add clues to an existing one with the same id. Returns it and whether it is new. */
export function createProject(name: string, match: string[] = []): { project: Project; created: boolean } {
  const clean = name.replace(/\s+/g, " ").trim();
  const id = projectSlug(clean);
  if (!id) throw new Error("Give the project a name with letters or numbers.");
  const clues = match.map((m) => m.trim()).filter((m) => m.length >= 2);
  const projects = loadProjects();
  let project = projects.find((p) => p.id === id);
  const created = !project;
  if (!project) {
    project = { id, name: clean, match: [] };
    projects.push(project);
  }
  for (const c of clues) if (!project.match.some((m) => m.toLowerCase() === c.toLowerCase())) project.match.push(c);
  saveProjects(projects);
  return { project, created };
}

/** The project whose clue is in the title, where or app (longest clue wins a tie, then list order). */
export function detectProject(projects: Project[], title: string, where: string, app: string): Project | null {
  const hay = [title, where, app].map((s) => s.toLowerCase());
  let best: { p: Project; len: number } | null = null;
  for (const p of projects)
    for (const clue of p.match) {
      const c = clue.toLowerCase();
      if (c.length < 2 || !hay.some((h) => h.includes(c))) continue;
      if (!best || c.length > best.len) best = { p, len: c.length };
    }
  return best?.p ?? null;
}

// ---------- corrections: sessions/<id>/assign.json ----------

export interface Assignment {
  from?: string;
  to?: string;
  project: string;
}

export function assignFile(sessionId: string): string {
  return path.join(sessionsDir(), sessionId, "assign.json");
}

export function readAssignments(sessionId: string): Assignment[] {
  const raw = readJson<{ assign?: unknown }>(assignFile(sessionId));
  const list = Array.isArray(raw?.assign) ? (raw!.assign as Record<string, unknown>[]) : [];
  return list
    .filter((a) => typeof a?.project === "string")
    .map((a) => ({
      project: a.project as string,
      ...(typeof a.from === "string" ? { from: a.from } : {}),
      ...(typeof a.to === "string" ? { to: a.to } : {}),
    }));
}

export function addAssignment(sessionId: string, a: Assignment): void {
  if (!fs.existsSync(path.join(sessionsDir(), sessionId))) throw new Error(`No session "${sessionId}".`);
  const list = readAssignments(sessionId);
  list.push(a);
  writeFileAtomic(assignFile(sessionId), JSON.stringify({ assign: list }, null, 2) + "\n");
}

/** The correction that covers time `t` (later entries win), if any. "" means "no project". */
export function assignedAt(list: Assignment[], t: string): string | undefined {
  const ms = Date.parse(t);
  let hit: string | undefined;
  for (const a of list) {
    const from = a.from ? Date.parse(a.from) : -Infinity;
    const to = a.to ? Date.parse(a.to) : Infinity;
    if (ms >= from && ms < to) hit = a.project;
  }
  return hit;
}
