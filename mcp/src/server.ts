// context-capsule — mcp: the MCP server (tools, the make_capsule prompt, capsule:// resources)
import fs from "node:fs";
import { McpServer, ResourceTemplate } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { z } from "zod";
import { DEFAULT_MAX_CHARS, condenseActivity, condenseDay, localDay } from "./activity.js";
import { addAssignment, createProject, loadProjects } from "./projects.js";
import { checkDay, finishDay, loadDay, prepareDay, today } from "./day.js";
import { SERVER_INSTRUCTIONS, continueFromCapsulePrompt, makeCapsulePrompt, makeDayCapsulesPrompt } from "./playbook.js";
import {
  CAPSULE_EXT,
  REQUIRED_SECTIONS,
  exportCapsule,
  findCapsule,
  importCapsule,
  listCapsules,
  readCapsule,
  saveCapsule,
} from "./capsules.js";
import {
  appNotRunning,
  pickerOpen,
  appRunning,
  describeSkip,
  describeStartError,
  readState,
  startAndWait,
  waitForState,
  writeControl,
} from "./control.js";
import { words } from "./platform.js";
import { capsuleHome } from "./paths.js";
import {
  formatClock,
  formatDuration,
  formatLocalDateTime,
  listSessionIds,
  projectTimes,
  noSessionsMessage,
  readEvents,
  readSessionMeta,
  resolveSessionId,
  summarizeSession,
} from "./sessions.js";

export const VERSION = "0.1.0";

export interface ServerOptions {
  /** How long start/stop wait for the app to answer via state.json (default: start 6 s, stop 4 s). */
  confirmTimeoutMs?: number;
}

const text = (t: string): CallToolResult => ({ content: [{ type: "text", text: t }] });
const fail = (t: string): CallToolResult => ({ content: [{ type: "text", text: t }], isError: true });

/** Turn thrown errors into a friendly tool error instead of a protocol error. */
function safe<A>(fn: (args: A) => CallToolResult | Promise<CallToolResult>) {
  return async (args: A): Promise<CallToolResult> => {
    try {
      return await fn(args);
    } catch (e) {
      return fail(e instanceof Error ? e.message : String(e));
    }
  };
}

