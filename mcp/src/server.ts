// karya-capsule — mcp: the MCP server (tools, the make_capsule prompt, capsule:// resources)
import fs from "node:fs";
import { McpServer, ResourceTemplate } from "@modelcontextprotocol/sdk/server/mcp.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { z } from "zod";
import { DEFAULT_MAX_CHARS, condenseActivity } from "./activity.js";
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
  APP_NOT_RUNNING,
  PICKER_OPEN,
  appRunning,
  describeStartError,
  readState,
  startAndWait,
  waitForState,
  writeControl,
} from "./control.js";
import { capsuleHome } from "./paths.js";
import {
  formatDuration,
  formatLocalDateTime,
  listSessionIds,
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
  const server = new McpServer(
    { name: "karya-capsule", version: VERSION },
    {
      instructions:
        "Karya Capsule records what the user does on screen (text only, on their Mac) and lets you turn a work session " +
        "into a capsule: one shareable Markdown file with what they worked on, the decisions and why. " +
        "Typical flow: start_recording → user works → stop_recording → get_session_activity → write the capsule " +
        "(see the make_capsule prompt) → save_capsule → export_capsule to share. Received capsules: import_capsule, then get_capsule.",
    },
  );

  // ---------- recording ----------

  server.registerTool(
    "start_recording",
    {
      title: "Start recording",
      description:
        "Ask the Capsule menu-bar app to start recording. The app opens the macOS picker; the user chooses the window " +
        "or screen to record, and recording starts after that (screen text every 3 s, nothing leaves the Mac). " +
        "Returns the session id once recording, or says the picker is waiting for the user.",
      inputSchema: {},
    },
    safe(async () => {
      const state = readState();
      if (!appRunning(state)) return fail(APP_NOT_RUNNING);
      if (state!.recording)
        return text(`Already recording session ${state!.session_id ?? "(unknown)"}${since(state!.started_at)}. Call stop_recording when done.`);
      const out = await startAndWait(startMs);
      switch (out.kind) {
        case "recording":
          return text(
            `Recording started: session ${out.state.session_id ?? "(unknown)"}${since(out.state.started_at)}.\n` +
              "Work as usual. Call stop_recording when you're done (or press ⌥⇧C).",
          );
        case "picking":
          return text(`${PICKER_OPEN}\nCall recording_status to check, and stop_recording when you're done.`);
        case "error":
          return fail(describeStartError(out.error));
        case "timeout":
          return text(
            "Sent the start command, but the Capsule app hasn't answered yet. Look for the macOS picker or the " +
              "menu-bar icon (a filled circle means recording), or call recording_status in a moment.",
          );
      }
    }),
  );

  server.registerTool(
    "stop_recording",
    {
      title: "Stop recording",
      description: "Ask the Capsule app to stop the current recording. Returns the session id to pass to get_session_activity.",
      inputSchema: {},
    },
    safe(async () => {
      const state = readState();
      if (!appRunning(state)) {
        return fail(
          "The Capsule app is not running, so nothing is being recorded." +
            latestHint() +
            "\nTo record again, open the app (`open -a Capsule`) and call start_recording.",
        );
      }
      if (!state!.recording) return text(`Not recording right now.${latestHint()}`);
      const id = state!.session_id;
      writeControl("stop");
      const { confirmed } = await waitForState((s) => !s.recording, stopMs);
      const next = id
        ? `\nNext: get_session_activity with session_id "${id}" (or use the make_capsule prompt) to write a capsule.`
        : "";
      if (!confirmed)
        return text(`Sent the stop command for session ${id ?? "(unknown)"}; the app hasn't confirmed yet.${next}`);
      return text(`Stopped. Session id: ${id ?? "(unknown)"}.${next}`);
    }),
  );

  server.registerTool(
    "recording_status",
    {
      title: "Recording status",
      description: "Is the Capsule app running, is the picker open, is it recording, and why the last start failed. Reads state.json.",
      inputSchema: {},
      annotations: { readOnlyHint: true },
    },
    safe(() => {
      const state = readState();
      if (!state)
        return text(`The Capsule app has not run yet on this Mac (no ${capsuleHome()}/state.json).\n${APP_NOT_RUNNING}`);
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
      }
      if (!running) lines.push("", APP_NOT_RUNNING);
      return text(lines.join("\n"));
    }),
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
        return `- ${id} · ${start} → ${end} · ${formatDuration(s.duration_ms)} · ${s.events} samples${textSources(s.sources)} · ${apps}`;
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
        "front matter (author, date, sessions, apps) is added for you. Never overwrites: a second capsule with the same title gets -2.",
      inputSchema: {
        title: z.string().min(1).describe("Short title, e.g. 'Pricing page redesign'"),
        markdown: z.string().min(1).describe("The capsule body in Markdown, without front matter"),
        session_ids: z.array(z.string()).optional().describe("Sessions this capsule was written from"),
      },
    },
    safe((args: { title: string; markdown: string; session_ids?: string[] }) => {
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
          .map((c) => `- ${c.title} · by ${c.author || "?"} · ${c.created || "?"} · name: ${c.slug} · ${c.path}`)
          .join("\n"),
      );
    }),
  );

  server.registerTool(
    "get_capsule",
    {
      title: "Get capsule",
      description: "The full text of a capsule, by name (slug), file name or title.",
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

export function makeCapsulePrompt(sessionId: string): string {
  const id = sessionId?.trim() || "latest";
  return `Write a capsule from my recorded work session "${id}".

1. Call get_session_activity with session_id "${id}". It is a timeline of what was on my screen: segments by app and window, with local HH:MM times and the screen text (read through Accessibility, or OCR when that wasn't possible, so expect some noise).
2. Write the capsule in Markdown with exactly these sections, in this order:

## Summary
2-4 sentences: what this work was and where it ended up.
## What I worked on
Bullets, in time order, naming the apps, documents and pages.
## Decisions
One bullet per decision: - **Decision** — why. *(seen in <App>, HH:MM)*
Every decision must cite the moment and app it came from in the timeline.
## Open questions
What is still unclear or undecided.
## Next steps
Concrete next actions.
## Sources
Documents, pages, tickets, URLs and files that appeared, with the app they were in.

Rules:
- Only use what the timeline shows. If you infer something, mark it as a guess: *(guess)*.
- Never include secrets: passwords, API keys, tokens, private personal data, or anything marked [redacted].
- Keep it under ~1500 words. Plain, clear language a teammate can read cold.
- Don't add front matter; it is added for you.

3. Show me the capsule and ask if I want changes. When I'm happy, call save_capsule with a short title, the Markdown, and session_ids ["${id}"]. Then offer export_capsule so I can send it.`;
}
