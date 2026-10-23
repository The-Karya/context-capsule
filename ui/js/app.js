// context-capsule — ui: the window — starts up, keeps what the pages show (files read through the host), the sidebar,
// the Today pill, light/dark, and which page is on show

import * as host from "./host.js";
import { h, icon, closePopups } from "./dom.js";
import { fillIcons } from "./icons.js";
import { makeDates, dayKey } from "./core/dates.js";
import * as Stats from "./core/stats.js";
import * as Caps from "./core/capsules.js";
import * as Proj from "./core/projects.js";
import * as Keys from "./core/shortcuts.js";
import * as AI from "./core/yourai.js";
import { thisComputerFor } from "./core/timeline.js";
import * as home from "./pages/home.js";
import * as timeline from "./pages/timeline.js";
import * as capsules from "./pages/capsules.js";
import * as projects from "./pages/projects.js";
import * as settings from "./pages/settings.js";
import * as help from "./pages/help.js";
import * as actions from "./actions.js";

const PAGES = { home, timeline, capsules, projects, settings, help };
const query = new URLSearchParams(location.search);

/** Everything the pages read. Updated from the host; pages call app.refresh() / app.render(). */
export const app = {
  hello: {},
  platform: "mac",
  dates: makeDates(),
  config: {},
  projects: [],
  capsules: [],
  sessionIds: [],
  state: {},
  clients: [],
  clientsLoaded: false,
  setup: { steps: [], screenRecording: true },
  today: { seconds: 0, byProject: [] },
  week: { seconds: 0, byProject: [] },
  streak: 0,
  weekCapsules: 0,
  ai: { dayLine: null, working: false, busy: null, note: null },
  page: "home",
  dark: false,
  systemDark: false,
  // Session files read once and kept (cleared when the host says sessions changed).
  files: new Map(),
  nowOffset: 0,
  started: Date.now(),
};

/** The page's clock (dev renders pin it with ?now=…). */
app.now = () => Date.now() + app.nowOffset;
app.thisComputer = () => thisComputerFor(app.platform);
app.keys = () => {
  const k = Keys.fromConfig(app.config, app.platform);
  return { record: Keys.format(k.record, app.platform), voice: Keys.format(k.voice, app.platform), raw: k };
};
app.choice = () => AI.resolve(app.config.ai, app.clients);
app.kind = () => AI.kindOf(app.choice());
app.aiName = () => AI.choiceName(app.kind(), app.clients);
app.projectName = (id) => (id ? Proj.nameOf(app.projects, id) : "Not sorted");
app.firstName = () => {
  const n = typeof app.config.author === "string" && app.config.author.trim() ? app.config.author : app.hello.userName || "";
  return n.trim().split(/\s+/)[0] || "";
};

// ---------- reading ----------

async function readJSON(path) {
  const r = await host.read([path]);
  try {
    return JSON.parse(r[path] ?? "null") ?? {};
  } catch {
    return {};
  }
}

app.loadConfig = async () => {
  const c = await readJSON("config.json");
  app.config = c && typeof c === "object" && !Array.isArray(c) ? c : {};
  applyLook();
};

app.loadProjects = async () => {
  const r = await host.read(["projects.json"]);
  app.projects = Proj.parse(r["projects.json"] ?? "");
};

app.loadCapsules = async () => {
  const names = (await host.list("capsules")).filter((n) => n.endsWith(".md"));
  const texts = await host.read(names.map((n) => "capsules/" + n));
  app.capsules = Caps.all(names.filter((n) => texts["capsules/" + n] != null).map((n) => ({ name: n, text: texts["capsules/" + n] })))
    .map((c) => ({ ...c, file: "capsules/" + c.name }));
};

/** Raw files of some sessions: { id: { events, gaps, assign, session } } (kept until they change). */
app.sessionFiles = async (ids) => {
  const need = [];
  for (const id of ids) {
    if (app.files.has(id)) continue;
    for (const f of ["events.jsonl", "gaps.jsonl", "assign.json", "session.json"]) need.push(`sessions/${id}/${f}`);
  }
  if (need.length) {
    const r = await host.read(need);
    for (const id of ids) {
      if (app.files.has(id)) continue;
      const p = (f) => r[`sessions/${id}/${f}`] ?? null;
      app.files.set(id, { events: p("events.jsonl"), gaps: p("gaps.jsonl"), assign: p("assign.json"), session: p("session.json") });
    }
  }
  const out = {};
  for (const id of ids) out[id] = app.files.get(id);
  return out;
};

app.loadSessions = async () => {
  app.sessionIds = await host.list("sessions");
};