export function createServer(opts: ServerOptions = {}): McpServer {
  const startMs = opts.confirmTimeoutMs ?? 6000;
  const stopMs = opts.confirmTimeoutMs ?? 4000;
  const noteMs = opts.confirmTimeoutMs ?? 3000;
  const server = new McpServer(
    { name: "context-capsule", version: VERSION },
    {
      instructions: SERVER_INSTRUCTIONS,
    },
  );

  // ---------- recording ----------

  server.registerTool(
    "start_recording",
    {
      title: "Start recording",
      description:
        "Ask the Context Capsule app to start recording. The app opens the system picker; the user chooses the window " +
        "or screen to record, and recording starts after that (screen text every 3 s, nothing leaves the Mac). " +
        "Returns the session id once recording, or says the picker is waiting for the user.",
      inputSchema: {},
    },
    safe(async () => {
      const state = readState();
      if (!appRunning(state)) return fail(appNotRunning());
      if (state!.recording)
        return text(`Already recording session ${state!.session_id ?? "(unknown)"}${since(state!.started_at)}. Call stop_recording when done.`);
      const out = await startAndWait(startMs);
      switch (out.kind) {
        case "recording":
          return text(
            `Recording started: session ${out.state.session_id ?? "(unknown)"}${since(out.state.started_at)}.\n` +
              `Work as usual. Call stop_recording when you're done (or press ${words().recordKeyShort}).`,
          );
        case "picking":
          return text(`${pickerOpen()}\nCall recording_status to check, and stop_recording when you're done.`);
        case "error":
          return fail(describeStartError(out.error));
        case "timeout":
          return text(
            `Sent the start command, but the Context Capsule app hasn't answered yet. Look for ${words().picker} or the ` +
              `icon in the ${words().tray} (a filled circle means recording), or call recording_status in a moment.`,
          );
      }
    }),
  );

  server.registerTool(
    "stop_recording",
    {
      title: "Stop recording",
      description: "Ask the Context Capsule app to stop the current recording. Returns the session id to pass to get_session_activity.",
      inputSchema: {},
    },
    safe(async () => {
      const state = readState();
      if (!appRunning(state)) {
        return fail(
          "The Context Capsule app is not running, so nothing is being recorded." +
            latestHint() +
            `\nTo record again, open the app (${words().openApp}) and call start_recording.`,
        );
      }
      if (!state!.recording) return text(`Not recording right now.${latestHint()}`);
      const id = state!.session_id;
      writeControl("stop");
      const { confirmed, state: after } = await waitForState((s) => !s.recording, stopMs);
      const next = id
        ? `\nNext: get_session_activity with session_id "${id}" (or use the make_capsule prompt) to write a capsule.`
        : "";
      if (!confirmed)
        return text(`Sent the stop command for session ${id ?? "(unknown)"}; the app hasn't confirmed yet.${next}`);
      if (after?.events === 0) {
        const why = after.skip ? ` Most of the time ${describeSkip(after.skip)}.` : "";
        return text(
          `Stopped session ${id ?? "(unknown)"}, but nothing was recorded.${why}\n` +
            "Context Capsule only reads the window in front, and only if it is part of what was picked. " +
            "Next time pick the screen you work on (or the window), and keep that window in front.",
        );
      }
      const count = after?.events != null ? ` ${after.events} event${after.events === 1 ? "" : "s"}.` : "";
      return text(`Stopped. Session id: ${id ?? "(unknown)"}.${count}${next}`);
    }),
  );

  server.registerTool(
    "recording_status",
    {
      title: "Recording status",
      description: "Is the Context Capsule app running, is the picker open, is it recording, and why the last start failed. Reads state.json.",
      inputSchema: {},
      annotations: { readOnlyHint: true },
    },
    safe(() => {
      const state = readState();
      if (!state)
        return text(`The Context Capsule app has not run yet on ${words().machine} (no ${capsuleHome()}/state.json).\n${appNotRunning()}`);
      const running = appRunning(state);
      const lines = [
        `App running: ${running ? "yes" : "no"}`,
        `Recording: ${running && state.recording ? "yes" : "no"}`,
      ];
      if (running && !state.recording && state.picking)
        lines.push("Picker: open — waiting for you to choose the window or screen to record");
      if (running && !state.recording && !state.picking && state.last_error)
        lines.push(`Last start: did not happen (${state.last_error === "cancelled" ? "picker closed without a choice" : state.last_error})`);
      if (running && state.recording) {
        lines.push(`Session: ${state.session_id ?? "(unknown)"}`);
        if (state.started_at)
          lines.push(`Started: ${formatLocalDateTime(state.started_at)} (${formatDuration(Date.now() - Date.parse(state.started_at))} ago)`);
        if (state.events != null) lines.push(`Events so far: ${state.events}`);
        if (state.skip) lines.push(`Right now: not recording a sample — ${describeSkip(state.skip)}`);
        if (state.listening) lines.push(`Voice note: listening (press ${words().voiceKeyShort} again to finish)`);
        if (state.project)
          lines.push(`Project: ${state.project} (${state.project_by === "you" ? "picked" : "detected from clues"})`);
        else lines.push("Project: not detected (no clue matched yet)");
      }
      if (!running) lines.push("", appNotRunning());
      return text(lines.join("\n"));
    }),
  );

  server.registerTool(
    "add_note",
    {
      title: "Add a note",
      description:
        "Add a typed note to the session being recorded right now: what the user is doing, how, or a decision and why. " +
        "Notes are the user's own words and become the main source for the capsule. Only works while recording.",
      inputSchema: { text: z.string().min(1).max(4000).describe("The note, in the user's words") },
    },
    safe(async ({ text: note }: { text: string }) => {
      const body = note.trim();
      if (!body) return fail("The note is empty.");
      const state = readState();
      if (!appRunning(state)) return fail(`Notes go into a recording, and the app isn't running.\n${appNotRunning()}`);
      if (!state!.recording)
        return fail(`Not recording right now, so there is no session to add the note to. Start one with start_recording (or ${words().recordKeyShort}), then add the note.`);
      const at = Date.now();
      writeControl("note", at, body);
      const { confirmed } = await waitForState((s) => s.note_at === at || !s.recording, noteMs);
      const after = readState();
      if (!confirmed || after?.note_at !== at)
        return text(`Sent the note to the app for session ${state!.session_id ?? "(unknown)"}, but it hasn't confirmed yet. Check recording_status.`);
      return text(`Added the note to session ${state!.session_id ?? "(unknown)"}.`);
    }),
  );

  // ---------- projects ----------

  server.registerTool(
    "list_projects",
    {
      title: "List projects",
      description: "Projects (from projects.json) with their clues, time recorded today and their capsules.",
      inputSchema: {},
      annotations: { readOnlyHint: true },
    },
    safe(() => {
      const projects = loadProjects();
      const times = new Map(projectTimes(loadDay(today()).events).map((t) => [t.project, t.ms]));
      const capsules = listCapsules();
      if (!projects.length)
        return text(
          "No projects yet. Create one with create_project (name + clues like a folder, website or file name), " +
            "or let me sort a day's work with the make_day_capsules prompt.",
        );
      const rows = projects.map((p) => {
        const caps = capsules.filter((c) => c.project === p.id);
        const whole = caps.find((c) => c.kind === "project");
        const days = caps.filter((c) => c.kind !== "project").length;
        return (
          `- ${p.name} (id ${p.id}) · today ${formatDuration(times.get(p.id) ?? 0)} · clues: ${p.match.join(", ") || "none"}` +
          ` · capsules: ${whole ? `project capsule "${whole.slug}"` : "no project capsule yet"}, ${days} daily`
        );
      });
      const loose = times.get("") ?? 0;
      return text(`Projects:\n${rows.join("\n")}${loose ? `\nNot sorted yet today: ${formatDuration(loose)}` : ""}`);
    }),
  );

  server.registerTool(
    "create_project",
    {
      title: "Create project",
      description:
        "Add a project. Clues (`match`) are words found in window titles, page addresses / file paths or app names " +
        "(e.g. a repo folder, a website, a document name); work matching them is sorted into this project automatically, " +
        "including past recordings. Calling it again with the same name adds clues.",
      inputSchema: {
        name: z.string().min(1).describe("Project name, e.g. 'Client report'"),
        match: z.array(z.string()).optional().describe("Clues, e.g. ['acme-site', 'figma.com/file/AbC']"),
      },
    },
    safe(({ name, match }: { name: string; match?: string[] }) => {
      const { project, created } = createProject(name, match ?? []);
      return text(
        `${created ? "Created" : "Updated"} project "${project.name}" (id ${project.id}). Clues: ${project.match.join(", ") || "none"}.`,
      );
    }),
  );

  server.registerTool(
    "set_project",
    {
      title: "Set current project",
      description:
        'Tell the recorder which project the user is working on now (every sample goes to it until changed), or "auto" to detect it again from clues.',
      inputSchema: { project: z.string().min(1).describe('Project id or name, or "auto"') },
    },
    safe(async ({ project }: { project: string }) => {
      const wanted = project.trim();
      let id = "auto";
      if (wanted.toLowerCase() !== "auto") {
        const p = loadProjects().find((x) => x.id === wanted || x.name.toLowerCase() === wanted.toLowerCase());
        if (!p) return fail(`No project "${wanted}". Create it first with create_project.`);
        id = p.id;
      }
      const state = readState();
      if (!appRunning(state)) return fail(`The recorder isn't running.\n${appNotRunning()}`);
      const at = Date.now();
      writeControl("project", at, undefined, id);
      const { confirmed } = await waitForState((s) => (id === "auto" ? s.project_by !== "you" : s.project === id), noteMs);
      if (id === "auto") return text("The recorder detects the project from clues again.");
      return text(confirmed ? `Now recording into project ${id}.` : `Sent: switch to project ${id}. The app hasn't confirmed yet.`);
    }),
  );

  server.registerTool(
    "assign_project",
    {
      title: "Assign work to a project",
      description:
        "Correct which project a stretch of a session belongs to (or the whole session). Times are ISO or local HH:MM on the session's day. " +
        'Use project "" to un-assign. Stored in the session\'s assign.json; the recording itself is not changed.',
      inputSchema: {
        session_id: z.string().describe('Session id, or "latest"'),
        project: z.string().describe("Project id (\"\" = none)"),
        from: z.string().optional().describe("Start (ISO, or HH:MM local); default: session start"),
        to: z.string().optional().describe("End, exclusive (ISO, or HH:MM local); default: session end"),
      },
    },
    safe(({ session_id, project, from, to }: { session_id: string; project: string; from?: string; to?: string }) => {
      const id = resolveSessionId(session_id);
      const p = project.trim();
      if (p && !loadProjects().some((x) => x.id === p)) return fail(`No project "${p}". Create it first with create_project.`);
      const day = readSessionMeta(id).started_at ?? new Date().toISOString();
      const f = from ? toIso(from, day) : undefined;
      const t = to ? toIso(to, day) : undefined;
      addAssignment(id, { project: p, ...(f ? { from: f } : {}), ...(t ? { to: t } : {}) });
      const span = f || t ? `${f ? formatClock(f) : "start"}–${t ? formatClock(t) : "end"}` : "the whole session";
      return text(`Session ${id}, ${span}: ${p ? `project ${p}` : "no project"}.`);
    }),
  );

  server.registerTool(
    "get_day_activity",
    {
      title: "Get a day's activity by project",
      description:
        "Every session on one local day, split by project: time per project, then a condensed timeline per project " +
        '(voice/typed notes included). Work matching no project is listed as "Not sorted yet". Use it to write daily project capsules.',
      inputSchema: {
        date: z.string().optional().describe("YYYY-MM-DD, local (default today)"),
        project: z.string().optional().describe('Only this project id ("" = only unsorted work)'),
        max_chars: z.number().int().min(500).max(1_000_000).optional().describe(`Maximum length (default ${DEFAULT_MAX_CHARS})`),
      },
      annotations: { readOnlyHint: true },
    },
    safe(({ date, project, max_chars }: { date?: string; project?: string; max_chars?: number }) => {
      const d = loadDay(checkDay(date));
      return text(condenseDay(d.day, d.events, d.sessionIds, { project, maxChars: max_chars }));
    }),
  );

  server.registerTool(
    "prepare_day",
    {
      title: "Prepare a day's capsules",
      description:
        "Start here to make a day's capsules: the steps to follow, the projects to write up (with their sessions and which capsules " +
        "exist), the current project capsules to merge into, and the day's activity split by project — all in one call.",
      inputSchema: {
        date: z.string().optional().describe("YYYY-MM-DD, local (default today)"),
        max_chars: z.number().int().min(2000).max(1_000_000).optional().describe(`Maximum length (default ${DEFAULT_MAX_CHARS})`),
      },
      annotations: { readOnlyHint: true },
    },
    safe(({ date, max_chars }: { date?: string; max_chars?: number }) => text(prepareDay(checkDay(date), max_chars))),
  );

  server.registerTool(
    "finish_day",
    {
      title: "Check a day's capsules are done",
      description:
        "End here: checks that every project worked on that day has its daily capsule and an updated project capsule, and nothing " +
        "is left unsorted. Lists exactly what's missing, or a table of what was made.",
      inputSchema: { date: z.string().optional().describe("YYYY-MM-DD, local (default today)") },
      annotations: { readOnlyHint: true },
    },
    safe(({ date }: { date?: string }) => text(finishDay(checkDay(date)).text)),
  );

  // ---------- sessions ----------

  server.registerTool(
    "list_sessions",
    {
      title: "List sessions",
      description: "Recorded sessions, newest first: id, start, end, duration, where the text came from (Accessibility or OCR) and the apps used most.",
      inputSchema: { limit: z.number().int().min(1).max(500).optional().describe("How many to list (default 20)") },
      annotations: { readOnlyHint: true },
    },
    safe(({ limit }: { limit?: number }) => {
      const ids = listSessionIds();
      if (!ids.length) return text(noSessionsMessage());
      const shown = ids.slice(0, limit ?? 20);
      const rows = shown.map((id) => {
        const s = summarizeSession(id);
        const start = s.started_at ? formatLocalDateTime(s.started_at) : "?";
        const end = s.ended_at ? formatLocalDateTime(s.ended_at).slice(11) : "still open";
        const apps = s.top_apps.map((a) => `${a.app} ${formatDuration(a.ms)}`).join(", ") || "no activity";
        const notes = s.notes ? ` · ${s.notes} ${s.notes === 1 ? "note" : "notes"}` : "";
        return `- ${id} · ${start} → ${end} · ${formatDuration(s.duration_ms)} · ${s.events} samples${textSources(s.sources)}${notes} · ${apps}`;
      });
      const more = ids.length > shown.length ? `\n(${ids.length - shown.length} older sessions not shown)` : "";
      return text(`Sessions (local time):\n${rows.join("\n")}${more}`);
    }),
  );

  server.registerTool(
    "get_session_activity",
    {
      title: "Get session activity",
      description:
        "A condensed timeline of one session for writing a capsule: segments grouped by app and window with HH:MM spans, " +
        "screen text de-duplicated and cleaned, trimmed to max_chars with a fair share per segment. Use session_id \"latest\" for the newest.",
      inputSchema: {
        session_id: z.string().describe('Session id like 20261019-100103, or "latest"'),
        max_chars: z
          .number()
          .int()
          .min(500)
          .max(1_000_000)
          .optional()
          .describe(`Maximum length of the result (default ${DEFAULT_MAX_CHARS})`),
      },
      annotations: { readOnlyHint: true },
    },
    safe(({ session_id, max_chars }: { session_id: string; max_chars?: number }) => {
      const id = resolveSessionId(session_id);
      return text(condenseActivity(readSessionMeta(id), readEvents(id), { maxChars: max_chars ?? DEFAULT_MAX_CHARS }));
    }),
  );

  // ---------- capsules ----------

  server.registerTool(
    "save_capsule",
    {
      title: "Save capsule",
      description:
        `Save a capsule you wrote. Pass the Markdown body (sections: ${REQUIRED_SECTIONS.map((s) => `## ${s}`).join(", ")}); ` +
        "front matter (author, date, sessions, apps) is added for you. Never overwrites: a second capsule with the same title gets -2. " +
        'For projects: kind "daily" (needs project + date) saves <project>-<date>; kind "project" replaces the project\'s one growing capsule (the old version is kept in .history).',
      inputSchema: {
        title: z.string().min(1).describe("Short title, e.g. 'Pricing page redesign'"),
        markdown: z.string().min(1).describe("The capsule body in Markdown, without front matter"),
        session_ids: z.array(z.string()).optional().describe("Sessions this capsule was written from"),
        project: z.string().optional().describe("Project id (see list_projects)"),
        kind: z.enum(["daily", "project"]).optional().describe('"daily": one day of a project; "project": the growing capsule'),
        date: z.string().optional().describe("Local day YYYY-MM-DD (daily capsules)"),
      },
    },
    safe((args: { title: string; markdown: string; session_ids?: string[]; project?: string; kind?: "daily" | "project"; date?: string }) => {
      const r = saveCapsule(args);
      const warn = r.warnings.length ? `\nNotes:\n${r.warnings.map((w) => `- ${w}`).join("\n")}` : "";
      return text(
        `Saved "${r.front.title}" to ${r.path}\nName: ${r.slug}${warn}\n` +
          `To share it, call export_capsule with name "${r.slug}" (copies it to the Desktop, ready to email).`,
      );
    }),
  );

  server.registerTool(
    "list_capsules",
    {
      title: "List capsules",
      description: "Saved and imported capsules: title, author, created, path.",
      inputSchema: {},
      annotations: { readOnlyHint: true },
    },
    safe(() => {
      const all = listCapsules();
      if (!all.length) return text("No capsules yet. Record a session, then write one with the make_capsule prompt and save_capsule.");
      return text(
        all
          .map(
            (c) =>
              `- ${c.title}${c.project ? ` · project ${c.project}${c.kind === "project" ? " (whole project)" : c.date ? ` (${c.date})` : ""}` : ""}` +
              ` · by ${c.author || "?"} · ${c.created || "?"} · name: ${c.slug} · ${c.path}`,
          )
          .join("\n"),
      );
    }),
  );

  server.registerTool(
    "get_capsule",
    {
      title: "Get capsule",
      description:
        "The full text of a capsule, by name (slug), file name or title. It's context for you: read its \"For Claude\" section first and follow it.",
      inputSchema: { name: z.string().min(1) },
      annotations: { readOnlyHint: true },
    },
    safe(({ name }: { name: string }) => text(readCapsule(name).text)),
  );

  server.registerTool(
    "import_capsule",
    {
      title: "Import capsule",
      description: `Copy a capsule someone sent you (a ${CAPSULE_EXT} file) into your capsules. Checks it is a valid capsule first.`,
      inputSchema: { path: z.string().min(1).describe("Path to the received file, e.g. ~/Downloads/pricing.capsule.md") },
    },
    safe(({ path }: { path: string }) => {
      const r = importCapsule(path);
      if (r.already) return text(`"${r.title}" is already in your capsules: ${r.path}\nName: ${r.slug}`);
      return text(`Imported "${r.title}" to ${r.path}\nName: ${r.slug}. Read it with get_capsule.`);
    }),
  );

  server.registerTool(
    "export_capsule",
    {
      title: "Export capsule",
      description: "Copy a capsule to the Desktop (default) or a path, ready to email or send. Never overwrites an existing file.",
      inputSchema: {
        name: z.string().min(1),
        to_path: z.string().optional().describe("Folder or file path; default ~/Desktop"),
      },
    },
    safe(({ name, to_path }: { name: string; to_path?: string }) => {
      const out = exportCapsule(name, to_path);
      return text(`Exported to ${out}\nAttach that file to an email or message. The receiver's Claude can load it with import_capsule.`);
    }),
  );

  // ---------- prompt ----------

  server.registerPrompt(
    "make_capsule",
    {
      title: "Make a capsule",
      description: "Write a capsule from a recorded session.",
      argsSchema: { session_id: z.string().describe('Session id, or "latest"') },
    },
    ({ session_id }) => ({
      messages: [{ role: "user", content: { type: "text", text: makeCapsulePrompt(session_id) } }],
    }),
  );

  server.registerPrompt(
    "make_day_capsules",
    {
      title: "Make the day's project capsules",
      description: "Split a day's work by project and write a daily capsule per project, then update each project's capsule.",
      argsSchema: { date: z.string().optional().describe("YYYY-MM-DD, local (default today)") },
    },
    ({ date }) => ({
      messages: [{ role: "user", content: { type: "text", text: makeDayCapsulesPrompt(date) } }],
    }),
  );

  server.registerPrompt(
    "continue_from_capsule",
    {
      title: "Continue from a capsule",
      description: "Load a capsule someone sent (or one of yours) and carry on the work from where it stands.",
      argsSchema: { capsule: z.string().describe("Capsule name, title, or the path of a received .capsule.md file") },
    },
    ({ capsule }) => ({
      messages: [{ role: "user", content: { type: "text", text: continueFromCapsulePrompt(capsule) } }],
    }),
  );

  // ---------- resources ----------

  server.registerResource(
    "capsule",
    new ResourceTemplate("capsule://{slug}", {
      list: () => ({
        resources: listCapsules().map((c) => ({
          uri: `capsule://${c.slug}`,
          name: c.slug,
          title: c.title,
          description: `${c.title}${c.author ? ` by ${c.author}` : ""}`,
          mimeType: "text/markdown",
        })),
      }),
      complete: { slug: (v) => listCapsules().map((c) => c.slug).filter((s) => s.startsWith(v)) },
    }),
    { title: "Capsules", description: "Saved capsules as Markdown", mimeType: "text/markdown" },
    (uri, vars) => {
      const slug = decodeURIComponent(String(Array.isArray(vars.slug) ? vars.slug[0] : vars.slug));
      const info = findCapsule(slug);
      return { contents: [{ uri: uri.href, mimeType: "text/markdown", text: fs.readFileSync(info.path, "utf8") }] };
    },
  );

  return server;
}

