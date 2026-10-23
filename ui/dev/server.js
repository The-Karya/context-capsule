// context-capsule — ui: dev server — serves ui/ with a fake host (BRIDGE.md) over a data folder; writes stay in memory.
// Usage: bun ui/dev/server.js [dataFolder=/tmp/mac-ref-data] [--port 5174]
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join, normalize, resolve } from "node:path";

const argv = process.argv.slice(2);
const portAt = argv.indexOf("--port");
const PORT = portAt >= 0 ? Number(argv[portAt + 1]) : 5174;
const DATA = resolve(argv.find((a, i) => !a.startsWith("--") && argv[i - 1] !== "--port") ?? "/tmp/mac-ref-data");
const UI = resolve(import.meta.dir, "..");

const MIME = {
  ".html": "text/html; charset=utf-8", ".css": "text/css; charset=utf-8", ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8", ".svg": "image/svg+xml", ".png": "image/png", ".woff2": "font/woff2",
  ".json": "application/json; charset=utf-8", ".md": "text/markdown; charset=utf-8",
};

// MARK: the data folder, with an in-memory overlay

const overlay = new Map(); // path -> text
const deleted = new Set(); // paths (files, or "sessions/<id>" folders)
const isDeleted = (p) => [...deleted].some((d) => p === d || p.startsWith(d + "/"));

const ALLOWED = [
  /^(config|projects|state)\.json$/,
  /^sessions\/[^/]+\/(events\.jsonl|gaps\.jsonl|assign\.json|session\.json)$/,
  /^capsules\/[^/]+\.md$/,
];
function safe(p) {
  if (typeof p !== "string" || !p || p.startsWith("/") || p.includes("\\") || /^[a-zA-Z]:/.test(p)) return false;
  return !p.split("/").some((s) => s === ".." || s === "." || s === "");
}
const allowed = (p) => safe(p) && ALLOWED.some((re) => re.test(p));

function read(p) {
  if (isDeleted(p)) return null;
  if (overlay.has(p)) return overlay.get(p);
  try { return readFileSync(join(DATA, p), "utf8"); } catch { return null; }
}
function write(p, text) { deleted.delete(p); overlay.set(p, text); }

function list(dir) {
  let names = [];
  try { names = readdirSync(join(DATA, dir), { withFileTypes: true }); } catch {}
  const want = dir === "sessions" ? (d) => d.isDirectory() : (d) => d.isFile() && d.name !== ".history";
  return names.filter(want).map((d) => d.name).filter((n) => !n.startsWith(".") && !isDeleted(`${dir}/${n}`)).sort();
}

const config = () => { try { return JSON.parse(read("config.json") ?? "{}"); } catch { return {}; } };
function setConfig(key, value) {
  const c = config();
  if (value === null || value === undefined) delete c[key]; else c[key] = value; // existing keys keep their order
  write("config.json", JSON.stringify(c, null, 2));
}

// MARK: fake host state

const params = (query) => new URLSearchParams(query ?? "");
let clipboard = "";
const setupDone = new Set();
const clients = [
  { id: "claude", name: "Claude", installed: false, connected: false, how: "extension" },
  { id: "cursor", name: "Cursor", installed: false, connected: false, how: "config" },
];
const state = {
  recording: false, picking: false, listening: false, started_at: null, session_id: null, events: 0,
  project: null, project_by: null, screens: 1, can_add_screen: false, several_screens: false,
  update: { line: "", ready: false, working: false },
};
const nowISO = () => new Date().toISOString().replace(/\.\d+Z$/, "Z");

// MARK: events (server-sent)

const streams = new Set();
const enc = new TextEncoder();
function emit(event, payload) {
  const chunk = enc.encode(`data: ${JSON.stringify({ event, payload })}\n\n`);
  for (const c of streams) { try { c.enqueue(chunk); } catch { streams.delete(c); } }
  console.log(`  ← ${event}`);
}
function events(query) {
  if (params(query).get("recording") === "1" && !state.recording) {
    Object.assign(state, { recording: true, started_at: nowISO(), session_id: list("sessions").at(-1) ?? null, events: 12 });
  }
  let me, ping;
  const body = new ReadableStream({
    start(c) {
      me = c; streams.add(c);
      c.enqueue(enc.encode(`data: ${JSON.stringify({ event: "state", payload: state })}\n\n`));
      ping = setInterval(() => { try { c.enqueue(enc.encode(": ping\n\n")); } catch {} }, 15000);
    },
    cancel() { streams.delete(me); clearInterval(ping); },
  });
  return new Response(body, { headers: { "content-type": "text/event-stream", "cache-control": "no-store" } });
}