/** Home's numbers: today, this week, the streak, capsules this week. */
app.loadStats = async () => {
  const now = app.now();
  const day = new Date(now);
  day.setHours(0, 0, 0, 0);
  const first = app.hello.firstWeekday || Stats.firstWeekday(app.hello.locale);
  const week = Stats.weekStart(now, first);
  const files = await app.sessionFiles(Stats.weekCandidates(app.sessionIds, Math.min(week, day.getTime())));
  app.today = Stats.totals(files, day.getTime(), now + 60_000, app.projects);
  app.week = Stats.totals(files, week, now + 60_000, app.projects);
  app.streak = Stats.streak(app.sessionIds, now);
  app.weekCapsules = Caps.weekCount(app.capsules, week);
};

app.loadClients = async () => {
  try {
    const out = await host.call("mcp.run", { command: "clients" });
    app.clients = AI.sorted(AI.parseClients(typeof out === "string" ? out : JSON.stringify(out)));
  } catch {
    app.clients = [{ id: "claude", name: "Claude", installed: false, connected: false, how: "extension" }];
  }
  app.clientsLoaded = true;
};

app.loadSetup = async () => {
  try {
    app.setup = await host.call("setup.status");
  } catch {}
};

// ---------- light and dark ----------

function applyLook() {
  const a = app.config.appearance;
  app.dark = a === "dark" ? true : a === "light" ? false : app.systemDark;
  document.documentElement.toggleAttribute("data-dark", app.dark);
}

/** "glass": the host draws the system material behind a transparent page (Mac, Windows 11); else flat fills. */
function applyMaterial(m) {
  if (m === "glass") document.documentElement.dataset.material = "glass";
  else delete document.documentElement.dataset.material;
}

const media = window.matchMedia("(prefers-color-scheme: dark)");
media.addEventListener?.("change", () => {
  if (app.hostDark == null) {
    app.systemDark = media.matches;
    applyLook();
  }
});

// ---------- pages ----------

let current = null;

/** Show a page (and remember it in the address, so Back works in the dev browser). */
app.go = (page, opts = {}) => {
  if (!PAGES[page]) page = "home";
  closePopups();
  const same = page === app.page && current;
  app.page = page;
  if (location.hash.slice(1) !== page) history.replaceState(null, "", "#" + page);
  for (const a of document.querySelectorAll(".side-row")) {
    if (a.dataset.page === page) a.setAttribute("aria-current", "page");
    else a.removeAttribute("aria-current");
  }
  if (!same) mount(opts);
  else current.show?.(opts);
};

function mount(opts) {
  const root = document.getElementById("page");
  current?.unmount?.();
  root.replaceChildren();
  root.scrollTop = 0;
  current = PAGES[app.page].mount(root, app, opts) || {};
  layout();
  pill();
}

/** Something changed: the page updates what it shows (`what`: "state", "sessions", "capsules", …). */
app.refresh = (what) => {
  pill();
  current?.refresh?.(what);
};

function layout() {
  const page = document.getElementById("page");
  const wide = page.clientWidth > 900;
  document.getElementById("window").classList.toggle("wide", wide);
  if (wide !== app.wide) {
    app.wide = wide;
    current?.refresh?.("layout");
  }
}
window.addEventListener("resize", layout);

// ---------- the pill: the live timer while contextualizing, else today's time ----------

function clock(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const p = (n) => String(n).padStart(2, "0");
  return `${Math.floor(s / 3600)}:${p(Math.floor(s / 60) % 60)}:${p(s % 60)}`;
}
app.clock = clock;
app.elapsed = () => {
  const t = Date.parse(app.state.started_at || "");
  return Number.isNaN(t) ? 0 : app.now() - t;
};

function pill() {
  const el = document.getElementById("pill");
  if (app.state.recording) {
    el.title = "Contextualizing";
    el.replaceChildren(h("span", { class: "dot", "aria-hidden": "true" }), h("span", { class: "tnum" }, clock(app.elapsed())));
    el.setAttribute("aria-label", "Contextualizing " + clock(app.elapsed()));
  } else {
    el.title = "Time contextualized today";
    el.removeAttribute("aria-label");
    el.replaceChildren(icon("clock"), h("span", null, "Today " + Stats.duration(app.today.seconds)));
  }
}
setInterval(() => {
  if (app.state.recording) {
    pill();
    current?.tick?.();
  }
}, 1000);

// ---------- start ----------