function since(iso: string | null): string {
  return iso ? ` (since ${formatLocalDateTime(iso)})` : "";
}

/** " (text: 80% accessibility, 20% OCR)", or "" for old sessions without `src`. */
function textSources(src: { ax: number; ocr: number }): string {
  const n = src.ax + src.ocr;
  if (!n) return "";
  if (!src.ocr) return " (text: accessibility)";
  if (!src.ax) return " (text: OCR)";
  const ax = Math.round((src.ax / n) * 100);
  return ` (text: ${ax}% accessibility, ${100 - ax}% OCR)`;
}

function latestHint(): string {
  const id = listSessionIds()[0];
  return id ? ` Latest session: ${id}.` : "";
}



/** ISO as is, or local "HH:MM" on the local day of `dayIso`. */
function toIso(v: string, dayIso: string): string {
  const s = v.trim();
  const hm = /^(\d{1,2}):(\d{2})$/.exec(s);
  if (hm) {
    const d = new Date(dayIso);
    d.setHours(Number(hm[1]), Number(hm[2]), 0, 0);
    return d.toISOString().replace(/\.\d{3}Z$/, "Z");
  }
  if (Number.isNaN(Date.parse(s))) throw new Error(`"${v}" is not a time (use HH:MM or ISO).`);
  return new Date(s).toISOString().replace(/\.\d{3}Z$/, "Z");
}