// MARK: methods

class Refuse extends Error {}
const refuse = (why) => { throw new Refuse(why); };

function deleteLines(session, lines) {
  const path = `sessions/${session}/events.jsonl`;
  const text = read(path);
  if (text === null) refuse("gone");
  const parts = text.split(/(?<=\n)/); // keeps each line's ending (CRLF untouched)
  const gone = new Set();
  for (const { t, line } of lines) {
    const i = parts.findIndex((raw, k) => {
      if (gone.has(k) || raw.replace(/\r?\n$/, "") !== line) return false;
      try { return JSON.parse(line).t === t; } catch { return false; }
    });
    if (i < 0) refuse("gone");
    gone.add(i);
  }
  write(path, parts.filter((_, k) => !gone.has(k)).join(""));
}

const methods = {
  "host.hello": (_, q) => {
    const platform = q.get("platform") ?? "mac";
    setTimeout(() => emit("state", state), 0);
    return {
      platform, version: "1.4.2", locale: q.get("locale") ?? "en-US", hour12: null, firstWeekday: null,
      userName: "Saachi Badal", dark: q.get("dark") === "1", material: q.get("material") === "glass" ? "glass" : "none", firstRun: q.get("firstRun") === "1", page: null,
      voiceLanguages: [
        { id: "en-US", name: "English (United States)" }, { id: "en-GB", name: "English (United Kingdom)" },
        { id: "hi-IN", name: "Hindi (India)" }, { id: "es-ES", name: "Spanish (Spain)" },
        { id: "fr-FR", name: "French (France)" }, { id: "de-DE", name: "German (Germany)" },
      ],
      voiceLanguageDefault: platform === "mac" ? "Same as my Mac" : "Same as my computer",
    };
  },
  "data.list": ({ dir }) => (dir === "sessions" || dir === "capsules" ? list(dir) : refuse(`can't list ${dir}`)),
  "data.read": ({ files }) => {
    if (!Array.isArray(files)) refuse("files must be a list");
    const bad = files.find((f) => !allowed(f));
    if (bad !== undefined) refuse(`not allowed: ${bad}`);
    return Object.fromEntries(files.map((f) => [f, read(f)]));
  },
  "config.set": ({ key, value }) => {
    if (typeof key !== "string" || !key) refuse("no key");
    setConfig(key, value);
    emit("data-changed", { what: ["config"] });
    return {};
  },
  "projects.save": ({ text }) => {
    if (typeof text !== "string") refuse("no text");
    try { JSON.parse(text); } catch { refuse("that isn't valid projects.json"); }
    write("projects.json", text);
    emit("data-changed", { what: ["projects"] });
    return {};
  },
  "timeline.delete": ({ session, lines }) => {
    if (typeof session !== "string" || !safe(session) || session.includes("/")) refuse("bad session");
    if (state.recording && state.session_id === session) refuse("recording");
    if (lines == null) {
      if (isDeleted(`sessions/${session}`) || !existsSync(join(DATA, "sessions", session))) refuse("gone");
      deleted.add(`sessions/${session}`);
    } else {
      if (!Array.isArray(lines)) refuse("lines must be a list");
      deleteLines(session, lines);
    }
    emit("data-changed", { what: ["sessions"] });
    return {};
  },
  recorder: ({ action, id }) => {
    if (action === "toggle") {
      Object.assign(state, state.recording
        ? { recording: false, started_at: null, session_id: null, events: 0 }
        : { recording: true, started_at: nowISO(), session_id: null, events: 0 });
    } else if (action === "voice") state.listening = !state.listening;
    else if (action === "project") Object.assign(state, { project: id ?? null, project_by: id ? "you" : null });
    else if (action !== "add-screen" && action !== "new-project") refuse(`unknown action ${action}`);
    emit("state", state);
    return {};
  },
  "shortcuts.set": ({ record, voice }) => {
    if ([record, voice].some((k) => String(k).includes("Ctrl+Alt+T"))) refuse("taken");
    setConfig("hotkeys", record === "Alt+C" && voice === "Alt+V" ? null : { record, voice });
    return {};
  },
  "shortcuts.listen": () => ({}),
  "setup.status": (_, q) => {
    const platform = q.get("platform") ?? "mac";
    const ids = platform === "mac" ? ["read", "voice", "dictation", "ai", "login"] : ["voice", "ai", "login"];
    const fresh = q.get("firstRun") === "1";
    const connected = clients.some((c) => c.connected);
    const done = (id) => setupDone.has(id) || (id === "ai" ? connected : !fresh);
    return { steps: ids.map((id) => ({ id, done: done(id) })), screenRecording: true };
  },
  "setup.do": ({ id }) => { setupDone.add(id); return {}; },
  "mcp.run": ({ command, id, date }) => {
    // The program's stdout text, as the real hosts pass it on (yourai.parse* read it).
    const out = (v) => JSON.stringify(v);
    if (command === "clients") return out(clients);
    if (command === "connect" || command === "disconnect") {
      const c = clients.find((c) => c.id === id);
      if (!c) refuse(`unknown client ${id}`);
      c.connected = command === "connect";
      emit("data-changed", { what: ["clients"] });
      return out({ ok: true, message: "" });
    }
    if (command === "day") {
      const text = `Here is what I did on ${date ?? "2026-10-23"} (from Context Capsule): pricing page work for Website, ` +
        `the Acme Q3 client report, and Context Capsule bridge notes. Please make a capsule for each project.`;
      return out({ text, chars: [...text].length });
    }
    if (command === "save") return out({ saved: [], refused: [] });
    refuse(`unknown command ${command}`);
  },
  "ai.open": () => ({ opened: false }),
  "clipboard.read": () => ({ text: clipboard }),
  "clipboard.write": ({ text }) => { clipboard = String(text ?? ""); return {}; },
  "open.url": ({ url }) => (/^https?:\/\//.test(url ?? "") ? {} : refuse("only http(s) links")),
  "capsule.open": ({ file }) => (allowed(file) && file.startsWith("capsules/") ? {} : refuse(`not allowed: ${file}`)),
  "capsule.share": ({ file }) => (allowed(file) && file.startsWith("capsules/") ? { done: true } : refuse(`not allowed: ${file}`)),
  "capsule.pick": () => null,
  "apps.pick": () => null,
  "apps.names": ({ ids }) => Object.fromEntries((ids ?? []).map((id) => [id, id])),
  updates: () => ({}),
};

async function bridge(req) {
  let body;
  try { body = await req.json(); } catch { return Response.json({ ok: false, error: "bad request" }); }
  const { method, args = {}, query = "" } = body ?? {};
  const short = JSON.stringify(args ?? {});
  console.log(`→ ${method} ${short.length > 120 ? short.slice(0, 117) + "..." : short}`);
  const fn = methods[method];
  if (!fn) return Response.json({ ok: false, error: `unknown method ${method}` });
  try {
    return Response.json({ ok: true, value: (await fn(args ?? {}, params(query))) ?? null });
  } catch (e) {
    if (!(e instanceof Refuse)) console.error(e);
    console.log(`  ✗ ${e.message}`);
    return Response.json({ ok: false, error: e.message });
  }
}

// MARK: static files

function file(pathname) {
  const rel = normalize(decodeURIComponent(pathname === "/" ? "/index.html" : pathname)).replace(/^\/+/, "");
  const full = resolve(UI, rel);
  if (!full.startsWith(UI + "/") || !existsSync(full) || !statSync(full).isFile()) {
    return new Response("not found", { status: 404 });
  }
  const headers = { "content-type": MIME[extname(full)] ?? "application/octet-stream", "cache-control": "no-store" };
  if (rel === "index.html") {
    const html = readFileSync(full, "utf8").replace(/<head(\s[^>]*)?>/i, (m) => `${m}\n<script src="/dev/shim.js"></script>`);
    return new Response(html, { headers });
  }
  return new Response(Bun.file(full), { headers });
}

Bun.serve({
  port: PORT,
  idleTimeout: 0, // keep /events open
  fetch(req) {
    const url = new URL(req.url);
    if (url.pathname === "/bridge" && req.method === "POST") return bridge(req);
    if (url.pathname === "/events") return events(url.search);
    return file(url.pathname);
  },
});
console.log(`context-capsule ui dev: http://localhost:${PORT}/  (data: ${DATA}, writes stay in memory)`);