async function start() {
  const pinned = query.get("now");
  if (pinned) {
    const t = Date.parse(pinned);
    if (!Number.isNaN(t)) app.nowOffset = t - Date.now();
  }
  try {
    app.hello = (await host.call("host.hello")) || {};
  } catch (e) {
    document.getElementById("page").textContent = host.why(e);
    return;
  }
  app.platform = app.hello.platform || "mac";
  document.documentElement.dataset.platform = app.platform;
  document.documentElement.lang = app.hello.locale || "en";
  const hour12 = app.hello.hour12 == null ? undefined : app.hello.hour12;
  app.dates = makeDates({ locale: app.hello.locale || undefined, hour12 });
  if (typeof app.hello.dark === "boolean") {
    app.hostDark = app.hello.dark;
    app.systemDark = app.hello.dark;
  } else {
    app.systemDark = media.matches;
  }
  applyMaterial(app.hello.material);

  fillIcons();
  for (const a of document.querySelectorAll(".side-row")) {
    a.addEventListener("click", (e) => {
      e.preventDefault();
      app.go(a.dataset.page);
    });
  }
  sidebarKeys();

  await Promise.all([app.loadConfig(), app.loadProjects(), app.loadCapsules(), app.loadSessions(), app.loadSetup()]);
  await app.loadStats();

  host.on("state", (s) => {
    const was = app.state.recording;
    app.state = s || {};
    if (was !== app.state.recording) {
      app.files.clear();
      app.loadSessions().then(app.loadStats).then(() => app.refresh("sessions"));
    } else app.refresh("state");
  });
  host.on("appearance", (a) => {
    app.hostDark = !!a?.dark;
    app.systemDark = app.hostDark;
    applyLook();
  });
  host.on("material", (m) => applyMaterial(m?.material));
  host.on("navigate", (n) => app.go(n?.page || "home"));
  host.on("open-capsule", (c) => actions.continueFrom(app, c?.path || "", c?.text || ""));
  host.on("data-changed", async (d) => {
    const what = new Set(d?.what || []);
    if (what.has("config")) await app.loadConfig();
    if (what.has("projects")) await app.loadProjects();
    if (what.has("capsules")) await app.loadCapsules();
    if (what.has("sessions")) {
      const live = app.state.recording ? app.state.session_id : null;
      if (live) app.files.delete(live);
      else app.files.clear();
      await app.loadSessions();
    }
    if (what.has("clients")) await app.loadClients();
    if (what.has("sessions") || what.has("projects") || what.has("capsules")) await app.loadStats();
    for (const w of what) app.refresh(w);
  });
  // Home's numbers every minute, as on the Mac.
  setInterval(() => app.loadStats().then(() => app.refresh("stats")), 60_000);
  // The AI apps (Settings, Home's words) — a moment: ask in the background.
  app.loadClients().then(() => app.refresh("clients"));

  const first = query.get("page") || location.hash.slice(1) || app.hello.page || (app.hello.firstRun ? "settings" : "home");
  app.go(first, { open: query.get("open") });
  window.addEventListener("hashchange", () => app.go(location.hash.slice(1)));
  // Ready for renders once the page's own reads are done.
  setTimeout(async () => {
    await current?.ready;
    setTimeout(() => (document.documentElement.dataset.ready = "1"), 50);
  });
}

/** The sidebar is one list: ↑ ↓ move between pages, as in a native sidebar. */
function sidebarKeys() {
  const rows = [...document.querySelectorAll(".side-row")];
  for (const r of rows) {
    r.addEventListener("keydown", (e) => {
      const i = rows.indexOf(r);
      let j = -1;
      if (e.key === "ArrowDown") j = Math.min(rows.length - 1, i + 1);
      else if (e.key === "ArrowUp") j = Math.max(0, i - 1);
      else if (e.key === "Home") j = 0;
      else if (e.key === "End") j = rows.length - 1;
      if (j < 0) return;
      e.preventDefault();
      rows[j].focus();
      app.go(rows[j].dataset.page);
    });
  }
}

// ⌘, / Ctrl+, opens Settings; ⌘F / Ctrl+F the page's search field.
document.addEventListener("keydown", (e) => {
  const mod = app.platform === "mac" ? e.metaKey : e.ctrlKey;
  if (mod && e.key === ",") {
    e.preventDefault();
    app.go("settings");
  } else if (mod && e.key.toLowerCase() === "f") {
    const s = document.querySelector("#page input[type=search]");
    if (s) {
      e.preventDefault();
      s.focus();
      s.select();
    }
  }
});

// A capsule dropped on the window (in the dev browser; the hosts send "open-capsule").
document.addEventListener("dragover", (e) => e.preventDefault());
document.addEventListener("drop", async (e) => {
  e.preventDefault();
  const f = e.dataTransfer?.files?.[0];
  if (f) actions.continueFrom(app, f.name, await f.text());
});

export { dayKey };
start();
